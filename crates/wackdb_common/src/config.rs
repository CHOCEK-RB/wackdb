use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

const DEFAULT_DATA_DIR: &str = "data";
const DEFAULT_PAGE_SIZE: usize = 8 * 1024;

const MIN_DISK_SECTOR_SIZE: usize = 512;
const STANDARD_MEMORY_PAGE_SIZE: usize = 4 * 1024;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub data_dir: String,
    pub page_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            data_dir: DEFAULT_DATA_DIR.to_string(),
            page_size: DEFAULT_PAGE_SIZE,
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        self.validate_page_size()?;
        Ok(())
    }

    fn validate_page_size(&self) -> Result<()> {
        if self.page_size == 0 {
            bail!("page_size must be greater than 0");
        }

        let is_valid_size = self.page_size.is_multiple_of(MIN_DISK_SECTOR_SIZE)
            || self.page_size.is_multiple_of(STANDARD_MEMORY_PAGE_SIZE);

        if !is_valid_size {
            bail!(
                "page_size must be a multiple of {} or {}",
                MIN_DISK_SECTOR_SIZE,
                STANDARD_MEMORY_PAGE_SIZE
            );
        }

        Ok(())
    }

    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }
}
