use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub page_size: usize,
    pub buffer_pool_size: usize,
    pub segment_size: u64,
    pub lru_capacity: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            page_size: 8192,
            buffer_pool_size: 1024,
            segment_size: 1024 * 1024 * 1024,
            lru_capacity: 1024,
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        if self.page_size == 0 {
            bail!("PAGE_SIZE must be greater than 0");
        }

        if self.page_size % 512 != 0 && self.page_size % 4096 != 0 {
            bail!("PAGE_SIZE must be a multiple of 512 or 4096");
        }

        Ok(())
    }

    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }
}
