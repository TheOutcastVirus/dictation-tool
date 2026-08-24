use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const DEFAULT_MODEL: &str = "ggml-medium.en.bin";

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub model: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            model: DEFAULT_MODEL.to_string(),
        }
    }
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .expect("no config dir")
        .join("dictation-tool")
        .join("config.toml")
}

pub fn load() -> Config {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
        Err(_) => Config::default(),
    }
}

pub fn save(config: &Config) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(contents) = toml::to_string_pretty(config) {
        let _ = std::fs::write(&path, contents);
    }
}
