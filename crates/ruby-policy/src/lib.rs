//! Fail-closed security boundaries, path traversal prevention, SSRF filtering,
//! and the ConfirmationBroker for Ruby Assistant.

use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("Path outside allowed roots: {0}")]
    PathDisallowed(PathBuf),
    #[error("Access to sensitive directory blocked: {0}")]
    SensitiveDirectoryBlocked(PathBuf),
    #[error("SSRF: Access to blocked address/host: {0}")]
    SsrfBlocked(String),
    #[error("User rejected action or confirmation timed out: {0}")]
    ConfirmationDenied(String),
}

pub trait SecurityPolicy: Send + Sync {
    fn validate_read_path(&self, path: &Path) -> Result<PathBuf, PolicyError>;
    fn validate_write_path(&self, path: &Path) -> Result<PathBuf, PolicyError>;
    fn validate_url(&self, url: &url::Url) -> Result<(), PolicyError>;
}

#[derive(Debug, Clone)]
pub struct DefaultSecurityPolicy {
    allowed_roots: Vec<PathBuf>,
}

impl DefaultSecurityPolicy {
    pub fn new(allowed_roots: Vec<PathBuf>) -> Self {
        Self { allowed_roots }
    }

    fn check_sensitive(&self, path: &Path) -> Result<(), PolicyError> {
        let p_str = path.to_string_lossy().to_lowercase();
        if p_str.contains("\\windows\\")
            || p_str.contains("/windows/")
            || p_str.contains(".ssh")
            || p_str.contains(".env")
            || p_str.contains("credentials")
        {
            return Err(PolicyError::SensitiveDirectoryBlocked(path.to_path_buf()));
        }
        Ok(())
    }
}

impl SecurityPolicy for DefaultSecurityPolicy {
    fn validate_read_path(&self, path: &Path) -> Result<PathBuf, PolicyError> {
        let canonical = match path.canonicalize() {
            Ok(c) => c,
            Err(_) => path.to_path_buf(),
        };
        self.check_sensitive(&canonical)?;
        for root in &self.allowed_roots {
            if let Ok(c_root) = root.canonicalize() {
                if canonical.starts_with(&c_root) {
                    return Ok(canonical);
                }
            } else if canonical.starts_with(root) {
                return Ok(canonical);
            }
        }
        Err(PolicyError::PathDisallowed(path.to_path_buf()))
    }

    fn validate_write_path(&self, path: &Path) -> Result<PathBuf, PolicyError> {
        self.validate_read_path(path)
    }

    fn validate_url(&self, url: &url::Url) -> Result<(), PolicyError> {
        let host = url.host_str().unwrap_or_default();
        if host == "localhost"
            || host == "127.0.0.1"
            || host == "::1"
            || host == "169.254.169.254"
            || host.ends_with(".internal")
        {
            return Err(PolicyError::SsrfBlocked(host.to_string()));
        }
        Ok(())
    }
}

#[async_trait::async_trait]
pub trait ConfirmationBroker: Send + Sync {
    async fn request_confirmation(&self, action_description: &str) -> Result<bool, PolicyError>;
}
