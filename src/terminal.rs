use std::{io, time::Duration};

use crossterm::{
    cursor,
    event::{DisableMouseCapture, EnableMouseCapture},
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
}

impl TerminalGuard {
    pub fn enter(mouse: bool) -> Result<(Self, Tui)> {
        enable_raw_mode()?;
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
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        Ok((Self { mouse }, terminal))
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
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

pub fn spawn_terminal_events(timeout: Duration, tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || {
        loop {
            match crossterm::event::poll(timeout) {
                Ok(true) => match crossterm::event::read() {
                    Ok(crossterm::event::Event::Key(key)) => {
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
