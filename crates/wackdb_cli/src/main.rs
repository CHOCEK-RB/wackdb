use anyhow::Result;
use std::path::Path;
use wackdb_common::Config;

fn main() -> Result<()> {
    let config_path = Path::new("config.toml");

    let config = if config_path.exists() {
        println!("Loading configuration from {:?}", config_path);
        Config::from_file(config_path)?
    } else {
        println!("Config file not found, using default configuration");
        Config::default()
    };

    println!("Configuration loaded:");
    println!("  PAGE_SIZE: {}", config.page_size);
    println!("  BUFFER_POOL_SIZE: {}", config.buffer_pool_size);
    println!("  SEGMENT_SIZE: {}", config.segment_size);
    println!("  LRU_CAPACITY: {}", config.lru_capacity);

    Ok(())
}
