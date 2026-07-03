#![warn(missing_docs)]
//! WackDB Command Line Interface
//!
//! This binary provides an interactive demonstration of the WackDB storage engine.

mod commands;
mod state;
mod ui;

use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::{Backend, CrosstermBackend},
    style::{Color, Style},
    widgets::{Block, Borders},
};
use ratatui_textarea::{Input, Key, TextArea};
use std::path::Path;
use std::{error::Error, io};

use crate::commands::process_command;
use crate::state::AppState;
use crate::ui::render_ui;

use wackdb_buffer::buffer_pool::BufferPoolManager;
use wackdb_catalog::Catalog;
use wackdb_storage::disk_manager::BasicDiskManager;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the data directory (WackDB stores multiple files per relation, not a single file)
    #[arg(short, long, default_value = "wackdb_data")]
    data_dir: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    // Setup panic hook to restore terminal state in case of panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stderr(), crossterm::terminal::LeaveAlternateScreen);
        original_hook(panic_info);
    }));

    let args = Args::parse();

    // Setup Ratatui terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run TUI
    let res = run_app(&mut terminal, &args.data_dir);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}");
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, data_dir: &str) -> Result<(), Box<dyn Error>>
where
    <B as Backend>::Error: 'static,
{
    let mut catalog = Catalog::open(data_dir)?;
    let mut state = AppState::new(data_dir);

    state.log(format!(
        "Connected to WackDB Data Directory at: {}",
        data_dir
    ));
    state.log("Type \\help to see available commands.");

    let mut textarea = TextArea::default();
    textarea.set_block(
        Block::default()
            .borders(Borders::ALL)
            .title("Command Input (Enter to execute)")
            .style(Style::default().fg(Color::Cyan)),
    );

    let disk_manager = BasicDiskManager::<8192>::new(Path::new(data_dir))?;
    // Set buffer pool size to 64 frames (~512 KB) so that large insertions trigger LRU eviction
    let bpm = BufferPoolManager::new(64, disk_manager);
    
    let shared_bpm = std::sync::Arc::new(parking_lot::RwLock::new(bpm));
    
    let bpm_clone = shared_bpm.clone();
    let data_dir_clone = data_dir.to_string();
    std::thread::spawn(move || {
        if let Ok(rt) = tokio::runtime::Runtime::new() {
            rt.block_on(async {
                let _ = wackdb_web::start_server(bpm_clone, data_dir_clone, 3000).await;
            });
        }
    });

    loop {
        terminal.draw(|f| render_ui(f, &mut state, &textarea, &shared_bpm.read()))?;

        let event = event::read()?;
        match event.into() {
            Input { key: Key::Esc, .. } => return Ok(()),
            Input {
                key: Key::PageUp, ..
            } => {
                state.vertical_scroll = state.vertical_scroll.saturating_sub(10);
            }
            Input {
                key: Key::PageDown, ..
            } => {
                state.vertical_scroll = state.vertical_scroll.saturating_add(10);
            }
            Input {
                key: Key::Up,
                ctrl: true,
                ..
            } => {
                state.vertical_scroll = state.vertical_scroll.saturating_sub(1);
            }
            Input {
                key: Key::Down,
                ctrl: true,
                ..
            } => {
                state.vertical_scroll = state.vertical_scroll.saturating_add(1);
            }
            Input {
                key: Key::Enter, ..
            } => {
                let text = textarea.lines().join("\n");
                let text = text.trim();
                if text == "\\quit" || text == "\\exit" {
                    return Ok(());
                }
                if !text.is_empty() {
                    process_command(text, &mut state, &mut catalog, &mut shared_bpm.write())?;

                    textarea = TextArea::default();
                    textarea.set_block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title("Command Input (Enter to execute)")
                            .style(Style::default().fg(Color::Cyan)),
                    );
                }
            }
            input => {
                textarea.input(input);
            }
        }
    }
}
