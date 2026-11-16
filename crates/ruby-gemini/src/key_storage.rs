//! Secure API Key retrieval using Windows Credential Manager (keyring) with .env fallback.
//! Never logs, leaks, or persists keys to version control.

use keyring::Entry;
use tracing::{debug, warn};

const KEYRING_SERVICE: &str = "ruby-agent";
const KEYRING_USER: &str = "gemini-api-key";

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
