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
    Terminal,
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Margin},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
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
            .title("SQL Input (Enter to Execute, Ctrl+Up/Down or PageUp/Down to Scroll Output)")
            .style(Style::default().fg(Color::LightGreen)),
    );

    let mut vertical_scroll = 0;

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

            let content_len = logs.len();
            let view_height = chunks[2].height.saturating_sub(2) as usize;
            let max_scroll = content_len.saturating_sub(view_height);
            if vertical_scroll > max_scroll {
                vertical_scroll = max_scroll;
            }

            let messages: String = logs.join("\n");
            let logs_widget = Paragraph::new(messages)
                .block(Block::default().borders(Borders::ALL).title("Output"))
                .style(Style::default().fg(Color::White))
                .scroll((vertical_scroll as u16, 0));
            f.render_widget(logs_widget, chunks[2]);

            let mut scrollbar_state = ScrollbarState::default()
                .content_length(max_scroll)
                .position(vertical_scroll);

            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"));

            f.render_stateful_widget(
                scrollbar,
                chunks[2].inner(Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
                &mut scrollbar_state,
            );
        })?;

        let event = event::read()?;
        match event.into() {
            Input { key: Key::Esc, .. } => return Ok(()),
            Input {
                key: Key::PageUp, ..
            } => {
                vertical_scroll = vertical_scroll.saturating_sub(10);
            }
            Input {
                key: Key::PageDown, ..
            } => {
                vertical_scroll = vertical_scroll.saturating_add(10);
            }
            Input {
                key: Key::Up,
                ctrl: true,
                ..
            } => {
                vertical_scroll = vertical_scroll.saturating_sub(1);
            }
            Input {
                key: Key::Down,
                ctrl: true,
                ..
            } => {
                vertical_scroll = vertical_scroll.saturating_add(1);
            }
            Input {
                key: Key::Enter, ..
            } => {
                let text = textarea.lines().join("\n");
                let text = text.trim();
                if text == "exit" || text == "quit" {
                    return Ok(());
                }
                if !text.is_empty() {
                    if text == "\\demo_buffer" {
                        logs.push("--- DEMO BUFFER POOL ---".to_string());
                        logs.push("Beginning batch insert of 10,000 records...".to_string());

                        let mut pages_allocated = 1;
                        let mut total_inserted = 0;
                        let (mut current_frame_id, mut current_page_id) = match bpm.new_page(0) {
                            Ok(res) => res,
                            Err(e) => {
                                logs.push(format!("Error: {:?}", e));
                                continue;
                            }
                        };

                        let cities = [
                            "New York",
                            "San Francisco",
                            "London",
                            "Berlin",
                            "Tokyo",
                            "Paris",
                            "Madrid",
                            "Lima",
                            "Toronto",
                            "Sydney",
                        ];

                        for i in 1..=10000 {
                            let name = format!("User{}", i);
                            let email = format!("user{}@example.com", i);
                            let city = cities[(i as usize) % cities.len()];
                            let record = format!("{},{},{},{}", i, name, email, city);
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

                            if i <= 5 || i > 9995 {
                                if i == 6 {
                                    logs.push(
                                        "... (skipping prints for bulk inserts) ...".to_string(),
                                    );
                                }
                                logs.push(format!(
                                    "[INSERT] Page ID: {:?}, Size: {}, Tuple: {}",
                                    current_page_id.page_num,
                                    bytes.len(),
                                    record
                                ));
                            }
                        }
                        let _ = bpm.unpin_page(current_page_id, true);

                        logs.push("\n[ Batch Insert Complete ]".to_string());
                        logs.push(format!("Total Records Inserted: {}", total_inserted));
                        logs.push(format!("Total Pages Allocated: {}", pages_allocated));

                        logs.push(format!(
                            "\n[ Architecture of the Final Page (Page {}) ]",
                            current_page_id.page_num
                        ));
                        {
                            let last_page_guard = bpm.read_page(current_frame_id);
                            let header = last_page_guard.header();
                            logs.push(format!("- Total Slots: {}", header.total_slots));
                            logs.push(format!("- Free Space Upper: {}", header.free_space_upper));

                            logs.push("\n[ Tuple Data Region (Last Page) ]".to_string());
                            for i in 0..header.total_slots as usize {
                                if let Some((_, data)) = last_page_guard.get_record(i) {
                                    logs.push(format!(
                                        " Data {:02} -> {}",
                                        i,
                                        String::from_utf8_lossy(data)
                                    ));
                                }
                            }
                        }

                        logs.push(format!(
                            "\n[ Cache Verification on Last Page (Page ID: {:?}) ]",
                            current_page_id
                        ));
                        let hits_before = bpm.get_hits();
                        let misses_before = bpm.get_misses();
                        let _ = bpm.fetch_page(current_page_id);
                        let hits_after = bpm.get_hits();

                        logs.push(format!(" - Misses so far: {}", misses_before));
                        logs.push(format!(
                            " - Hits on second load: {}",
                            hits_after - hits_before
                        ));

                        let _ = bpm.unpin_page(current_page_id, false);

                        if let Err(e) = bpm.flush_all_pages() {
                            logs.push(format!("Warning: Failed to flush to disk: {:?}", e));
                        } else {
                            logs.push("\n[PERSIST] All pages successfully flushed to disk (check the wack.db file!).".to_string());
                        }
                    } else if text == "\\demo_btree" {
                        logs.push("--- DEMO B+ TREE ---".to_string());
                        logs.push(
                            "Initializing B+ Tree Index on top of Buffer Pool...".to_string(),
                        );

                        let btree = wackdb_btree::tree::BTreeIndex::new(bpm, None);
                        let mut success = true;

                        logs.push("Inserting keys 0 to 399 to trigger a root SPLIT...".to_string());
                        for i in 0u16..400 {
                            let ctid = wackdb_storage::CTID {
                                page_id: wackdb_storage::PageId {
                                    file_id: i as u32,
                                    page_num: i as u32,
                                },
                                slot_idx: i,
                            };
                            if let Err(e) = btree.insert(i as i32, ctid) {
                                logs.push(format!("Error inserting key {}: {:?}", i, e));
                                success = false;
                                break;
                            }

                            if i <= 3 || i >= 397 {
                                if i == 4 {
                                    logs.push(
                                        "... (skipping prints for bulk inserts) ...".to_string(),
                                    );
                                }
                                logs.push(format!("[BTREE INSERT] Key: {}, CTID: {:?}", i, ctid));
                            }
                        }

                        if success {
                            logs.push("\n[ BTREE Batch Insert Complete ]".to_string());
                            logs.push("Successfully inserted 400 keys. The leaf node reached max capacity (340) and SPLIT into a new root and right sibling!".to_string());

                            logs.push("\n[ Point Search Verifications ]".to_string());
                            let val_0 = btree.search(0);
                            let val_150 = btree.search(150);
                            let val_399 = btree.search(399);
                            logs.push(format!("Search Key 0 -> {:?}", val_0));
                            logs.push(format!("Search Key 150 -> {:?}", val_150));
                            logs.push(format!("Search Key 399 -> {:?}", val_399));

                            if let Err(e) = bpm.flush_all_pages() {
                                logs.push(format!("Warning: Failed to flush to disk: {:?}", e));
                            } else {
                                logs.push(
                                    "\n[PERSIST] B+ Tree changes fully persisted to disk."
                                        .to_string(),
                                );
                            }
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
                            .title("SQL Input (Enter to Execute, Ctrl+Up/Down or PageUp/Down to Scroll Output)")
                            .style(Style::default().fg(Color::LightGreen)),
                    );
                    vertical_scroll = usize::MAX;
                }
            }
            input => {
                textarea.input(input);
            }
        }
    }
}
