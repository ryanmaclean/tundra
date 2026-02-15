mod app;
mod event;
mod tabs;
mod ui;
mod widgets;

use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{self as ct_event, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::app::App;

fn main() -> Result<()> {
    // Set up panic hook to restore terminal on panic.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = restore_terminal();
        original_hook(panic_info);
    }));

    at_telemetry::logging::init_logging("at-tui", "warn");

    let result = run();

    restore_terminal()?;
    result
}

fn run() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    loop {
        terminal.draw(|frame| {
            ui::render(frame, &app);
        })?;

        if ct_event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = ct_event::read()? {
                app.on_key(key);
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn restore_terminal() -> Result<()> {
    disable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, LeaveAlternateScreen)?;
    Ok(())
}
