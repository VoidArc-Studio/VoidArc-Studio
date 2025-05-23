pub mod compositor;
pub mod launcher;

use toml::Value;
use std::fs;

/// Loads the configuration from /etc/blue-environment/config.toml or fallback
pub fn load_config() -> Value {
    let config_str = fs::read_to_string("/etc/blue-environment/config.toml")
    .unwrap_or_else(|_| include_str!("../config.toml").to_string());
    config_str.parse::<Value>().expect("Invalid config format")
}

/// Detects the current distribution from /etc/os-release
pub fn detect_distro() -> String {
    fs::read_to_string("/etc/os-release")
    .ok()
    .and_then(|content| {
        content.lines()
        .find(|line| line.starts_with("ID="))
        .map(|line| line.strip_prefix("ID=").unwrap_or("unknown").to_string())
    })
    .unwrap_or("unknown".to_string())
}
