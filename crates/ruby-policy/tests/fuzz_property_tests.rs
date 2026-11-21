use ruby_policy::{
    parse_ip_or_numeric_host, DefaultSecurityPolicy, DefaultToolSecurityPolicy, SecurityPolicy,
    ToolSecurityPolicy,
};
use std::path::{Path, PathBuf};
use tempfile::tempdir;

/// Simple deterministic pseudo-random number generator for reproducible fuzz runs.
struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn next_u32(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.state >> 32) as u32
    }

    fn next_bounded(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            0
        } else {
            self.next_u32() % bound
        }
    }

    #[allow(dead_code)]
    fn next_string(&mut self, len: usize, alphabet: &[char]) -> String {
        let mut s = String::with_capacity(len);
        for _ in 0..len {
            let idx = self.next_bounded(alphabet.len() as u32) as usize;
            s.push(alphabet[idx]);
        }
        s
    }
}

#[test]
fn test_fuzz_path_canonicalization_and_traversal_never_escapes_or_panics() {
    let tmp = tempdir().expect("tempdir");
    let root_path = tmp.path().to_path_buf();
    let safe_subdir = root_path.join("safe_dir");
    std::fs::create_dir_all(&safe_subdir).unwrap();

    let fs_policy = DefaultSecurityPolicy::new(vec![root_path.clone()]);

    let traversal_components = [
        "..",
        "../",
        "..\\",
        "....//",
        "...\\",
        "....",
        "\u{FF0E}\u{FF0E}", // fullwidth two dots
        "\u{3002}\u{3002}", // ideographic full stop
        "\u{FE52}\u{FE52}", // small full stop
        "CON",
        "PRN",
        "AUX",
        "NUL",
        "COM1",
        "LPT1",
        "foo:bar",
        "foo::$DATA",
        "\\\\evil-server\\share",
        "//evil-server/share",
        "\\\\?\\C:\\Windows",
        "\\\\.\\PhysicalDrive0",
        "C:\\Windows\\System32",
        "D:\\Windows",
        "/etc/passwd",
        "/var/run",
        "\0secret",
        "normal_file.txt",
        "sub/nested.txt",
    ];

    let mut rng = SimpleRng::new(0xDEAD_BEEF_CAFE_BABE);

    // 1. Structured adversarial combinations
    for comp in &traversal_components {
        let test_path = PathBuf::from(comp);
        let res_read = fs_policy.validate_read_path(&test_path);
        let res_write = fs_policy.validate_write_path(&test_path);

        // If it succeeded, it MUST be inside root_path!
        if let Ok(canon) = res_read {
            assert!(
                canon.starts_with(&root_path),
                "Escaped root via validate_read_path! Input: {:?}, Resolved: {:?}",
                comp,
                canon
            );
        }
        if let Ok(canon) = res_write {
            assert!(
                canon.starts_with(&root_path),
                "Escaped root via validate_write_path! Input: {:?}, Resolved: {:?}",
                comp,
                canon
            );
        }
    }

    // 2. Randomized generative fuzzing: 5,000 iterations of chained path tokens
    for _ in 0..5000 {
        let num_parts = rng.next_bounded(6) as usize + 1;
        let mut built = root_path.clone();
        let mut raw_str = String::new();

        for i in 0..num_parts {
            let part =
                traversal_components[rng.next_bounded(traversal_components.len() as u32) as usize];
            if i > 0 {
                if rng.next_bounded(2) == 0 {
                    raw_str.push('/');
                } else {
                    raw_str.push('\\');
                }
            }
            raw_str.push_str(part);
            built = built.join(part);
        }

        // Test with PathBuf built from joined tokens
        let _ = fs_policy.validate_read_path(&built);
        let _ = fs_policy.validate_write_path(&built);

        // Test with raw string path
        let raw_p = Path::new(&raw_str);
        if let Ok(canon) = fs_policy.validate_read_path(raw_p) {
            assert!(
                canon.starts_with(&root_path),
                "Fuzzed raw path escaped root on read! Input: {:?}, Canon: {:?}",
                raw_str,
                canon
            );
        }
        if let Ok(canon) = fs_policy.validate_write_path(raw_p) {
            assert!(
                canon.starts_with(&root_path),
                "Fuzzed raw path escaped root on write! Input: {:?}, Canon: {:?}",
                raw_str,
                canon
            );
        }
    }
}

#[test]
fn test_fuzz_url_validation_and_ssrf_rejections() {
    let policy = DefaultToolSecurityPolicy::new(vec![], std::collections::HashMap::new());

    // 1. Exact SSRF evasion edge cases
    let ssrf_vectors = [
        // IPv4 loopback variants
        "http://127.0.0.1",
        "http://127.0.0.2:8080",
        "http://127.1",
        "http://0177.0.0.1",
        "http://017700000001",
        "http://0x7f.0.0.1",
        "http://0x7f000001",
        "http://2130706433",
        // Local link and metadata
        "http://169.254.169.254/latest/meta-data/",
        "http://169.254.1.1",
        "http://0.0.0.0",
        "http://0.0.0.0:80",
        // IPv6 loopback and mapped
        "http://[::1]",
        "http://[::1]:8080",
        "http://[0:0:0:0:0:0:0:1]",
        "http://[::ffff:127.0.0.1]",
        "http://[::ffff:7f00:1]",
        "http://[0:0:0:0:0:ffff:127.0.0.1]",
        "http://[fe80::1]",
        "http://[fc00::1]",
        "http://[fd00::1]",
        // Cloud internal hostnames
        "http://metadata.google.internal",
        "http://metadata.google.internal/computeMetadata/v1/",
        "http://localhost",
        "http://localhost:3000",
        "http://sub.localhost",
        // Dangerous schemes
        "file:///C:/Windows/System32/cmd.exe",
        "gopher://127.0.0.1:70",
        "ftp://example.com/file",
        "data:text/html,<script>alert(1)</script>",
        "javascript:alert(1)",
    ];

    for &vec in &ssrf_vectors {
        let res = policy.validate_browser_url(vec);
        assert!(
            res.is_err(),
            "Vector should have been rejected as SSRF/dangerous! Vector: {}",
            vec
        );
    }

    // 2. Numeric and alternate IP parsing tests
    assert!(parse_ip_or_numeric_host("127.0.0.1").is_some());
    assert!(parse_ip_or_numeric_host("0x7f000001").is_some());
    assert!(parse_ip_or_numeric_host("2130706433").is_some());
    assert!(parse_ip_or_numeric_host("017700000001").is_some());
    assert!(parse_ip_or_numeric_host("::1").is_some());
    assert!(parse_ip_or_numeric_host("[::1]").is_some());

    // 3. Generative fuzzing: 5,000 randomized URLs
    let mut rng = SimpleRng::new(0x1234_5678_9ABC_DEF0);
    let schemes = ["http", "https", "ftp", "file", "gopher", "ws", "wss"];
    let hosts = [
        "example.com",
        "127.0.0.1",
        "0.0.0.0",
        "10.0.0.1",
        "192.168.1.1",
        "169.254.169.254",
        "localhost",
        "[::1]",
        "metadata.google.internal",
        "93.184.216.34", // example.com IP
        "8.8.8.8",
        "xn--e1afmkfd.xn--p1ai",
    ];

    for _ in 0..5000 {
        let scheme = schemes[rng.next_bounded(schemes.len() as u32) as usize];
        let host = hosts[rng.next_bounded(hosts.len() as u32) as usize];
        let port_choice = rng.next_bounded(3);
        let port_str = match port_choice {
            0 => "".to_string(),
            1 => format!(":{}", rng.next_bounded(65535) + 1),
            _ => ":80".to_string(),
        };

        let raw_url = format!(
            "{}://{}{}/test?query={}",
            scheme,
            host,
            port_str,
            rng.next_u32()
        );
        let res = policy.validate_browser_url(&raw_url);

        // Disallowed schemes must ALWAYS fail
        if scheme != "http" && scheme != "https" {
            assert!(res.is_err(), "Non-http(s) scheme accepted: {}", raw_url);
        }

        // Loopback / private / link-local / metadata hosts must ALWAYS fail
        if host == "127.0.0.1"
            || host == "0.0.0.0"
            || host == "10.0.0.1"
            || host == "192.168.1.1"
            || host == "169.254.169.254"
            || host == "localhost"
            || host == "[::1]"
            || host == "metadata.google.internal"
        {
            assert!(res.is_err(), "Blocked host accepted: {}", raw_url);
        }
    }
}
