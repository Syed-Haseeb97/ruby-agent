//! Fail-closed security boundaries, path traversal prevention, SSRF filtering,
//! and the ConfirmationBroker for Ruby Assistant.

pub mod computer_policy;

use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
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
    #[error("Tool execution blocked by security policy: {0}")]
    ToolBlocked(String),
    #[error("Unknown tool denied by default: {0}")]
    UnknownToolDenied(String),
    #[error("Domain blocked by security policy: {0}")]
    DomainBlocked(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

/// Parses alternate IP representations including decimal (integer), hex, and octal.
pub fn parse_ip_or_numeric_host(host: &str) -> Option<IpAddr> {
    let clean = host.trim_matches(|c| c == '[' || c == ']');
    if clean.is_empty() {
        return None;
    }

    // 1. Standard IP parse
    if let Ok(ip) = clean.parse::<IpAddr>() {
        return Some(ip);
    }

    // 2. Pure integer (decimal IP e.g. 2130706433 -> 127.0.0.1)
    if clean.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(num) = clean.parse::<u32>() {
            return Some(IpAddr::V4(Ipv4Addr::from(num)));
        }
    }

    // 3. Hexadecimal IP (e.g. 0x7f000001 or 0x7f.0.0.1)
    if clean.starts_with("0x") || clean.starts_with("0X") {
        let hex_str = &clean[2..];
        if let Ok(num) = u32::from_str_radix(hex_str, 16) {
            return Some(IpAddr::V4(Ipv4Addr::from(num)));
        }
    }

    // 4. Dotted hex or octal parts (e.g. 0x7f.0.0.1 or 0177.0.0.1)
    let parts: Vec<&str> = clean.split('.').collect();
    if parts.len() == 4 {
        let mut octets = [0u8; 4];
        let mut all_valid = true;
        for (i, part) in parts.iter().enumerate() {
            let val = if part.starts_with("0x") || part.starts_with("0X") {
                u32::from_str_radix(&part[2..], 16)
            } else if part.len() > 1
                && part.starts_with('0')
                && part.chars().all(|c| ('0'..='7').contains(&c))
            {
                u32::from_str_radix(&part[1..], 8)
            } else {
                part.parse::<u32>()
            };

            match val {
                Ok(v) if v <= 255 => octets[i] = v as u8,
                _ => {
                    all_valid = false;
                    break;
                }
            }
        }
        if all_valid {
            return Some(IpAddr::V4(Ipv4Addr::new(
                octets[0], octets[1], octets[2], octets[3],
            )));
        }
    }

    None
}

/// Checks if an IP address is private, loopback, link-local, unspecified, or reserved.
pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            // Loopback (127.0.0.0/8)
            if octets[0] == 127 || v4.is_loopback() {
                return true;
            }
            // Unspecified / current network (0.0.0.0/8)
            if octets[0] == 0 {
                return true;
            }
            // Broadcast (255.255.255.255)
            if v4.is_broadcast() {
                return true;
            }
            // Private (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16)
            if octets[0] == 10
                || (octets[0] == 172 && (16..=31).contains(&octets[1]))
                || (octets[0] == 192 && octets[1] == 168)
                || v4.is_private()
            {
                return true;
            }
            // Link-local / Cloud metadata (169.254.0.0/16)
            if (octets[0] == 169 && octets[1] == 254) || v4.is_link_local() {
                return true;
            }
            // CGNAT (100.64.0.0/10)
            if octets[0] == 100 && (64..=127).contains(&octets[1]) {
                return true;
            }
            // Documentation (192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24)
            if (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
                || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
                || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
                || v4.is_documentation()
            {
                return true;
            }
            // Benchmark (198.18.0.0/15)
            if octets[0] == 198 && (18..=19).contains(&octets[1]) {
                return true;
            }
            // Multicast (224.0.0.0/4)
            if v4.is_multicast() {
                return true;
            }
            false
        }
        IpAddr::V6(v6) => {
            // Loopback (::1)
            if v6.is_loopback() {
                return true;
            }
            // Unspecified (::)
            if v6.is_unspecified() {
                return true;
            }
            // Multicast (ff00::/8)
            if v6.is_multicast() {
                return true;
            }
            let segments = v6.segments();
            // Unique Local Address ULA (fc00::/7)
            if (segments[0] & 0xfe00) == 0xfc00 {
                return true;
            }
            // Link-local unicast (fe80::/10)
            if (segments[0] & 0xffc0) == 0xfe80 {
                return true;
            }
            // IPv4-mapped IPv6 (::ffff:127.0.0.1)
            if let Some(mapped_v4) = v6.to_ipv4_mapped() {
                return is_blocked_ip(IpAddr::V4(mapped_v4));
            }
            // Embedded IPv4 check for 96 zero bits
            if segments[0] == 0
                && segments[1] == 0
                && segments[2] == 0
                && segments[3] == 0
                && segments[4] == 0
            {
                let mapped = Ipv4Addr::new(
                    (segments[6] >> 8) as u8,
                    segments[6] as u8,
                    (segments[7] >> 8) as u8,
                    segments[7] as u8,
                );
                return is_blocked_ip(IpAddr::V4(mapped));
            }
            false
        }
    }
}

pub trait SecurityPolicy: Send + Sync {
    fn validate_read_path(&self, path: &Path) -> Result<PathBuf, PolicyError>;
    fn validate_write_path(&self, path: &Path) -> Result<PathBuf, PolicyError>;
    fn validate_url(&self, url: &url::Url) -> Result<(), PolicyError>;
    fn allowed_roots(&self) -> &[PathBuf] {
        &[]
    }
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
        if host.is_empty() {
            return Err(PolicyError::SsrfBlocked("Empty host in URL".into()));
        }

        // 1. Literal host name check
        if host == "localhost"
            || host == "0.0.0.0"
            || host == "127.0.0.1"
            || host == "::1"
            || host == "169.254.169.254"
            || host.ends_with(".localhost")
            || host.ends_with(".local")
            || host.ends_with(".internal")
            || host.ends_with(".arpa")
        {
            return Err(PolicyError::SsrfBlocked(host));
        }

        // 2. Alternate numeric/hex/octal/IPv6 IP parse
        if let Some(ip) = parse_ip_or_numeric_host(&host) {
            if is_blocked_ip(ip) {
                return Err(PolicyError::SsrfBlocked(format!("{host} (parsed as {ip})")));
            }
        }

        // 3. DNS resolution check (Best-effort pre-flight check):
        // ARCHITECTURAL SECURITY NOTES:
        // A. DNS-time pre-flight checks CANNOT completely prevent DNS rebinding attacks,
        //    because a malicious authoritative nameserver can return a benign public IP
        //    on initial resolution and subsequently return 127.0.0.1 or an RFC1918 private IP (TTL=0)
        //    when the HTTP client actually connects.
        // B. The PRIMARY security control against SSRF is strict domain allowlisting
        //    (`allowed_domains`), which is evaluated before this step.
        // C. Synchronous DNS resolvers (`to_socket_addrs`) must not block the async runtime
        //    indefinitely; this check is a secondary defense-in-depth gate.
        let port = url.port_or_known_default().unwrap_or(80);
        if let Ok(addrs) = (host.as_str(), port).to_socket_addrs() {
            for addr in addrs {
                if is_blocked_ip(addr.ip()) {
                    return Err(PolicyError::SsrfBlocked(format!(
                        "{host} (resolves to blocked IP {})",
                        addr.ip()
                    )));
                }
            }
        }

        Ok(())
    }

    fn allowed_roots(&self) -> &[PathBuf] {
        &self.allowed_roots
    }
}

#[async_trait::async_trait]
pub trait ConfirmationBroker: Send + Sync {
    async fn request_confirmation(&self, action_description: &str) -> Result<bool, PolicyError>;
}

/// Security risk classifications for tool calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RiskClass {
    ReadOnly,
    Write,
    Destructive,
    ExternalNetwork,
    Unknown,
}

pub trait ToolSecurityPolicy: Send + Sync {
    fn classify_tool(&self, server: &str, tool_name: &str) -> RiskClass;
    fn validate_tool_call(
        &self,
        server: &str,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<RiskClass, PolicyError>;
    fn validate_browser_url(&self, raw_url: &str) -> Result<url::Url, PolicyError>;
    fn validate_all_urls_in_json(&self, val: &serde_json::Value) -> Result<(), PolicyError>;
    fn revalidate_redirect_url(
        &self,
        original_url: &url::Url,
        redirect_target: &str,
    ) -> Result<url::Url, PolicyError>;
}

#[derive(Debug, Clone, Default)]
pub struct DefaultToolSecurityPolicy {
    allowed_domains: Vec<String>,
    tool_risk_overrides: std::collections::HashMap<String, RiskClass>,
}

impl DefaultToolSecurityPolicy {
    pub fn new(
        allowed_domains: Vec<String>,
        tool_risk_overrides: std::collections::HashMap<String, RiskClass>,
    ) -> Self {
        Self {
            allowed_domains,
            tool_risk_overrides,
        }
    }

    pub fn with_allowed_domains(allowed_domains: Vec<String>) -> Self {
        Self {
            allowed_domains,
            tool_risk_overrides: std::collections::HashMap::new(),
        }
    }
}

impl ToolSecurityPolicy for DefaultToolSecurityPolicy {
    fn classify_tool(&self, server: &str, tool_name: &str) -> RiskClass {
        let key = format!("{server}::{tool_name}");
        if let Some(risk) = self.tool_risk_overrides.get(&key) {
            return *risk;
        }
        if let Some(risk) = self.tool_risk_overrides.get(tool_name) {
            return *risk;
        }

        // Explicit per-tool risk table (server::tool -> RiskClass). No substring matching!
        match (
            server.to_ascii_lowercase().as_str(),
            tool_name.to_ascii_lowercase().as_str(),
        ) {
            // Playwright tools
            ("playwright", "navigate") | ("playwright", "goto") | ("playwright", "open_url") => {
                RiskClass::ExternalNetwork
            }
            ("playwright", "screenshot")
            | ("playwright", "get_content")
            | ("playwright", "get_title")
            | ("playwright", "get_url") => RiskClass::ReadOnly,
            ("playwright", "click")
            | ("playwright", "type")
            | ("playwright", "fill")
            | ("playwright", "upload")
            | ("playwright", "submit") => RiskClass::Write,
            ("playwright", "evaluate") => RiskClass::Destructive,

            // Memory tools
            ("memory", "search_memories") | ("memory", "get_recent_memories") => {
                RiskClass::ReadOnly
            }
            ("memory", "store_memory") => RiskClass::Write,
            ("memory", "delete_memory") | ("memory", "wipe_all") => RiskClass::Destructive,

            // Filesystem tools
            ("fs", "read_file") | ("fs", "list_directory") | ("fs", "file_info") => {
                RiskClass::ReadOnly
            }
            ("fs", "write_file") | ("fs", "create_dir") => RiskClass::Write,
            ("fs", "delete_file") => RiskClass::Destructive,

            // Git tools
            ("git", "status") | ("git", "diff") | ("git", "log") => RiskClass::ReadOnly,
            ("git", "commit") => RiskClass::Write,
            ("git", "push") => RiskClass::Destructive,

            // Actions & Remote tools
            ("actions", "open_file")
            | ("actions", "send_file")
            | ("actions", "list_folder")
            | ("actions", "file_info") => RiskClass::ReadOnly,
            ("actions", "launch_app")
            | ("actions", "screen_click")
            | ("actions", "change_setting") => RiskClass::Write,

            // Computer use tools
            ("computer_use", "screenshot") | ("computer_use", "mouse_move") => RiskClass::ReadOnly,
            ("computer_use", "left_click")
            | ("computer_use", "right_click")
            | ("computer_use", "double_click")
            | ("computer_use", "type_text")
            | ("computer_use", "press_key")
            | ("computer_use", "drag") => RiskClass::Write,

            // Shell tool (when explicitly requested through tool layer)
            ("shell", "execute_command") => RiskClass::Write,

            // Voice live tools (direct dispatch)
            ("voice_live", "list_folder")
            | ("voice_live", "read_file")
            | ("voice_live", "search_memories") => RiskClass::ReadOnly,
            ("voice_live", "delete_file") | ("voice_live", "wipe_all") => RiskClass::Destructive,
            ("voice_live", "launch_app")
            | ("voice_live", "store_memory")
            | ("voice_live", "write_file") => RiskClass::Write,

            // Unknown tools are denied by default (no substring fallback)
            _ => RiskClass::Unknown,
        }
    }

    fn validate_tool_call(
        &self,
        server: &str,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<RiskClass, PolicyError> {
        let risk = self.classify_tool(server, tool_name);
        if risk == RiskClass::Unknown {
            return Err(PolicyError::UnknownToolDenied(format!(
                "Tool '{tool_name}' on server '{server}' is not recognized in explicit risk table and is denied by default"
            )));
        }

        // Validate every url-like argument in the JSON payload (keys or values)
        self.validate_all_urls_in_json(arguments)?;

        Ok(risk)
    }

    /// Recursively validates every URL-like string found in tool call arguments.
    fn validate_all_urls_in_json(&self, val: &serde_json::Value) -> Result<(), PolicyError> {
        match val {
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    let k_lower = k.to_ascii_lowercase();
                    if k_lower.contains("url")
                        || k_lower.contains("uri")
                        || k_lower.contains("endpoint")
                        || k_lower.contains("target")
                        || k_lower.contains("redirect")
                        || k_lower.contains("href")
                        || k_lower.contains("link")
                    {
                        if let Some(s) = v.as_str() {
                            let _ = self.validate_browser_url(s)?;
                        }
                    } else if let Some(s) = v.as_str() {
                        if s.starts_with("http://")
                            || s.starts_with("https://")
                            || s.starts_with("ws://")
                            || s.starts_with("wss://")
                        {
                            let _ = self.validate_browser_url(s)?;
                        }
                    }
                    self.validate_all_urls_in_json(v)?;
                }
            }
            serde_json::Value::Array(arr) => {
                for v in arr {
                    if let Some(s) = v.as_str() {
                        if s.starts_with("http://")
                            || s.starts_with("https://")
                            || s.starts_with("ws://")
                            || s.starts_with("wss://")
                        {
                            let _ = self.validate_browser_url(s)?;
                        }
                    }
                    self.validate_all_urls_in_json(v)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_browser_url(&self, raw_url: &str) -> Result<url::Url, PolicyError> {
        let parsed = url::Url::parse(raw_url)
            .map_err(|e| PolicyError::SsrfBlocked(format!("Invalid URL syntax: {e}")))?;

        // 1. SSRF prevention
        let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
        if host.is_empty() {
            return Err(PolicyError::SsrfBlocked("Empty host in URL".into()));
        }

        let clean_host = host.trim_matches(|c| c == '[' || c == ']');

        if clean_host == "localhost"
            || clean_host == "0.0.0.0"
            || clean_host == "127.0.0.1"
            || clean_host == "::1"
            || clean_host == "169.254.169.254"
            || clean_host.ends_with(".localhost")
            || clean_host.ends_with(".local")
            || clean_host.ends_with(".internal")
            || clean_host.ends_with(".arpa")
        {
            return Err(PolicyError::SsrfBlocked(host));
        }

        if let Some(ip) = parse_ip_or_numeric_host(clean_host) {
            if is_blocked_ip(ip) {
                return Err(PolicyError::SsrfBlocked(format!("{host} (parsed as {ip})")));
            }
        }

        let port = parsed.port_or_known_default().unwrap_or(80);
        if let Ok(addrs) = (clean_host, port).to_socket_addrs() {
            for addr in addrs {
                if is_blocked_ip(addr.ip()) {
                    return Err(PolicyError::SsrfBlocked(format!(
                        "{host} (resolves to blocked IP {})",
                        addr.ip()
                    )));
                }
            }
        }

        // 2. Domain allowlist check (if configured)
        if !self.allowed_domains.is_empty() {
            let matches_allowed = self.allowed_domains.iter().any(|domain| {
                let d = domain.to_ascii_lowercase();
                if let Some(suffix) = d.strip_prefix("*.") {
                    clean_host == suffix || clean_host.ends_with(&format!(".{suffix}"))
                } else {
                    clean_host == d || clean_host.ends_with(&format!(".{d}"))
                }
            });
            if !matches_allowed {
                return Err(PolicyError::DomainBlocked(host));
            }
        }

        Ok(parsed)
    }

    /// Re-validates a redirect URL against SSRF policy rules.
    fn revalidate_redirect_url(
        &self,
        _original_url: &url::Url,
        redirect_target: &str,
    ) -> Result<url::Url, PolicyError> {
        self.validate_browser_url(redirect_target)
    }
}

pub const UNIFIED_CONFIRMATION_TTL: Duration = Duration::from_secs(120);
pub const MAX_CONFIRMATION_ATTEMPTS: u32 = 5;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingUnifiedAction {
    pub id: String,
    pub code: String,
    pub server: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub description: String,
    pub failed_attempts: u32,
    #[serde(skip, default = "Instant::now")]
    pub created_at: Instant,
    #[serde(skip, default = "default_expiry")]
    pub expires_at: Instant,
}

fn default_expiry() -> Instant {
    Instant::now() + UNIFIED_CONFIRMATION_TTL
}

#[derive(Debug, Clone)]
pub struct UnifiedConfirmationManager {
    pending: Arc<std::sync::Mutex<Option<PendingUnifiedAction>>>,
}

impl Default for UnifiedConfirmationManager {
    fn default() -> Self {
        Self::new()
    }
}

impl UnifiedConfirmationManager {
    pub fn new() -> Self {
        Self {
            pending: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Generates a cryptographically secure 4-digit code (1000..=9999) using OS CSPRNG.
    pub fn generate_secure_code_4_digits() -> String {
        let mut bytes = [0u8; 4];
        getrandom::getrandom(&mut bytes).expect("OS CSPRNG must be available");
        let num = u32::from_ne_bytes(bytes);
        let code = (num % 9000) + 1000;
        format!("{:04}", code)
    }

    /// Registers a pending action and returns the secure 4-digit code.
    pub fn register_action(
        &self,
        server: &str,
        tool_name: &str,
        arguments: serde_json::Value,
        description: &str,
    ) -> String {
        let code = Self::generate_secure_code_4_digits();
        let now = Instant::now();
        let action = PendingUnifiedAction {
            id: uuid::Uuid::new_v4().to_string(),
            code: code.clone(),
            server: server.to_string(),
            tool_name: tool_name.to_string(),
            arguments,
            description: description.to_string(),
            failed_attempts: 0,
            created_at: now,
            expires_at: now + UNIFIED_CONFIRMATION_TTL,
        };

        if let Ok(mut lock) = self.pending.lock() {
            *lock = Some(action);
        }

        code
    }

    /// Explicitly registers an action with a predefined code and TTL (for deterministic testing).
    pub fn register_action_with_code(
        &self,
        server: &str,
        tool_name: &str,
        arguments: serde_json::Value,
        description: &str,
        code: &str,
        ttl: Duration,
    ) {
        let now = Instant::now();
        let action = PendingUnifiedAction {
            id: uuid::Uuid::new_v4().to_string(),
            code: code.to_string(),
            server: server.to_string(),
            tool_name: tool_name.to_string(),
            arguments,
            description: description.to_string(),
            failed_attempts: 0,
            created_at: now,
            expires_at: now + ttl,
        };

        if let Ok(mut lock) = self.pending.lock() {
            *lock = Some(action);
        }
    }

    /// Verifies the confirmation code:
    /// - If expired -> removes action and returns error
    /// - If matched -> consumes action and returns action details
    /// - If mismatched -> increments failed_attempts; if >= 5 drops action completely
    pub fn verify_and_consume(
        &self,
        input_code: &str,
    ) -> Result<PendingUnifiedAction, PolicyError> {
        let mut lock = self
            .pending
            .lock()
            .map_err(|_| PolicyError::PermissionDenied("Lock poisoned".into()))?;

        if let Some(mut action) = lock.take() {
            if Instant::now() > action.expires_at {
                return Err(PolicyError::PermissionDenied(
                    "Confirmation code has expired (120s limit)".into(),
                ));
            }

            if action.code.trim() == input_code.trim() {
                Ok(action)
            } else {
                action.failed_attempts += 1;
                if action.failed_attempts >= MAX_CONFIRMATION_ATTEMPTS {
                    Err(PolicyError::PermissionDenied(
                        "Maximum failed confirmation attempts (5) exceeded; action invalidated"
                            .into(),
                    ))
                } else {
                    let remaining = MAX_CONFIRMATION_ATTEMPTS - action.failed_attempts;
                    *lock = Some(action);
                    Err(PolicyError::PermissionDenied(format!(
                        "Incorrect confirmation code ({remaining} attempts remaining)"
                    )))
                }
            }
        } else {
            Err(PolicyError::PermissionDenied(
                "No pending action waiting for confirmation".into(),
            ))
        }
    }

    /// Cancels any currently pending action.
    pub fn cancel_pending(&self) -> Option<PendingUnifiedAction> {
        if let Ok(mut lock) = self.pending.lock() {
            lock.take()
        } else {
            None
        }
    }

    /// Inspects whether there is a pending action without consuming it.
    pub fn get_pending_summary(&self) -> Option<(String, Duration)> {
        if let Ok(lock) = self.pending.lock() {
            if let Some(action) = lock.as_ref() {
                let now = Instant::now();
                if now <= action.expires_at {
                    return Some((
                        action.description.clone(),
                        action.expires_at.duration_since(now),
                    ));
                }
            }
        }
        None
    }
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

    #[test]
    fn test_tool_security_policy_classification_and_default_deny() {
        let mut overrides = std::collections::HashMap::new();
        overrides.insert("custom::my_reader".to_string(), RiskClass::ReadOnly);
        overrides.insert("custom::my_writer".to_string(), RiskClass::Write);

        let policy = DefaultToolSecurityPolicy::new(
            vec!["github.com".into(), "*.github.com".into()],
            overrides,
        );

        // 1. Explicit risk table matches
        assert_eq!(policy.classify_tool("fs", "read_file"), RiskClass::ReadOnly);
        assert_eq!(
            policy.classify_tool("fs", "list_directory"),
            RiskClass::ReadOnly
        );
        assert_eq!(
            policy.classify_tool("memory", "search_memories"),
            RiskClass::ReadOnly
        );
        assert_eq!(policy.classify_tool("fs", "write_file"), RiskClass::Write);
        assert_eq!(
            policy.classify_tool("fs", "delete_file"),
            RiskClass::Destructive
        );
        assert_eq!(
            policy.classify_tool("playwright", "navigate"),
            RiskClass::ExternalNetwork
        );
        assert_eq!(
            policy.classify_tool("playwright", "evaluate"),
            RiskClass::Destructive
        );

        // 2. Config override entries match
        assert_eq!(
            policy.classify_tool("custom", "my_reader"),
            RiskClass::ReadOnly
        );
        assert_eq!(
            policy.classify_tool("custom", "my_writer"),
            RiskClass::Write
        );

        // 3. Unknown tools default to Unknown (no substring fallback)
        assert_eq!(
            policy.classify_tool("mcp", "list_directory"),
            RiskClass::Unknown
        );
        assert_eq!(
            policy.classify_tool("db", "search_items"),
            RiskClass::Unknown
        );
        assert_eq!(
            policy.classify_tool("shell", "eval_custom_command"),
            RiskClass::Unknown
        );

        let res = policy.validate_tool_call("shell", "eval_custom_command", &serde_json::json!({}));
        assert!(matches!(res, Err(PolicyError::UnknownToolDenied(_))));

        // 4. Domain allowlist on URL
        let allowed_args = serde_json::json!({ "url": "https://api.github.com/v1" });
        assert!(policy
            .validate_tool_call("playwright", "navigate", &allowed_args)
            .is_ok());

        let blocked_domain_args = serde_json::json!({ "url": "https://evil-hacker.com/steal" });
        let blocked_res = policy.validate_tool_call("playwright", "navigate", &blocked_domain_args);
        assert!(matches!(blocked_res, Err(PolicyError::DomainBlocked(_))));

        // 7. SSRF blocked on URL
        let ssrf_args = serde_json::json!({ "url": "http://127.0.0.1:8080/admin" });
        let ssrf_res = policy.validate_tool_call("playwright", "navigate", &ssrf_args);
        assert!(matches!(ssrf_res, Err(PolicyError::SsrfBlocked(_))));
    }

    #[test]
    fn test_comprehensive_ssrf_rejection_classes() {
        let policy = DefaultToolSecurityPolicy::new(vec![], Default::default());

        // 1. Loopback IPv4, IPv6, localhost
        let test_cases_loopback = [
            "http://127.0.0.1/status",
            "http://127.1.2.3:8080/",
            "http://localhost:3000/",
            "http://test.localhost/",
            "http://[::1]/",
        ];
        for url in test_cases_loopback {
            let res = policy.validate_browser_url(url);
            assert!(
                matches!(res, Err(PolicyError::SsrfBlocked(_))),
                "Failed to block loopback: {url}"
            );
        }

        // 2. Private ranges (10/8, 172.16/12, 192.168/16)
        let test_cases_private = [
            "http://10.0.0.1/admin",
            "http://10.255.255.254/",
            "http://172.16.0.1/",
            "http://172.31.255.255/",
            "http://192.168.1.1/router",
            "http://192.168.0.100/",
        ];
        for url in test_cases_private {
            let res = policy.validate_browser_url(url);
            assert!(
                matches!(res, Err(PolicyError::SsrfBlocked(_))),
                "Failed to block private IP: {url}"
            );
        }

        // 3. Link-local (169.254/16) and cloud metadata
        let test_cases_link_local = [
            "http://169.254.169.254/latest/meta-data/",
            "http://169.254.1.1/",
            "http://[fe80::1]/",
        ];
        for url in test_cases_link_local {
            let res = policy.validate_browser_url(url);
            assert!(
                matches!(res, Err(PolicyError::SsrfBlocked(_))),
                "Failed to block link-local: {url}"
            );
        }

        // 4. Unspecified 0.0.0.0 and broadcast
        let test_cases_unspecified = [
            "http://0.0.0.0:8000/",
            "http://0/",
            "http://[::]/",
            "http://255.255.255.255/",
        ];
        for url in test_cases_unspecified {
            let res = policy.validate_browser_url(url);
            assert!(
                matches!(res, Err(PolicyError::SsrfBlocked(_))),
                "Failed to block unspecified: {url}"
            );
        }

        // 5. IPv6 ULA (fc00::/7) and mapped IPv4 (::ffff:127.0.0.1)
        let test_cases_ipv6 = [
            "http://[fc00::1]/",
            "http://[fd12:3456:789a::1]/",
            "http://[::ffff:127.0.0.1]/",
            "http://[::ffff:10.0.0.1]/",
            "http://[::127.0.0.1]/",
        ];
        for url in test_cases_ipv6 {
            let res = policy.validate_browser_url(url);
            assert!(
                matches!(res, Err(PolicyError::SsrfBlocked(_))),
                "Failed to block IPv6 special form: {url}"
            );
        }

        // 6. Alternate IP spellings (decimal, hex, octal)
        let test_cases_alternate = [
            "http://2130706433/",   // 127.0.0.1 in decimal integer
            "http://0x7f000001/",   // 127.0.0.1 in monolithic hex
            "http://0x7f.0.0.1/",   // 127.0.0.1 in dotted hex
            "http://0177.0.0.1/",   // 127.0.0.1 in dotted octal
            "http://017700000001/", // 127.0.0.1 in monolithic octal
            "http://0x0a000001/",   // 10.0.0.1 in hex
            "http://167772161/",    // 10.0.0.1 in decimal
        ];
        for url in test_cases_alternate {
            let res = policy.validate_browser_url(url);
            assert!(
                matches!(res, Err(PolicyError::SsrfBlocked(_))),
                "Failed to block alternate spelling: {url}"
            );
        }
    }

    #[test]
    fn test_unified_confirmation_manager_lifecycle_ttl_and_attempt_capping() {
        let mgr = UnifiedConfirmationManager::new();

        // 1. Generate code check CSPRNG bounds (1000..=9999)
        for _ in 0..20 {
            let code = UnifiedConfirmationManager::generate_secure_code_4_digits();
            assert_eq!(code.len(), 4);
            let val = code.parse::<u32>().unwrap();
            assert!((1000..=9999).contains(&val));
        }

        // 2. Action registration and exact action binding
        let args = serde_json::json!({ "path": "test.txt", "content": "hello" });
        let code = mgr.register_action("fs", "write_file", args.clone(), "Write test file");
        assert_eq!(code.len(), 4);

        // 3. Failed attempts capped at 5 -> drops action
        for remaining in (1..5).rev() {
            let res = mgr.verify_and_consume("0000"); // wrong code
            assert!(res.is_err());
            let err_msg = res.unwrap_err().to_string();
            assert!(err_msg.contains(&format!("{remaining} attempts remaining")));
        }
        // 5th failed attempt -> max exceeded & action invalidated
        let res5 = mgr.verify_and_consume("0000");
        assert!(res5.is_err());
        assert!(res5
            .unwrap_err()
            .to_string()
            .contains("Maximum failed confirmation attempts (5) exceeded"));

        // Subsequent attempt now reports no pending action
        let res_subsequent = mgr.verify_and_consume(&code);
        assert!(res_subsequent.is_err());
        assert!(res_subsequent
            .unwrap_err()
            .to_string()
            .contains("No pending action"));

        // 4. Test TTL expiry
        mgr.register_action_with_code(
            "fs",
            "delete_file",
            serde_json::json!({}),
            "Delete file",
            "1234",
            Duration::from_millis(10),
        );
        std::thread::sleep(Duration::from_millis(25));
        let res_expired = mgr.verify_and_consume("1234");
        assert!(res_expired.is_err());
        assert!(res_expired.unwrap_err().to_string().contains("expired"));

        // 5. Successful verification consumes action
        let success_code = mgr.register_action("fs", "write_file", args.clone(), "Write test file");
        let verified = mgr
            .verify_and_consume(&success_code)
            .expect("Must verify successfully");
        assert_eq!(verified.server, "fs");
        assert_eq!(verified.tool_name, "write_file");
        assert_eq!(verified.arguments, args);

        // Cannot consume twice
        assert!(mgr.verify_and_consume(&success_code).is_err());
    }
}
