#![warn(missing_docs)]
#![allow(
    clippy::collapsible_if,
    clippy::match_like_matches_macro,
    clippy::unnecessary_cast
)]
//! WackDB Command Line Interface
//!
//! This binary provides an interactive SQL REPL for the WackDB storage engine.

mod commands;

use clap::Parser;
use colored::Colorize;
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use wackdb_buffer::buffer_pool::BufferPoolManager;
use wackdb_catalog::Catalog;
use wackdb_storage::DiskManager;
use wackdb_storage::disk_manager::BasicDiskManager;
use wackdb_wal::DiskLogManager;

use crate::commands::process_command;

/// Main configuration for the WackDB node.
#[derive(serde::Deserialize, Debug, Default)]
#[serde(default)]
pub struct WackDbConfig {
    /// Checkpointer thread configuration.
    pub checkpoint: CheckpointConfig,
    /// Size of the buffer pool in pages.
    pub buffer_pool_size: Option<usize>,
    /// Web interface configuration.
    pub web: WebConfig,
    /// Query execution configuration.
    pub query: QueryConfig,
}

/// Configuration for query execution.
#[derive(serde::Deserialize, Debug)]
pub struct QueryConfig {
    /// Number of tuples to sort in RAM before spilling to disk.
    pub sort_chunk_size: usize,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            sort_chunk_size: 1000,
        }
    }
}

/// Configuration for the web server.
#[derive(serde::Deserialize, Debug)]
pub struct WebConfig {
    /// Port for the visualizer web server.
    pub port: u16,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self { port: 3000 }
    }
}

/// Configuration for the automatic checkpointer.
#[derive(serde::Deserialize, Debug)]
pub struct CheckpointConfig {
    /// Interval in seconds between forced checkpoints.
    pub interval_secs: u64,
    /// Maximum size of the WAL in KB before triggering a checkpoint.
    pub max_wal_size_kb: u64,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            interval_secs: 30,
            max_wal_size_kb: 1024,
        }
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the data directory (WackDB stores multiple files per relation, not a single file)
    #[arg(short, long, default_value = "wackdb_data")]
    data_dir: String,
}

const PAGE_SIZE: usize = 8192;
const BUFFER_POOL_SIZE: usize = 64;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let data_dir = args.data_dir;

    let config_path = Path::new("wackdb.toml");
    let config: WackDbConfig = if config_path.exists() {
        let content = std::fs::read_to_string(config_path).unwrap_or_default();
        toml::from_str(&content).unwrap_or_default()
    } else {
        WackDbConfig::default()
    };


    println!("Connected to Data Directory at: {}", data_dir);
    println!("Loaded Config: {:?}", config);
    println!("Enter '.help' for usage hints. Statements must end with ';'.");

    std::fs::create_dir_all(&data_dir)?;

    let mut catalog = Catalog::open(&data_dir)?;
    let disk_manager = BasicDiskManager::<PAGE_SIZE>::new(Path::new(&data_dir))?;
    let disk_lm = Arc::new(DiskLogManager::new(&data_dir)?);
    let log_manager: Arc<dyn wackdb_buffer::LogManager> = disk_lm.clone();
    let bpm_size = config.buffer_pool_size.unwrap_or(BUFFER_POOL_SIZE);
    let bpm = BufferPoolManager::new_with_log_manager(bpm_size, disk_manager, log_manager);

    let shared_bpm = Arc::new(parking_lot::RwLock::new(bpm));

    // Check and rebuild corrupted BTrees from heap file before WAL
    {
        let bpm_guard = shared_bpm.read();
        let bpm = &*bpm_guard;
        for table in catalog.list_tables() {
            let table_name = table.name.clone();
            if let Ok(meta) = catalog.get_table(&table_name) {
                if let Ok(schema) = catalog.get_schema(&table_name) {
                    let btree = wackdb_btree::tree::BTreeIndex::new(
                        bpm,
                        meta.root_page_num.map(|n| wackdb_storage::PageId {
                            file_id: meta.index_relation_id,
                            page_num: n,
                        }),
                        meta.index_relation_id,
                    );

                    let is_corrupted = match btree.search(0) {
                        Err(wackdb_btree::tree::BTreeError::InvalidNode) => true,
                        _ => false,
                    };

                    if is_corrupted {
                        println!(
                            "[WARN] Corrupted BTree root detected for table '{}'. Rebuilding index from heap file...",
                            table_name
                        );

                        let fresh_btree =
                            wackdb_btree::tree::BTreeIndex::new(bpm, None, meta.index_relation_id);

                        let max_pages = bpm.get_total_pages(meta.heap_relation_id).unwrap_or(0);
                        let mut current_page = 0;
                        while current_page <= max_pages {
                            let page_id = wackdb_storage::PageId {
                                file_id: meta.heap_relation_id,
                                page_num: current_page,
                            };

                            if let Ok(frame_id) = bpm.fetch_page(page_id) {
                                let page = bpm.read_page(frame_id);
                                let max_slots =
                                    wackdb_query::get_total_slots_from_bytes(&page.data.0);
                                for slot in 0..max_slots {
                                    if let Some(record) = wackdb_query::get_record_from_bytes(
                                        &page.data.0,
                                        slot as usize,
                                    ) {
                                        let tuple = wackdb_tuple::Tuple {
                                            data: record.1.to_vec(),
                                        };
                                        if let Ok(vals) = tuple.to_values(&schema) {
                                            if let Some(wackdb_tuple::Value::Integer(pk)) =
                                                vals.first()
                                            {
                                                let ctid = wackdb_storage::CTID {
                                                    page_id,
                                                    slot_idx: slot as u16,
                                                };
                                                let _ = fresh_btree.insert(*pk, ctid);
                                            }
                                        }
                                    }
                                }
                                drop(page);
                                let _ = bpm.unpin_page(page_id, false);
                            }
                            current_page += 1;
                        }

                        if let Some(root_id) = fresh_btree.get_root_page_id() {
                            let _ = catalog.update_root_page(&table_name, Some(root_id.page_num));
                        }
                    }
                }
            }
        }
    }

    // Crash Recovery Phase
    if let Ok(records) = disk_lm.read_all_logs() {
        let mut replayed = 0;
        let mut current_pages = std::collections::HashMap::new();

        // Initialize current_pages with the last page of each table's heap file
        {
            let bpm_guard = shared_bpm.read();
            let bpm = &*bpm_guard;
            for table in catalog.list_tables() {
                let table_name = table.name.clone();
                if let Ok(meta) = catalog.get_table(&table_name) {
                    if let Ok(total_pages) = bpm.get_total_pages(meta.heap_relation_id) {
                        if total_pages > 0 {
                            let last_page = total_pages - 1;
                            let page_id = wackdb_storage::PageId {
                                file_id: meta.heap_relation_id,
                                page_num: last_page,
                            };
                            if let Ok(frame_id) = bpm.fetch_page(page_id) {
                                current_pages.insert(table_name, Some((frame_id, page_id)));
                            }
                        }
                    }
                }
            }
        }

        for rec in records {
            if rec.len() > 3 {
                // op(1) + tn_len(2)
                let op = rec[0]; // 0 = INSERT, 1 = DELETE
                let tn_len = u16::from_le_bytes(rec[1..3].try_into().unwrap_or([0, 0])) as usize;
                if tn_len > 0 && rec.len() >= 3 + tn_len {
                    let table = String::from_utf8_lossy(&rec[3..3 + tn_len]).to_string();
                    let tuple_data = &rec[3 + tn_len..];

                    if let Ok(schema) = catalog.get_schema(&table) {
                        let tuple = wackdb_tuple::Tuple {
                            data: tuple_data.to_vec(),
                        };

                        if let Ok(meta) = catalog.get_table(&table) {
                            let bpm_guard = shared_bpm.read();
                            let bpm = &*bpm_guard;

                            let mut btree = wackdb_btree::tree::BTreeIndex::new(
                                bpm,
                                meta.root_page_num.map(|n| wackdb_storage::PageId {
                                    file_id: meta.index_relation_id,
                                    page_num: n,
                                }),
                                meta.index_relation_id,
                            );

                            if let Ok(parsed_vals) = tuple.to_values(&schema) {
                                if let Some(wackdb_tuple::Value::Integer(pk)) = parsed_vals.first()
                                {
                                    if op == 0 {
                                        // INSERT RECOVERY
                                        if wackdb_btree::traits::Index::search(&btree, *pk).is_ok()
                                        {
                                            continue; // skip, already persisted or duplicate in WAL
                                        }

                                        let current_page =
                                            current_pages.entry(table.clone()).or_insert(None);
                                        if current_page.is_none() {
                                            let (frame_id, page_id) =
                                                bpm.new_page(meta.heap_relation_id).unwrap();
                                            {
                                                let mut page = bpm.write_page(frame_id);
                                                page.init();
                                            }
                                            *current_page = Some((frame_id, page_id));
                                        }

                                        let (frame_id, page_id) = current_page.unwrap();
                                        let slot_idx = {
                                            let mut page = bpm.write_page(frame_id);
                                            page.insert_record(&tuple.data, 1)
                                        };

                                        let ctid = match slot_idx {
                                            Some(idx) => wackdb_storage::CTID {
                                                page_id,
                                                slot_idx: idx as u16,
                                            },
                                            None => {
                                                bpm.unpin_page(page_id, true).unwrap();
                                                let (nf, np) =
                                                    bpm.new_page(meta.heap_relation_id).unwrap();
                                                *current_page = Some((nf, np));
                                                let idx = {
                                                    let mut page = bpm.write_page(nf);
                                                    page.init();
                                                    page.insert_record(&tuple.data, 1).unwrap_or(0)
                                                };
                                                wackdb_storage::CTID {
                                                    page_id: np,
                                                    slot_idx: idx as u16,
                                                }
                                            }
                                        };

                                        match btree.insert(*pk, ctid) {
                                            Ok(_) => {}
                                            Err(e) => panic!(
                                                "BTree insert error during WAL replay: {:?}",
                                                e
                                            ),
                                        }

                                        if let Some(r) = btree.get_root_page_id() {
                                            drop(bpm_guard);
                                            catalog
                                                .update_root_page(&table, Some(r.page_num))
                                                .unwrap();
                                        } else {
                                            drop(bpm_guard);
                                        }
                                        replayed += 1;
                                    } else if op == 1 {
                                        // DELETE RECOVERY
                                        if let Ok(ctid) =
                                            wackdb_btree::traits::Index::search(&btree, *pk)
                                        {
                                            let _ = wackdb_btree::traits::Index::delete(
                                                &mut btree, *pk,
                                            );
                                            if let Some(r) = btree.get_root_page_id() {
                                                drop(bpm_guard);
                                                catalog
                                                    .update_root_page(&table, Some(r.page_num))
                                                    .unwrap();
                                            } else {
                                                drop(bpm_guard);
                                            }

                                            // Also delete from heap
                                            let bpm_guard2 = shared_bpm.read();
                                            let bpm2 = &*bpm_guard2;
                                            if let Ok(fid) = bpm2.fetch_page(ctid.page_id) {
                                                let mut p_write = bpm2.write_page(fid);
                                                p_write.mark_deleted(ctid.slot_idx as usize, 999); // using 999 as dummy xmax
                                                drop(p_write);
                                                let _ = bpm2.unpin_page(ctid.page_id, true);
                                            }
                                            drop(bpm_guard2);

                                            replayed += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Unpin all cached pages used during recovery
        let bpm_guard = shared_bpm.read();
        for (_, page_info) in current_pages {
            if let Some((_, page_id)) = page_info {
                let _ = bpm_guard.unpin_page(page_id, true);
            }
        }
        if replayed > 0 {
            println!("Crash Recovery: Replayed {} operations from WAL.", replayed);
        }
    }

    // Background Checkpointer Thread
    let checkpointer_bpm = shared_bpm.clone();
    let cp_interval = Duration::from_secs(config.checkpoint.interval_secs);
    let cp_max_size = config.checkpoint.max_wal_size_kb * 1024;

    std::thread::spawn(move || {
        let mut last_checkpoint = std::time::Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(500));

            let mut should_checkpoint = false;
            if last_checkpoint.elapsed() >= cp_interval {
                should_checkpoint = true;
            } else {
                let bpm = checkpointer_bpm.read();
                if let Some(lm) = bpm.get_log_manager() {
                    if lm.log_size() >= cp_max_size {
                        should_checkpoint = true;
                    }
                }
            }

            if should_checkpoint {
                let bpm = checkpointer_bpm.read();
                let _ = bpm.flush_all_pages();
                if let Some(lm) = bpm.get_log_manager() {
                    let _ = lm.flush_up_to(u64::MAX);
                    let _ = lm.checkpoint();
                }
                last_checkpoint = std::time::Instant::now();
            }
        }
    });

    // Background web server
    let bpm_clone = shared_bpm.clone();
    let data_dir_clone = data_dir.clone();
    let web_port = config.web.port;
    std::thread::spawn(move || {
        if let Ok(rt) = tokio::runtime::Runtime::new() {
            rt.block_on(async {
                let _ = wackdb_web::start_server(bpm_clone, data_dir_clone, web_port).await;
            });
        }
    });

    // Sleep briefly to allow the background web server to print its startup log
    // before we draw the interactive prompt.
    std::thread::sleep(Duration::from_millis(100));

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })?;

    let mut current_statement = String::new();

    while running.load(Ordering::SeqCst) {
        let prompt = if current_statement.is_empty() {
            format!("wackdb ({})> ", data_dir).cyan()
        } else {
            format!("{:width$}> ", "", width = data_dir.len() + 8).cyan()
        };
        print!("{}", prompt);
        io::stdout().flush().unwrap();

        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 {
            // EOF (Ctrl+D)
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() && current_statement.is_empty() {
            continue;
        }

        // Handle dot commands
        if current_statement.is_empty() && trimmed.starts_with('.') {
            if trimmed == ".exit" {
                break;
            } else {
                let start = Instant::now();
                let cmd_res = process_command(
                    trimmed,
                    &mut catalog,
                    &mut shared_bpm.write(),
                    config.query.sort_chunk_size,
                    false,
                    false,
                );
                match cmd_res {
                    Ok(Some(sig)) if sig == ".reset" => {
                        println!(
                            "{}",
                            "[WARN] Resetting database environment...".bold().yellow()
                        );
                        {
                            let mut bpm = shared_bpm.write();
                            let _ = bpm.flush_all_pages();
                            bpm.disk_manager().close_all();

                            // Replace with dummy to drop the real one and close all file handles
                            let dummy_dir = std::path::Path::new("wackdb_dummy_reset");
                            let _ = std::fs::create_dir_all(dummy_dir);
                            let dummy_disk = BasicDiskManager::<PAGE_SIZE>::new(dummy_dir).unwrap();
                            let dummy_lm: Arc<dyn wackdb_buffer::LogManager> =
                                Arc::new(DiskLogManager::new(dummy_dir).unwrap());
                            let dummy_bpm =
                                BufferPoolManager::new_with_log_manager(1, dummy_disk, dummy_lm);
                            *bpm = dummy_bpm;
                        }

                        if let Err(e) = std::fs::remove_dir_all(&data_dir) {
                            println!(
                                "{}",
                                format!("[WARN] Could not remove data directory: {}", e).yellow()
                            );
                        }
                        std::fs::create_dir_all(&data_dir)?;

                        // Re-initialize
                        catalog = Catalog::open(&data_dir)?;
                        let disk_manager =
                            BasicDiskManager::<PAGE_SIZE>::new(Path::new(&data_dir))?;
                        let disk_lm = Arc::new(DiskLogManager::new(&data_dir)?);
                        let log_manager: Arc<dyn wackdb_buffer::LogManager> = disk_lm.clone();
                        let new_bpm = BufferPoolManager::new_with_log_manager(
                            BUFFER_POOL_SIZE,
                            disk_manager,
                            log_manager,
                        );
                        *shared_bpm.write() = new_bpm;

                        let _ = std::fs::remove_dir_all("wackdb_dummy_reset");

                        println!(
                            "{}",
                            "[OK] Environment cleanly wiped and re-initialized."
                                .bold()
                                .green()
                        );
                        continue;
                    }
                    Ok(Some(sig)) if sig.starts_with(".source ") => {
                        let file_path = sig.trim_start_matches(".source ");
                        match std::fs::read_to_string(file_path) {
                            Ok(contents) => {
                                for stmt in contents.split(';') {
                                    let stmt = stmt.trim();
                                    if stmt.is_empty() {
                                        continue;
                                    }
                                    println!(
                                        "{}",
                                        format!("wackdb ({})> {};", data_dir, stmt).cyan()
                                    );
                                    let _ = process_command(
                                        stmt,
                                        &mut catalog,
                                        &mut shared_bpm.write(),
                                        config.query.sort_chunk_size,
                                        false,
                                        false,
                                    );
                                }
                            }
                            Err(e) => println!(
                                "{}",
                                format!("[ERR] Failed to read source file '{}': {}", file_path, e)
                                    .bold()
                                    .red()
                            ),
                        }
                        continue;
                    }
                    Ok(_) => {}
                    Err(e) => println!("Error: {}", e),
                }
                let duration = start.elapsed();
                println!("Time: {:?}", duration);
            }
            continue;
        }

        current_statement.push_str(&line);

        if current_statement.trim().ends_with(';') {
            let stmt = current_statement.trim().trim_end_matches(';').trim();

            let start = Instant::now();
            match process_command(
                stmt,
                &mut catalog,
                &mut shared_bpm.write(),
                config.query.sort_chunk_size,
                false,
                false,
            ) {
                Ok(telemetry) => {
                    let duration = start.elapsed();
                    println!("Execution time: {:?}", duration);
                    if let Some(t) = telemetry {
                        println!("{}", t);
                    }
                }
                Err(e) => {
                    println!("Error: {}", e);
                }
            }

            current_statement.clear();
        }
    }

    println!("\nShutting down WackDB. Performing Auto-Checkpoint...");
    let bpm = shared_bpm.write();
    let _ = bpm.flush_all_pages();
    if let Some(lm) = bpm.get_log_manager() {
        let _ = lm.flush_up_to(u64::MAX);
        let _ = lm.checkpoint();
        println!("Auto-Checkpoint complete. WAL truncated.");
    }
    drop(bpm);
    Ok(())
}
