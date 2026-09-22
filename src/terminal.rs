use std::{io, time::Duration};

use crossterm::{
    cursor,
    event::{
        DisableMouseCapture, EnableMouseCapture, KeyCode, KeyEvent, KeyModifiers,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::sync::mpsc;

use crate::{
    error::Result,
    events::{AppEvent, TerminalEvent},
};

pub type Tui = Terminal<CrosstermBackend<io::Stdout>>;

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

pub fn spawn_terminal_events(timeout: Duration, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || {
        loop {
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
    });
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
