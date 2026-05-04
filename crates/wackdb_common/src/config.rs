use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub data_dir: String,
    pub page_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            data_dir: "data".to_string(),
            page_size: 8192,
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        if self.page_size == 0 {
            bail!("page_size must be greater than 0");
        }
        if !self.page_size.is_multiple_of(512) {
            bail!("page_size must be a multiple of 512");
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
