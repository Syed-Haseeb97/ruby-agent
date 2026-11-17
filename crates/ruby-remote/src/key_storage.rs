//! Configuration and credentials storage for remote control.
//! Stores TELEGRAM_BOT_TOKEN in Windows Credential Manager (keyring) with .env fallback.
//! Persists paired user ID and last update offset to avoid replaying old commands on restart.

use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

const KEYRING_SERVICE: &str = "ruby-agent";
const TELEGRAM_KEYRING_USER: &str = "telegram-bot-token";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RemoteConfig {
    pub enabled: bool,
    pub keep_awake: bool,
    pub paired_user_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RemoteOffsetState {
    pub last_update_id: i64,
}

pub fn get_ruby_data_dir() -> PathBuf {
    let base = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| "D:\\projects\\ruby-agent".to_string());
    let path = Path::new(&base).join(".ruby");
    let _ = fs::create_dir_all(&path);
    path
}

fn config_path() -> PathBuf {
    get_ruby_data_dir().join("remote_config.json")
}

fn offset_path() -> PathBuf {
    get_ruby_data_dir().join("remote_offset.json")
}

/// Retrieves the Telegram bot token from Windows Credential Manager, falling back to environment variable.
pub fn get_telegram_bot_token() -> Option<String> {
    // 1. Windows Credential Manager
    if let Ok(entry) = Entry::new(KEYRING_SERVICE, TELEGRAM_KEYRING_USER) {
        if let Ok(password) = entry.get_password() {
            let trimmed = password.trim();
            if !trimmed.is_empty() {
                debug!("Retrieved Telegram bot token from Windows Credential Manager");
                return Some(trimmed.to_string());
            }
        }
    }

    // 2. TELEGRAM_BOT_TOKEN environment variable fallback
    if let Ok(env_token) = std::env::var("TELEGRAM_BOT_TOKEN") {
        let trimmed = env_token.trim();
        if !trimmed.is_empty() {
            debug!("Retrieved Telegram bot token from environment variable TELEGRAM_BOT_TOKEN");
            return Some(trimmed.to_string());
        }
    }

    warn!("Telegram bot token not found in Windows Credential Manager or TELEGRAM_BOT_TOKEN");
    None
}

/// Stores the Telegram bot token in Windows Credential Manager.
pub fn set_telegram_bot_token(token: &str) -> Result<(), String> {
    let entry = Entry::new(KEYRING_SERVICE, TELEGRAM_KEYRING_USER)
        .map_err(|e| format!("Failed to access keyring: {:?}", e))?;
    entry.set_password(token.trim()).map_err(|e| {
        format!(
            "Failed to store token in Windows Credential Manager: {:?}",
            e
        )
    })?;
    debug!("Successfully saved Telegram bot token to Windows Credential Manager");
    Ok(())
}

/// Loads remote configuration from disk.
pub fn load_remote_config() -> RemoteConfig {
    let p = config_path();
    if p.exists() {
        if let Ok(content) = fs::read_to_string(&p) {
            if let Ok(cfg) = serde_json::from_str::<RemoteConfig>(&content) {
                return cfg;
            }
        }
    }

    // Default configuration with env overrides
    let enabled = std::env::var("RUBY_REMOTE_ENABLED")
        .map(|v| v.trim() == "1" || v.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let keep_awake = std::env::var("RUBY_REMOTE_KEEP_AWAKE")
        .map(|v| v.trim() == "1" || v.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    RemoteConfig {
        enabled,
        keep_awake,
        paired_user_id: None,
    }
}

/// Saves remote configuration to disk.
pub fn save_remote_config(cfg: &RemoteConfig) -> Result<(), String> {
    let p = config_path();
    let content = serde_json::to_string_pretty(cfg)
        .map_err(|e| format!("Failed to serialize remote config: {:?}", e))?;
    fs::write(&p, content).map_err(|e| format!("Failed to write remote config: {:?}", e))?;
    Ok(())
}

/// Loads the persisted Telegram last_update_id offset to prevent replaying old messages on restart.
pub fn load_persisted_offset() -> i64 {
    let p = offset_path();
    if p.exists() {
        if let Ok(content) = fs::read_to_string(&p) {
            if let Ok(state) = serde_json::from_str::<RemoteOffsetState>(&content) {
                return state.last_update_id;
            }
        }
    }
    0
}

/// Persists the Telegram last_update_id offset.
pub fn save_persisted_offset(last_update_id: i64) -> Result<(), String> {
    let p = offset_path();
    let state = RemoteOffsetState { last_update_id };
    let content = serde_json::to_string(&state)
        .map_err(|e| format!("Failed to serialize remote offset: {:?}", e))?;
    fs::write(&p, content).map_err(|e| format!("Failed to write remote offset: {:?}", e))?;
    Ok(())
}
