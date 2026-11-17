//! Fail-closed security boundaries, path traversal prevention, SSRF filtering,
//! and the ConfirmationBroker for Ruby Assistant.

use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("Path outside allowed roots: {0}")]
    PathDisallowed(PathBuf),
    #[error("Access to sensitive directory or file blocked: {0}")]
    SensitiveDirectoryBlocked(PathBuf),
    #[error("Alternate data stream or device path blocked: {0}")]
    AlternateDataStreamBlocked(PathBuf),
    #[error("UNC network path blocked: {0}")]
    UncPathBlocked(PathBuf),
    #[error("Path canonicalization failed: {0}")]
    CanonicalizationFailed(PathBuf),
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
    /// Creates a security policy with configurable allowed roots.
    /// Default empty vector enforces fail-closed: deny everything.
    pub fn new(allowed_roots: Vec<PathBuf>) -> Self {
        Self { allowed_roots }
    }

    /// Helper to check ADS (Alternate Data Streams), device paths, and UNC paths before canonicalization.
    fn check_path_syntax(&self, path: &Path) -> Result<(), PolicyError> {
        let path_str = path.to_string_lossy();

        // 1. UNC paths (e.g. \\server\share or //server/share)
        if (path_str.starts_with(r"\\") || path_str.starts_with("//"))
            && !path_str.starts_with(r"\\?\")
        {
            return Err(PolicyError::UncPathBlocked(path.to_path_buf()));
        }

        // 2. Alternate Data Streams: colon after Windows drive letter or ::$DATA
        let drive_stripped = if path_str.len() >= 2 && path_str.chars().nth(1) == Some(':') {
            &path_str[2..]
        } else if path_str.starts_with(r"\\?\")
            && path_str.len() >= 6
            && path_str.chars().nth(5) == Some(':')
        {
            &path_str[6..]
        } else {
            &path_str[..]
        };

        if drive_stripped.contains(':') {
            return Err(PolicyError::AlternateDataStreamBlocked(path.to_path_buf()));
        }

        // 3. DOS device names (CON, PRN, AUX, NUL, COM1..9, LPT1..9)
        for comp in path.components() {
            let name = comp.as_os_str().to_string_lossy().to_ascii_uppercase();
            let base_name = name.split('.').next().unwrap_or("");
            if matches!(
                base_name,
                "CON"
                    | "PRN"
                    | "AUX"
                    | "NUL"
                    | "COM1"
                    | "COM2"
                    | "COM3"
                    | "COM4"
                    | "COM5"
                    | "COM6"
                    | "COM7"
                    | "COM8"
                    | "COM9"
                    | "LPT1"
                    | "LPT2"
                    | "LPT3"
                    | "LPT4"
                    | "LPT5"
                    | "LPT6"
                    | "LPT7"
                    | "LPT8"
                    | "LPT9"
            ) {
                return Err(PolicyError::AlternateDataStreamBlocked(path.to_path_buf()));
            }
        }

        Ok(())
    }

    /// Checks canonical paths against blocked sensitive directories and secret files.
    fn check_sensitive(&self, path: &Path) -> Result<(), PolicyError> {
        let p_str = path
            .to_string_lossy()
            .to_ascii_lowercase()
            .replace('/', "\\");

        // .env files
        if p_str.ends_with("\\.env") || p_str.contains("\\.env.") || p_str.contains("\\.env\\") {
            return Err(PolicyError::SensitiveDirectoryBlocked(path.to_path_buf()));
        }

        // .ssh directory or keys
        if p_str.contains("\\.ssh\\") || p_str.ends_with("\\.ssh") {
            return Err(PolicyError::SensitiveDirectoryBlocked(path.to_path_buf()));
        }

        // .gemini credentials
        if p_str.contains("\\.gemini\\credentials")
            || p_str.contains("\\.gemini\\config")
            || p_str.contains("\\.gemini\\antigravity\\secrets")
        {
            return Err(PolicyError::SensitiveDirectoryBlocked(path.to_path_buf()));
        }

        // Browser profiles
        if p_str.contains("\\google\\chrome\\user data")
            || p_str.contains("\\mozilla\\firefox\\profiles")
            || p_str.contains("\\microsoft\\edge\\user data")
            || p_str.contains("\\bravesoftware\\brave-browser\\user data")
            || p_str.contains("\\opera software\\")
        {
            return Err(PolicyError::SensitiveDirectoryBlocked(path.to_path_buf()));
        }

        // Windows Credential Manager & Vault
        if p_str.contains("\\microsoft\\credentials")
            || p_str.contains("\\microsoft\\vault")
            || p_str.contains("\\microsoft\\protect")
        {
            return Err(PolicyError::SensitiveDirectoryBlocked(path.to_path_buf()));
        }

        // %APPDATA% secrets & cloud credentials
        if p_str.contains("\\appdata\\")
            && (p_str.contains("\\.aws")
                || p_str.contains("\\.azure")
                || p_str.contains("\\.kube")
                || p_str.contains("\\secrets")
                || p_str.contains("\\keyring")
                || p_str.ends_with("\\credentials")
                || p_str.ends_with("\\token.json"))
        {
            return Err(PolicyError::SensitiveDirectoryBlocked(path.to_path_buf()));
        }

        // System directories
        if p_str.contains("\\windows\\system32")
            || p_str.contains("\\windows\\syswow64")
            || p_str.contains("\\windows\\winxs")
        {
            return Err(PolicyError::SensitiveDirectoryBlocked(path.to_path_buf()));
        }

        Ok(())
    }
}

impl SecurityPolicy for DefaultSecurityPolicy {
    fn validate_read_path(&self, path: &Path) -> Result<PathBuf, PolicyError> {
        // Enforce fail-closed: if allowed roots list is empty, deny everything
        if self.allowed_roots.is_empty() {
            return Err(PolicyError::PathDisallowed(path.to_path_buf()));
        }

        // Check syntax (ADS, UNC, DOS devices)
        self.check_path_syntax(path)?;

        // Resolve symlinks, junctions, and .. via canonicalize
        let canonical = path
            .canonicalize()
            .map_err(|_| PolicyError::CanonicalizationFailed(path.to_path_buf()))?;

        // Check sensitive paths on canonicalized path
        self.check_sensitive(&canonical)?;

        // Ensure canonical path starts with at least one canonicalized allowed root
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
        let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
        if host == "localhost"
            || host == "127.0.0.1"
            || host == "::1"
            || host == "169.254.169.254"
            || host.ends_with(".internal")
            || host.ends_with(".local")
        {
            return Err(PolicyError::SsrfBlocked(host));
        }

        if let Ok(ip) = host.parse::<std::net::IpAddr>() {
            match ip {
                std::net::IpAddr::V4(v4) => {
                    if v4.is_loopback() || v4.is_private() || v4.is_link_local() {
                        return Err(PolicyError::SsrfBlocked(host));
                    }
                }
                std::net::IpAddr::V6(v6) => {
                    if v6.is_loopback() {
                        return Err(PolicyError::SsrfBlocked(host));
                    }
                }
            }
        }

        Ok(())
    }
}

#[async_trait::async_trait]
pub trait ConfirmationBroker: Send + Sync {
    async fn request_confirmation(&self, action_description: &str) -> Result<bool, PolicyError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_default_empty_roots_denies_everything() {
        let policy = DefaultSecurityPolicy::new(vec![]);
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "sample").unwrap();

        let res = policy.validate_read_path(&file_path);
        assert_eq!(res, Err(PolicyError::PathDisallowed(file_path)));
    }

    #[test]
    fn test_allowed_path_inside_root_succeeds() {
        let temp = tempdir().unwrap();
        let allowed_root = temp.path().to_path_buf();
        let file_path = allowed_root.join("data.txt");
        fs::write(&file_path, "hello").unwrap();

        let policy = DefaultSecurityPolicy::new(vec![allowed_root]);
        let res = policy.validate_read_path(&file_path);
        assert!(res.is_ok());
    }

    #[test]
    fn test_path_traversal_double_dot_blocked() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("allowed");
        fs::create_dir_all(&root).unwrap();

        let outside_dir = temp.path().join("outside");
        fs::create_dir_all(&outside_dir).unwrap();
        let outside_file = outside_dir.join("secret.txt");
        fs::write(&outside_file, "secret").unwrap();

        let policy = DefaultSecurityPolicy::new(vec![root.clone()]);

        // Attempt traversal: allowed/../outside/secret.txt
        let traversal_path = root.join("..").join("outside").join("secret.txt");
        let res = policy.validate_read_path(&traversal_path);
        assert_eq!(res, Err(PolicyError::PathDisallowed(traversal_path)));
    }

    #[test]
    fn test_symlink_or_junction_resolved_and_enforced() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("allowed");
        fs::create_dir_all(&root).unwrap();

        let outside_dir = temp.path().join("outside");
        fs::create_dir_all(&outside_dir).unwrap();
        let outside_file = outside_dir.join("real_secret.txt");
        fs::write(&outside_file, "secret").unwrap();

        let link_path = root.join("link_to_outside.txt");
        #[cfg(windows)]
        {
            // Create symlink if privileges allow, or check canonicalization handling
            let _ = std::os::windows::fs::symlink_file(&outside_file, &link_path);
        }
        #[cfg(unix)]
        {
            let _ = std::os::unix::fs::symlink(&outside_file, &link_path);
        }

        let policy = DefaultSecurityPolicy::new(vec![root]);
        if link_path.exists() {
            let res = policy.validate_read_path(&link_path);
            assert_eq!(res, Err(PolicyError::PathDisallowed(link_path)));
        }
    }

    #[test]
    fn test_blocked_env_file() {
        let temp = tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let env_file = root.join(".env");
        fs::write(&env_file, "SECRET_KEY=123").unwrap();

        let policy = DefaultSecurityPolicy::new(vec![root.clone()]);
        let res = policy.validate_read_path(&env_file);
        assert_eq!(
            res,
            Err(PolicyError::SensitiveDirectoryBlocked(
                env_file.canonicalize().unwrap()
            ))
        );

        let env_local = root.join(".env.local");
        fs::write(&env_local, "SECRET=456").unwrap();
        let res_local = policy.validate_read_path(&env_local);
        assert_eq!(
            res_local,
            Err(PolicyError::SensitiveDirectoryBlocked(
                env_local.canonicalize().unwrap()
            ))
        );
    }

    #[test]
    fn test_blocked_ssh_directory() {
        let temp = tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let ssh_dir = root.join(".ssh");
        fs::create_dir_all(&ssh_dir).unwrap();
        let key_file = ssh_dir.join("id_rsa");
        fs::write(&key_file, "ssh-key").unwrap();

        let policy = DefaultSecurityPolicy::new(vec![root]);
        let res = policy.validate_read_path(&key_file);
        assert_eq!(
            res,
            Err(PolicyError::SensitiveDirectoryBlocked(
                key_file.canonicalize().unwrap()
            ))
        );
    }

    #[test]
    fn test_blocked_gemini_credentials() {
        let temp = tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let gemini_dir = root.join(".gemini");
        fs::create_dir_all(&gemini_dir).unwrap();
        let cred_file = gemini_dir.join("credentials.json");
        fs::write(&cred_file, "{}").unwrap();

        let policy = DefaultSecurityPolicy::new(vec![root]);
        let res = policy.validate_read_path(&cred_file);
        assert_eq!(
            res,
            Err(PolicyError::SensitiveDirectoryBlocked(
                cred_file.canonicalize().unwrap()
            ))
        );
    }

    #[test]
    fn test_blocked_browser_profiles() {
        let temp = tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let chrome_dir = root.join("Google").join("Chrome").join("User Data");
        fs::create_dir_all(&chrome_dir).unwrap();
        let profile_file = chrome_dir.join("Default");
        fs::write(&profile_file, "data").unwrap();

        let policy = DefaultSecurityPolicy::new(vec![root]);
        let res = policy.validate_read_path(&profile_file);
        assert_eq!(
            res,
            Err(PolicyError::SensitiveDirectoryBlocked(
                profile_file.canonicalize().unwrap()
            ))
        );
    }

    #[test]
    fn test_blocked_credential_vault() {
        let temp = tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let vault_dir = root.join("Microsoft").join("Vault");
        fs::create_dir_all(&vault_dir).unwrap();
        let vault_file = vault_dir.join("token");
        fs::write(&vault_file, "vault_token").unwrap();

        let policy = DefaultSecurityPolicy::new(vec![root]);
        let res = policy.validate_read_path(&vault_file);
        assert_eq!(
            res,
            Err(PolicyError::SensitiveDirectoryBlocked(
                vault_file.canonicalize().unwrap()
            ))
        );
    }

    #[test]
    fn test_blocked_appdata_secrets() {
        let temp = tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let appdata_dir = root.join("AppData").join("Roaming").join(".aws");
        fs::create_dir_all(&appdata_dir).unwrap();
        let aws_cred = appdata_dir.join("credentials");
        fs::write(&aws_cred, "[default]").unwrap();

        let policy = DefaultSecurityPolicy::new(vec![root]);
        let res = policy.validate_read_path(&aws_cred);
        assert_eq!(
            res,
            Err(PolicyError::SensitiveDirectoryBlocked(
                aws_cred.canonicalize().unwrap()
            ))
        );
    }

    #[test]
    fn test_blocked_alternate_data_stream_and_dos_devices() {
        let temp = tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let policy = DefaultSecurityPolicy::new(vec![root.clone()]);

        let ads_path = root.join("test.txt:stream");
        let res = policy.validate_read_path(&ads_path);
        assert_eq!(res, Err(PolicyError::AlternateDataStreamBlocked(ads_path)));

        let nul_device = PathBuf::from("NUL");
        let res_nul = policy.validate_read_path(&nul_device);
        assert_eq!(
            res_nul,
            Err(PolicyError::AlternateDataStreamBlocked(nul_device))
        );
    }

    #[test]
    fn test_blocked_unc_path() {
        let temp = tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let policy = DefaultSecurityPolicy::new(vec![root]);

        let unc_path = PathBuf::from(r"\\evil-server\share\file.txt");
        let res = policy.validate_read_path(&unc_path);
        assert_eq!(res, Err(PolicyError::UncPathBlocked(unc_path)));
    }

    #[test]
    fn test_ssrf_filtering() {
        let policy = DefaultSecurityPolicy::new(vec![]);
        assert!(policy
            .validate_url(&url::Url::parse("http://localhost:8080").unwrap())
            .is_err());
        assert!(policy
            .validate_url(&url::Url::parse("http://127.0.0.1:3000").unwrap())
            .is_err());
        assert!(policy
            .validate_url(&url::Url::parse("http://169.254.169.254/latest").unwrap())
            .is_err());
        assert!(policy
            .validate_url(&url::Url::parse("http://internal.service.local").unwrap())
            .is_err());
        assert!(policy
            .validate_url(&url::Url::parse("https://api.github.com/repos").unwrap())
            .is_ok());
    }
}
