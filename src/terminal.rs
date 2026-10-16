use std::{io, time::Duration};

use crossterm::{
    cursor,
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        KeyCode, KeyEvent, KeyModifiers, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend, buffer::Buffer, layout::Position};
use tokio::sync::{mpsc, watch};

use crate::{
    error::Result,
    events::{AppEvent, TerminalEvent},
};

pub type Tui = Terminal<CrosstermBackend<io::Stdout>>;

struct Screen {
    buffer: Buffer,
    cursor: Option<Position>,
}

/// Publishes complete screens so a slow terminal can skip obsolete frames safely.
/// Ratatui's terminal diff and all stdout writes belong to the output thread.
pub struct TerminalOutput {
    screens: Option<watch::Sender<Option<Screen>>>,
    thread: Option<std::thread::JoinHandle<io::Result<()>>>,
}

impl TerminalOutput {
    pub fn start(guard: TerminalGuard, mut terminal: Tui) -> io::Result<Self> {
        let (screens, receiver) = watch::channel::<Option<Screen>>(None);
        let runtime = tokio::runtime::Handle::current();
        let thread = std::thread::Builder::new()
            .name("mud-terminal-output".into())
            .spawn(move || {
                let _guard = guard;
                consume_screens(&runtime, receiver, |screen| {
                    terminal.draw(|frame| paint_screen(frame, &screen))?;
                    Ok(())
                })
            })?;
        Ok(Self {
            screens: Some(screens),
            thread: Some(thread),
        })
    }

    pub fn submit(&mut self, buffer: Buffer, cursor: Option<Position>) -> io::Result<()> {
        let Some(sender) = &self.screens else {
            return Err(io::Error::other("terminal output has stopped"));
        };
        if sender.send(Some(Screen { buffer, cursor })).is_err() {
            // Join the completed worker to retain the actual draw/panic error.
            self.finish()?;
            return Err(io::Error::other("terminal output thread stopped"));
        }
        Ok(())
    }

    pub fn finish(&mut self) -> io::Result<()> {
        self.screens.take();
        match self.thread.take() {
            Some(thread) => thread
                .join()
                .map_err(|_| io::Error::other("terminal output thread panicked"))?,
            None => Ok(()),
        }
    }
}

fn consume_screens(
    runtime: &tokio::runtime::Handle,
    mut receiver: watch::Receiver<Option<Screen>>,
    mut draw: impl FnMut(Screen) -> io::Result<()>,
) -> io::Result<()> {
    while runtime.block_on(receiver.changed()).is_ok() {
        // Release the watch lock before any potentially slow I/O.
        let screen = receiver.borrow_and_update().as_ref().map(|screen| Screen {
            buffer: screen.buffer.clone(),
            cursor: screen.cursor,
        });
        if let Some(screen) = screen {
            draw(screen)?;
        }
    }
    Ok(())
}

fn paint_screen(frame: &mut ratatui::Frame, screen: &Screen) {
    let area = frame.area().intersection(screen.buffer.area);
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            frame.buffer_mut()[(x, y)] = screen.buffer[(x, y)].clone();
        }
    }
    if let Some(cursor) = screen.cursor.filter(|p| area.contains(*p)) {
        frame.set_cursor_position(cursor);
    }
}

impl Drop for TerminalOutput {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

pub struct TerminalGuard {
    mouse: bool,
    keyboard_enhancement: bool,
}

impl TerminalGuard {
    pub fn enter(mouse: bool) -> Result<(Self, Tui)> {
        enable_raw_mode()?;
        // Own restoration before any fallible setup after raw mode is enabled.
        let mut guard = Self {
            mouse,
            keyboard_enhancement: false,
        };
        let mut stdout = io::stdout();
        if mouse {
            execute!(
                stdout,
                EnterAlternateScreen,
                EnableMouseCapture,
                cursor::Hide
            )?;
        } else {
            execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
        }
        // Native Windows input does not emit Paste events in Crossterm 0.28.
        if cfg!(unix) {
            execute!(stdout, EnableBracketedPaste)?;
        }
        // Unsupported Unix terminals ignore this request. Do not query support:
        // Crossterm 0.28 can wait indefinitely for a partial query response.
        // Its native Windows backend does not implement these commands.
        if cfg!(unix) {
            // Restore even if the write succeeds only partially.
            guard.keyboard_enhancement = true;
            execute!(stdout, PushKeyboardEnhancementFlags(keyboard_flags()))?;
        }
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        Ok((guard, terminal))
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        if cfg!(unix) {
            let _ = execute!(stdout, DisableBracketedPaste);
        }
        // Keyboard stacks belong to each screen: pop before leaving alternate screen.
        if self.keyboard_enhancement {
            let _ = execute!(stdout, PopKeyboardEnhancementFlags);
        }
        if self.mouse {
            let _ = execute!(
                stdout,
                cursor::Show,
                DisableMouseCapture,
                LeaveAlternateScreen
            );
        } else {
            let _ = execute!(stdout, cursor::Show, LeaveAlternateScreen);
        }
    }
}

fn keyboard_flags() -> KeyboardEnhancementFlags {
    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
        | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
        | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
        | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
}

fn normalize_terminal_key(mut key: KeyEvent) -> KeyEvent {
    // Crossterm 0.28 substitutes Kitty's shifted alternate character and drops
    // SHIFT, even with CONTROL set. Restore ASCII letter chords before they can
    // accidentally match a different macro (Ctrl+Shift+A versus Ctrl+A).
    // Config parsing remains separate: the spelling Ctrl+A still means Ctrl+a.
    if cfg!(unix)
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && let KeyCode::Char(c) = key.code
        && c.is_ascii_uppercase()
    {
        key.code = KeyCode::Char(c.to_ascii_lowercase());
        key.modifiers.insert(KeyModifiers::SHIFT);
    }
    key
}

pub fn spawn_terminal_events(
    timeout: Duration,
    tx: mpsc::Sender<AppEvent>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        while !tx.is_closed() {
            match crossterm::event::poll(timeout) {
                Ok(true) => match crossterm::event::read() {
                    Ok(crossterm::event::Event::Key(key)) => {
                        let key = normalize_terminal_key(key);
                        if tx
                            .blocking_send(AppEvent::Terminal(TerminalEvent::Key(key)))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(crossterm::event::Event::Paste(text)) => {
                        // Crossterm has already collected the payload; reject
                        // oversized values before placing them in our queue.
                        let event = if text.len() > crate::input::MAX_PASTE_BYTES {
                            TerminalEvent::PasteTooLarge
                        } else {
                            TerminalEvent::Paste(text)
                        };
                        if tx.blocking_send(AppEvent::Terminal(event)).is_err() {
                            break;
                        }
                    }
                    Ok(crossterm::event::Event::Mouse(mouse)) => {
                        if tx
                            .blocking_send(AppEvent::Terminal(TerminalEvent::Mouse(mouse)))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(crossterm::event::Event::Resize(width, height)) => {
                        if tx
                            .blocking_send(AppEvent::Terminal(TerminalEvent::Resize {
                                width,
                                height,
                            }))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                },
                Ok(false) => {}
                Err(_) => break,
            }
        }
    })
}

#[cfg(test)]
mod output_tests {
    use super::*;
    use ratatui::{backend::TestBackend, layout::Rect};

    fn screen(text: &str) -> Screen {
        Screen {
            buffer: Buffer::with_lines([text]),
            cursor: None,
        }
    }

    #[test]
    fn screens_are_clipped_when_the_terminal_shrinks() {
        let screen = screen("abcdef");
        for (width, height) in [(3, 1), (10, 2), (0, 0)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| paint_screen(frame, &screen)).unwrap();
            assert_eq!(
                terminal.backend().buffer().area,
                Rect::new(0, 0, width, height)
            );
            if width > 0 {
                assert_eq!(terminal.backend().buffer()[(0, 0)].symbol(), "a");
            }
        }
    }

    #[tokio::test]
    async fn slow_output_coalesces_frames_without_blocking_the_publisher() {
        let (tx, rx) = watch::channel(None);
        let runtime = tokio::runtime::Handle::current();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (seen_tx, mut seen_rx) = mpsc::unbounded_channel();
        let thread = std::thread::spawn(move || {
            let mut started = Some(started_tx);
            consume_screens(&runtime, rx, |screen| {
                seen_tx
                    .send(screen.buffer[(0, 0)].symbol().to_owned())
                    .unwrap();
                if let Some(started) = started.take() {
                    started.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                }
                Ok(())
            })
        });
        tx.send(Some(screen("A"))).unwrap();
        started_rx.await.unwrap();
        for _ in 0..1000 {
            tx.send(Some(screen("B"))).unwrap();
        }
        tx.send(Some(screen("Z"))).unwrap();
        release_tx.send(()).unwrap();
        assert_eq!(seen_rx.recv().await.as_deref(), Some("A"));
        assert_eq!(seen_rx.recv().await.as_deref(), Some("Z"));
        drop(tx);
        thread.join().unwrap().unwrap();
    }

    #[tokio::test]
    async fn output_failure_closes_the_publisher_and_preserves_the_error() {
        let (tx, rx) = watch::channel(None);
        let runtime = tokio::runtime::Handle::current();
        tx.send(Some(screen("A"))).unwrap();
        let task = tokio::task::spawn_blocking(move || {
            consume_screens(&runtime, rx, |_| Err(io::Error::other("broken terminal")))
        });
        assert_eq!(
            task.await.unwrap().unwrap_err().to_string(),
            "broken terminal"
        );
        assert!(tx.send(Some(screen("B"))).is_err());
    }

    #[tokio::test]
    async fn submit_preserves_the_worker_io_error_and_joins_it() {
        let (tx, rx) = watch::channel(None);
        let runtime = tokio::runtime::Handle::current();
        tx.send(Some(screen("A"))).unwrap();
        let thread = std::thread::spawn(move || {
            consume_screens(&runtime, rx, |_| Err(io::Error::other("broken pipe")))
        });
        tx.closed().await;
        let mut output = TerminalOutput {
            screens: Some(tx),
            thread: Some(thread),
        };
        let error = output.submit(screen("B").buffer, None).unwrap_err();
        assert_eq!(error.to_string(), "broken pipe");
        assert!(output.thread.is_none());
        output.finish().unwrap();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::macros::{MacroAction, MacroEngine, MacroRule};

    #[test]
    fn enhanced_ctrl_shift_letters_do_not_fire_unshifted_macros() {
        let mut macros = MacroEngine::default();
        for (key, command) in [("Ctrl+A", "plain"), ("Ctrl+Shift+A", "shifted")] {
            macros
                .add(MacroRule {
                    key: key.into(),
                    command: command.into(),
                    ..MacroRule::default()
                })
                .unwrap();
        }
        // Decoded forms of CSI 97;5u and CSI 97:65;6u respectively.
        for (code, command) in [('a', "plain"), ('A', "shifted")] {
            let key =
                normalize_terminal_key(KeyEvent::new(KeyCode::Char(code), KeyModifiers::CONTROL));
            assert_eq!(
                macros.action(key, false),
                Some(MacroAction::Execute(command))
            );
        }
        let key = KeyEvent::new(KeyCode::Char('A'), KeyModifiers::NONE);
        assert_eq!(normalize_terminal_key(key), key);
    }
}
