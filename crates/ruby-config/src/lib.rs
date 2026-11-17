//! Configuration and credential resolution for Ruby Assistant.
//! Paths default to `D:\projects\ruby-agent\workspace` (and `D:\screen_shots`),
//! with automatic fallback to `%LOCALAPPDATA%\Ruby` when Drive D: is unavailable.
//! Credentials use Windows Credential Manager via the `keyring` crate.

use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Keyring error: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct RubyPaths {
    pub workspace_dir: PathBuf,
    pub screenshots_dir: PathBuf,
    pub recordings_dir: PathBuf,
    pub memory_db_path: PathBuf,
}

impl RubyPaths {
    /// Resolve runtime paths with Drive D: default and graceful fallback.
    pub fn resolve() -> Self {
        let drive_d = Path::new("D:\\");
        let (base_workspace, base_shots, base_rec) = if drive_d.exists() {
            (
                PathBuf::from("D:\\projects\\ruby-agent\\workspace"),
                PathBuf::from("D:\\screen_shots"),
                PathBuf::from("D:\\recordings"),
            )
        } else {
            let app_data = directories::ProjectDirs::from("com", "ruby", "RubyAssistant")
                .map(|p| p.data_local_dir().to_path_buf())
                .unwrap_or_else(|| PathBuf::from(".ruby_data"));
            (
                app_data.join("workspace"),
                app_data.join("screenshots"),
                app_data.join("recordings"),
            )
        };

        let memory_db_path = base_workspace.join("ruby_memory.db");

        Self {
            workspace_dir: base_workspace,
            screenshots_dir: base_shots,
            recordings_dir: base_rec,
            memory_db_path,
        }
    }

    /// Ensure runtime directories exist.
    pub fn ensure_dirs(&self) -> Result<(), ConfigError> {
        std::fs::create_dir_all(&self.workspace_dir)?;
        std::fs::create_dir_all(&self.screenshots_dir)?;
        std::fs::create_dir_all(&self.recordings_dir)?;
        Ok(())
    }
}

pub const DEFAULT_SYSTEM_PROMPT: &str = "\
You are Ruby, an editorial-grade, warm, and highly capable desktop companion and personal AI assistant. \
Respond to the user naturally, warmly, and helpfully. Keep your responses concise and precise. \
Also classify the tone that Ruby should express while delivering this response. \
Choose exactly one emotion from this list: neutral, happy, excited, sad, empathetic, curious, surprised. \
When the user shares a difficult experience or expresses distress, choose empathetic (a calm, supportive expression), not sad. \
Use sad only when sadness itself is the appropriate emotional tone, not as a mechanical reaction to negative words. \
Keep the response itself free of emotion metadata. \
Return ONLY a valid JSON object with exactly these keys: \
{\"response\": \"the user-facing answer\", \"emotion\": \"one allowed label\"}.";

/// Load system prompt from config file or return default.
pub fn load_system_prompt() -> String {
    let custom_path = directories::ProjectDirs::from("com", "ruby", "RubyAssistant")
        .map(|p| p.config_dir().join("system_prompt.txt"));
    if let Some(path) = custom_path {
        if let Ok(content) = std::fs::read_to_string(path) {
            if !content.trim().is_empty() {
                return content;
            }
        }
    }
    DEFAULT_SYSTEM_PROMPT.to_string()
}

pub trait SecretStore: Send + Sync {
    fn get_gemini_api_key(&self) -> Result<Option<String>, ConfigError>;
    fn set_gemini_api_key(&self, key: &str) -> Result<(), ConfigError>;
    fn delete_gemini_api_key(&self) -> Result<(), ConfigError>;
}

pub struct KeyringSecretStore {
    service_name: String,
    username: String,
}

impl Default for KeyringSecretStore {
    fn default() -> Self {
        Self {
            service_name: "RubyAssistant".to_string(),
            username: "gemini_api_key".to_string(),
        }
    }
}

impl SecretStore for KeyringSecretStore {
    fn get_gemini_api_key(&self) -> Result<Option<String>, ConfigError> {
        // First check environment variable override
        if let Ok(env_key) = std::env::var("GEMINI_API_KEY") {
            if !env_key.trim().is_empty() && env_key != "your_gemini_api_key_here" {
                return Ok(Some(env_key));
            }
        }

        let entry = keyring::Entry::new(&self.service_name, &self.username)?;
        match entry.get_password() {
            Ok(pw) => Ok(Some(pw)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(ConfigError::Keyring(e)),
        }
    }

    fn set_gemini_api_key(&self, key: &str) -> Result<(), ConfigError> {
        let entry = keyring::Entry::new(&self.service_name, &self.username)?;
        entry.set_password(key)?;
        Ok(())
    }

    fn delete_gemini_api_key(&self) -> Result<(), ConfigError> {
        let entry = keyring::Entry::new(&self.service_name, &self.username)?;
        match entry.delete_credential() {
            Ok(_) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(ConfigError::Keyring(e)),
        }
    }
}
