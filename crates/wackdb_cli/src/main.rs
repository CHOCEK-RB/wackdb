#![warn(missing_docs)]
//! WackDB Command Line Interface
//!
//! This binary provides an interactive demonstration of the WackDB storage engine.

use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
};
use std::path::Path;
use std::{error::Error, io};
use wackdb_buffer::buffer_pool::BufferPoolManager;
use wackdb_storage::disk_manager::BasicDiskManager;

mod parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the database file
    #[arg(short, long, default_value = "wackdb.db")]
    db_path: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // Setup Ratatui terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Setup DB
    let data_dir = Path::new(&args.db_path);
    let disk_manager = BasicDiskManager::<8192>::new(data_dir)?;
    let mut bpm = BufferPoolManager::new(500, disk_manager);

    // Run TUI
    let res = run_app(&mut terminal, &args.db_path, &mut bpm);

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

fn run_app<B: Backend, D: wackdb_storage::DiskManager<8192>>(
    terminal: &mut Terminal<B>,
    db_path: &str,
    bpm: &mut BufferPoolManager<8192, D>,
) -> Result<(), Box<dyn Error>>
where
    <B as Backend>::Error: 'static,
{
    let mut input = String::new();
    let mut logs = vec![format!("Connected to WackDB at: {}", db_path)];

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints(
                    [
                        Constraint::Length(3),
                        Constraint::Length(3),
                        Constraint::Min(1),
                    ]
                    .as_ref(),
                )
                .split(f.area());

            let input_widget = Paragraph::new(format!("> {}", input))
                .style(Style::default().fg(Color::Yellow))
                .block(Block::default().borders(Borders::ALL).title("SQL Input"));
            f.render_widget(input_widget, chunks[0]);

            let metrics_widget = Paragraph::new(format!(
                "Hit Rate: {:.2}% | Total Hits: {} | Total Misses: {}",
                bpm.get_hit_rate() * 100.0,
                bpm.get_hits(),
                bpm.get_misses()
            ))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Buffer Pool Metrics"),
            );
            f.render_widget(metrics_widget, chunks[1]);

            let messages: String = logs.join("\n");
            let logs_widget = Paragraph::new(messages)
                .block(Block::default().borders(Borders::ALL).title("Output"));
            f.render_widget(logs_widget, chunks[2]);
        })?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char(c) => {
                    input.push(c);
                }
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Enter => {
                    if input.trim() == "exit" || input.trim() == "quit" {
                        return Ok(());
                    }

                    logs.push(format!("Executing: {}", input));
                    let mut p = parser::Parser::new(&input);
                    match p.parse() {
                        Ok(ast) => logs.push(format!("Parsed AST: {:?}", ast)),
                        Err(e) => logs.push(format!("Parse Error: {}", e)),
                    }

                    input.clear();
                }
                KeyCode::Esc => {
                    return Ok(());
                }
                _ => {}
            }
        }
    }
}
