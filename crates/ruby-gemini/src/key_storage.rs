//! Secure API Key retrieval using Windows Credential Manager (keyring) with .env fallback.
//! Never logs, leaks, or persists keys to version control.

use keyring::Entry;
use tracing::{debug, warn};

const KEYRING_SERVICE: &str = "ruby-agent";
const KEYRING_USER: &str = "gemini-api-key";
const FREELLMAPI_KEYRING_USER: &str = "freellmapi-api-key";
pub const DEFAULT_FREELLMAPI_BASE_URL: &str = "http://localhost:3001/v1";

/// Retrieves the Gemini API key from Windows Credential Manager, falling back to environment variable.
pub fn get_gemini_api_key() -> Option<String> {
    // 1. Try Windows Credential Manager via keyring
    if let Ok(entry) = Entry::new(KEYRING_SERVICE, KEYRING_USER) {
        if let Ok(password) = entry.get_password() {
            let trimmed = password.trim();
            if !trimmed.is_empty() {
                debug!("Retrieved Gemini API key from Windows Credential Manager");
                return Some(trimmed.to_string());
            }
        }
    }

    // 2. Fall back to GEMINI_API_KEY environment variable
    if let Ok(env_key) = std::env::var("GEMINI_API_KEY") {
        let trimmed = env_key.trim();
        if !trimmed.is_empty() {
            debug!("Retrieved Gemini API key from environment variable GEMINI_API_KEY");
            return Some(trimmed.to_string());
        }
    }

    warn!("Gemini API key not found in Windows Credential Manager or GEMINI_API_KEY environment variable");
    None
}

/// Stores a Gemini API key into Windows Credential Manager.
pub fn set_gemini_api_key(api_key: &str) -> Result<(), String> {
    let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| format!("Failed to access keyring: {:?}", e))?;
    entry
        .set_password(api_key.trim())
        .map_err(|e| format!("Failed to store key in Windows Credential Manager: {:?}", e))?;
    debug!("Successfully saved Gemini API key to Windows Credential Manager");
    Ok(())
}

/// Retrieves the FreeLLMAPI API key from Windows Credential Manager, falling back to environment variable.
pub fn get_freellmapi_api_key() -> Option<String> {
    // 1. Try Windows Credential Manager via keyring
    if let Ok(entry) = Entry::new(KEYRING_SERVICE, FREELLMAPI_KEYRING_USER) {
        if let Ok(password) = entry.get_password() {
            let trimmed = password.trim();
            if !trimmed.is_empty() {
                debug!("Retrieved FreeLLMAPI API key from Windows Credential Manager");
                return Some(trimmed.to_string());
            }
        }
    }

    // 2. Fall back to FREELLMAPI_API_KEY environment variable
    if let Ok(env_key) = std::env::var("FREELLMAPI_API_KEY") {
        let trimmed = env_key.trim();
        if !trimmed.is_empty() {
            debug!("Retrieved FreeLLMAPI API key from environment variable FREELLMAPI_API_KEY");
            return Some(trimmed.to_string());
        }
    }

    None
}

/// Stores a FreeLLMAPI API key into Windows Credential Manager.
pub fn set_freellmapi_api_key(api_key: &str) -> Result<(), String> {
    let entry = Entry::new(KEYRING_SERVICE, FREELLMAPI_KEYRING_USER)
        .map_err(|e| format!("Failed to access keyring: {:?}", e))?;
    entry
        .set_password(api_key.trim())
        .map_err(|e| format!("Failed to store key in Windows Credential Manager: {:?}", e))?;
    debug!("Successfully saved FreeLLMAPI API key to Windows Credential Manager");
    Ok(())
}

/// Resolves the FreeLLMAPI base URL from FREELLMAPI_BASE_URL env var or default.
pub fn get_freellmapi_base_url() -> String {
    std::env::var("FREELLMAPI_BASE_URL")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_FREELLMAPI_BASE_URL.to_string())
}

/// Checks whether third-party fallback (FreeLLMAPI) is explicitly allowed.
/// Default is false.
pub fn get_allow_third_party_fallback() -> bool {
    std::env::var("ALLOW_THIRD_PARTY_FALLBACK")
        .ok()
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            v == "1" || v == "true" || v == "yes" || v == "on"
        })
        .unwrap_or(false)
}
