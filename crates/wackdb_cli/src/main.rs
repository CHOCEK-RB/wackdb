#![warn(missing_docs)]
//! WackDB Command Line Interface
//!
//! This binary provides an interactive demonstration of the WackDB storage engine.

use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};
use ratatui_textarea::{Input, Key, TextArea};
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
    let mut logs = vec![format!("Connected to WackDB at: {}", db_path)];

    let mut textarea = TextArea::default();
    textarea.set_block(
        Block::default()
            .borders(Borders::ALL)
            .title("SQL Input (Enter to Execute)")
            .style(Style::default().fg(Color::LightGreen)),
    );

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

            f.render_widget(&textarea, chunks[0]);

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

        let event = event::read()?;
        match event.into() {
            Input { key: Key::Esc, .. } => return Ok(()),
            Input { key: Key::Enter, .. } => {
                let text = textarea.lines().join("\n");
                let text = text.trim();
                if text == "exit" || text == "quit" {
                    return Ok(());
                }
                if !text.is_empty() {
                    if text == "\\demo_buffer" {
                        logs.push("--- DEMO BUFFER POOL ---".to_string());
                        logs.push("Inserting 10,000 records into Buffer Pool...".to_string());
                        
                        let mut pages_allocated = 1;
                        let mut total_inserted = 0;
                        let (mut current_frame_id, mut current_page_id) = match bpm.new_page(0) {
                            Ok(res) => res,
                            Err(e) => {
                                logs.push(format!("Error: {:?}", e));
                                continue;
                            }
                        };
                        
                        for i in 1..=10000 {
                            let record = format!("User{}@example.com", i);
                            let bytes = record.as_bytes();
                            let mut inserted = false;
                            
                            {
                                let mut page_guard = bpm.write_page(current_frame_id);
                                if page_guard.header().total_slots == 0 {
                                    page_guard.init();
                                }
                                if page_guard.insert_record(bytes, 0).is_some() {
                                    inserted = true;
                                    total_inserted += 1;
                                }
                            }
                            
                            if !inserted {
                                let _ = bpm.unpin_page(current_page_id, true);
                                if let Ok((new_frame, new_page)) = bpm.new_page(0) {
                                    current_frame_id = new_frame;
                                    current_page_id = new_page;
                                    pages_allocated += 1;
                                    
                                    let mut new_page_guard = bpm.write_page(current_frame_id);
                                    new_page_guard.init();
                                    if new_page_guard.insert_record(bytes, 0).is_some() {
                                        total_inserted += 1;
                                    }
                                } else {
                                    logs.push("Error allocating new page!".to_string());
                                    break;
                                }
                            }
                        }
                        let _ = bpm.unpin_page(current_page_id, true);
                        
                        logs.push(format!("Inserted {} records.", total_inserted));
                        logs.push(format!("Allocated {} pages.", pages_allocated));
                        logs.push(format!("Buffer Pool Hit Rate: {:.2}%", bpm.get_hit_rate() * 100.0));
                    } else if text == "\\demo_btree" {
                        logs.push("--- DEMO B+ TREE ---".to_string());
                        logs.push("Initializing B+ Tree and inserting 400 keys (triggers leaf split)...".to_string());
                        
                        let btree = wackdb_btree::tree::BTreeIndex::new(bpm, None);
                        let mut success = true;
                        
                        for i in 0u16..400 {
                            let ctid = wackdb_storage::CTID {
                                page_id: wackdb_storage::PageId { file_id: i as u32, page_num: i as u32 },
                                slot_idx: i,
                            };
                            if let Err(e) = btree.insert(i as i32, ctid) {
                                logs.push(format!("Error inserting key {}: {:?}", i, e));
                                success = false;
                                break;
                            }
                        }
                        
                        if success {
                            logs.push("Successfully inserted 400 keys and triggered split!".to_string());
                            let val_0 = btree.search(0);
                            let val_399 = btree.search(399);
                            logs.push(format!("Search Key 0 -> {:?}", val_0));
                            logs.push(format!("Search Key 399 -> {:?}", val_399));
                        }
                    } else {
                        logs.push(format!("Executing: {}", text));
                        let mut p = parser::Parser::new(text);
                        match p.parse() {
                            Ok(ast) => logs.push(format!("Parsed AST: {:?}", ast)),
                            Err(e) => logs.push(format!("Parse Error: {}", e)),
                        }
                    }
                    
                    textarea = TextArea::default();
                    textarea.set_block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title("SQL Input (Enter to Execute)")
                            .style(Style::default().fg(Color::LightGreen)),
                    );
                }
            }
            input => {
                textarea.input(input);
            }
        }
    }
}
