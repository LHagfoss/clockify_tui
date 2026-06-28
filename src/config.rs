use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct Config {
    pub api_key: Option<String>,
    pub active_workspace_id: Option<String>,
}

fn get_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|mut p| {
        p.push("clockify-tui");
        p.push("config.toml");
        p
    })
}

pub fn load() -> Result<Config, Box<dyn std::error::Error>> {
    let path = get_config_path().ok_or("Could not locate config directory")?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let content = fs::read_to_string(path)?;
    let config: Config = toml::from_str(&content)?;
    Ok(config)
}

pub fn save(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let path = get_config_path().ok_or("Could not locate config directory")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = toml::to_string(config)?;
    fs::write(path, content)?;
    Ok(())
}
