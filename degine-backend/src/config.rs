use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub bind: String,
    pub database_url: String,
    #[serde(default)]
    pub database_ca: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LeanConfig {
    pub project_dir: PathBuf,
    pub helper_path: PathBuf,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_timeout_secs() -> u64 {
    180
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    pub supabase_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub lean: LeanConfig,
    pub auth: AuthConfig,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("failed to parse config {}", path.display()))
    }
}

impl LeanConfig {
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }
}
