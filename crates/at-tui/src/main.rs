mod api_client;
mod app;
mod command;
mod effects;
mod event;
mod tabs;
mod ui;
mod widgets;

use std::io::{self, BufRead, Write as _};
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{self as ct_event, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::App;

fn main() -> Result<()> {
    // Parse CLI args (simple, no clap dependency).
    let args: Vec<String> = std::env::args().collect();
    let offline = args.iter().any(|a| a == "--offline");
    let headless = args.iter().any(|a| a == "--headless");
    let api_base = args
        .iter()
        .position(|a| a == "--api")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| {
            at_core::lockfile::DaemonLockfile::read_valid()
                .map(|lock| lock.api_url())
                .unwrap_or_else(|| {
                    eprintln!("warning: no running daemon found, trying http://127.0.0.1:9090");
                    "http://127.0.0.1:9090".to_string()
                })
        });

    at_telemetry::logging::init_logging("at-tui", "warn");

    if headless {
        return run_headless(offline, &api_base);
    }

    // Set up panic hook to restore terminal on panic.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = restore_terminal();
        original_hook(panic_info);
    }));

    let result = run(offline, &api_base);

    restore_terminal()?;
    result
}

/// Spawn the background API refresh thread, returns a receiver channel.
fn spawn_refresh(offline: bool, api_base: &str) -> Option<flume::Receiver<api_client::AppData>> {
    if offline {
        return None;
    }
    let (tx, rx) = flume::unbounded::<api_client::AppData>();
    let base = api_base.to_string();
    std::thread::spawn(move || {
        let client = api_client::ApiClient::new(&base);
        loop {
            let data = client.fetch_all();
            if tx.send(data).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_secs(5));
        }
    });
    Some(rx)
}

/// Run the interactive TUI with the standard crossterm backend.
fn run(offline: bool, api_base: &str) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(offline);
    let data_rx = spawn_refresh(offline, api_base);

    loop {
        if let Some(ref rx) = data_rx {
            while let Ok(data) = rx.try_recv() {
                app.apply_data(data);
            }
        }

        terminal.draw(|frame| {
            ui::render(frame, &mut app);
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

/// Headless mode: reads JSON commands from stdin, outputs JSON to stdout.
/// No terminal rendering — pure state machine for agent automation.
///
/// Usage: `echo '{"cmd":"query_state"}' | at-tui --headless`
fn run_headless(offline: bool, api_base: &str) -> Result<()> {
    let mut app = App::new(offline);
    let data_rx = spawn_refresh(offline, api_base);

    // Emit initial state event
    emit_event(&serde_json::json!({
        "event": "started",
        "tabs": app::TAB_NAMES.len(),
        "offline": app.offline,
    }));

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        // Drain API data before processing command
        if let Some(ref rx) = data_rx {
            while let Ok(data) = rx.try_recv() {
                app.apply_data(data);
                emit_event(&serde_json::json!({
                    "event": "data_refreshed",
                    "agents": app.agents.len(),
                    "beads": app.beads.len(),
                }));
            }
        }

        // Try JSON command first, then text command
        let cmd = command::parse_json_command(&line).or_else(|| command::parse_command(&line));

        match cmd {
            Some(cmd) => {
                let prev_tab = app.current_tab;
                let result = command::execute_command(&mut app, cmd);

                // Emit navigation events
                if app.current_tab != prev_tab {
                    emit_event(&serde_json::json!({
                        "event": "tab_changed",
                        "tab": app.current_tab,
                        "tab_name": app::TAB_NAMES[app.current_tab],
                    }));
                }

                // Emit query result or ack
                if let Some(json_str) = result {
                    // Already JSON, print directly
                    println!("{}", json_str);
                    let _ = io::stdout().flush();
                } else {
                    emit_event(&serde_json::json!({"event": "ok"}));
                }
            }
            None => {
                emit_event(&serde_json::json!({
                    "event": "error",
                    "message": format!("unknown command: {}", line),
                }));
            }
        }

        if app.should_quit {
            emit_event(&serde_json::json!({"event": "quit"}));
            break;
        }
    }

    Ok(())
}

fn emit_event(value: &serde_json::Value) {
    if let Ok(s) = serde_json::to_string(value) {
        println!("{}", s);
        let _ = io::stdout().flush();
    }
}

/// GPU-accelerated backend via ratatui-wgpu.
///
/// Enable with: `cargo run -p at-tui --features gpu`
#[cfg(feature = "gpu")]
pub mod gpu_backend {
    pub use ratatui_wgpu::{Builder as WgpuBackendBuilder, Font};

    pub const DEFAULT_FONT_SIZE_PX: u32 = 16;
    pub const DEFAULT_DIMENSIONS: (u32, u32) = (1280, 720);
}

fn restore_terminal() -> Result<()> {
    disable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, LeaveAlternateScreen)?;
    Ok(())
}
