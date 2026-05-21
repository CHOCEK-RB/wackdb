#![warn(missing_docs)]
//! WackDB Command Line Interface
//!
//! This binary provides an interactive demonstration of the WackDB storage engine.

use std::error::Error;
use std::fs;
use std::path::Path;
use wackdb_buffer::buffer_pool::BufferPoolManager;
use wackdb_storage::disk_manager::BasicDiskManager;

fn main() -> Result<(), Box<dyn Error>> {
    let data_dir = Path::new("wackdb_data");
    if data_dir.exists() {
        fs::remove_dir_all(data_dir)?;
    }
    fs::create_dir_all(data_dir)?;

    // Buffer pool size to 500 frames
    let disk_manager = BasicDiskManager::<8192>::new(data_dir)?;
    let bpm = BufferPoolManager::new(500, disk_manager);

    println!("Buffer Pool Init (8KB/page) -> Storage Dir: {:?}", data_dir);
    println!("Beginning batch insert of 10,000 records...");

    let mut current_frame_id;
    let mut current_page_id;

    let (first_frame, first_page) = bpm.new_page(0)?;
    current_frame_id = first_frame;
    current_page_id = first_page;

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

    let mut total_inserted = 0;
    let mut pages_allocated = 1;

    for i in 1..=10000 {
        let name = format!("User{}", i);
        let email = format!("user{}@example.com", i);
        let city = cities[(i as usize) % cities.len()];
        let record = format!("{},{},{},{}", i, name, email, city);
        let bytes = record.as_bytes();

        let mut inserted = false;

        // Scope the write lock to drop it before we potentially unpin
        {
            let mut page_guard = bpm.write_page(current_frame_id);
            if page_guard.header().total_slots == 0 {
                // If it's a completely new page, initialize it!
                page_guard.init();
            }

            if page_guard.insert_record(bytes, 0).is_some() {
                inserted = true;
                total_inserted += 1;
            }
        }

        // If the page is full need a new page
        if !inserted {
            // Unpin the old page and mark it dirty
            bpm.unpin_page(current_page_id, true)?;

            // Request a new page from the Buffer Pool
            let (new_frame, new_page) = bpm.new_page(0)?;
            current_frame_id = new_frame;
            current_page_id = new_page;
            pages_allocated += 1;

            // Insert the record into the fresh page
            let mut new_page_guard = bpm.write_page(current_frame_id);
            new_page_guard.init();
            if new_page_guard.insert_record(bytes, 0).is_some() {
                total_inserted += 1;
            } else {
                panic!("Record too large for an empty page!");
            }
        }

        if i <= 5 || i > 9995 {
            if i == 6 {
                println!("... (skipping prints for bulk inserts) ...");
            }
            println!(
                "[INSERT] Page ID: {:?}, Size: {}, Tuple: {}",
                current_page_id.page_num,
                bytes.len(),
                record
            );
        }
    }

    println!("\n[ Batch Insert Complete ]");
    println!("Total Records Inserted: {}", total_inserted);
    println!("Total Pages Allocated: {}", pages_allocated);

    // Show the architecture of the very last page
    println!(
        "\n[ Architecture of the Final Page (Page {}) ]",
        current_page_id.page_num
    );
    {
        let last_page_guard = bpm.read_page(current_frame_id);
        last_page_guard.print_page_architecture();

        let header = last_page_guard.header();
        println!("\n[ Tuple Data Region (Last Page) ]");
        for i in 0..header.total_slots as usize {
            if let Some((_, data)) = last_page_guard.get_record(i) {
                println!(" Data {:02} -> {}", i, String::from_utf8_lossy(data));
            }
        }
    }

    // Unpin the last page
    bpm.unpin_page(current_page_id, true)?;

    // Prove it hits the cache
    println!("\n[Buffer Pool Table Snapshot - BEFORE 2ND FETCH]");
    bpm.print_buffer_state();

    // Cache vs disk hitting for the last page
    println!(
        "\n[ Cache Verification on Last Page (Page ID: {:?}) ]",
        current_page_id
    );
    let hits_before = bpm.get_hits();
    let misses_before = bpm.get_misses();
    let _ = bpm.fetch_page(current_page_id)?; // Re-fetch the last page
    let hits_after = bpm.get_hits();

    println!("\n[Buffer Pool Table Snapshot - AFTER 2ND FETCH]");
    bpm.print_buffer_state();

    println!(" - Misses so far: {}", misses_before);
    println!(" - Hits on second load: {}", hits_after - hits_before);

    bpm.unpin_page(current_page_id, false)?;

    // Flush to disk
    bpm.flush_all_pages()?;
    println!(
        "\n[PERSIST] Data successfully flushed to disk. Check '{}'",
        data_dir.display()
    );

    Ok(())
}
