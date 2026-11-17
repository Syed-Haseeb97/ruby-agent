//! Ephemeral 5-minute pairing code manager.
//! The pairing code is displayed ONLY in the local Console UI and NEVER written to logs.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::warn;

pub const PAIRING_CODE_TTL: Duration = Duration::from_secs(300); // 5 minutes
pub const MAX_FAILED_PAIRING_ATTEMPTS: u32 = 5;

#[derive(Clone, Debug)]
pub struct ActivePairingCode {
    pub code: String,
    pub created_at: Instant,
    pub expires_at: Instant,
    pub failed_attempts: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingResult {
    Success,
    InvalidOrExpired,
    MaxAttemptsExceeded,
}

#[derive(Clone)]
pub struct PairingManager {
    active_code: Arc<Mutex<Option<ActivePairingCode>>>,
}

impl Default for PairingManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PairingManager {
    pub fn new() -> Self {
        Self {
            active_code: Arc::new(Mutex::new(None)),
        }
    }

    /// Generates a cryptographically secure 6-digit random code using OS CSPRNG.
    pub fn generate_secure_code_6_digits() -> String {
        let mut bytes = [0u8; 4];
        getrandom::getrandom(&mut bytes).expect("OS CSPRNG must be available");
        let num = u32::from_ne_bytes(bytes);
        let code = (num % 900_000) + 100_000;
        format!("{:06}", code)
    }

    /// Generates or returns an active 6-character pairing code.
    pub fn get_or_generate_code(&self) -> (String, u64) {
        let mut lock = match self.active_code.lock() {
            Ok(l) => l,
            Err(p) => p.into_inner(),
        };

        let now = Instant::now();
        if let Some(ref current) = *lock {
            if now < current.expires_at {
                let remaining = current.expires_at.duration_since(now).as_secs();
                return (current.code.clone(), remaining);
            }
        }

        // Generate 6-digit random code using CSPRNG
        let code = Self::generate_secure_code_6_digits();

        let pairing = ActivePairingCode {
            code: code.clone(),
            created_at: now,
            expires_at: now + PAIRING_CODE_TTL,
            failed_attempts: 0,
        };

        *lock = Some(pairing);
        (code, PAIRING_CODE_TTL.as_secs())
    }

    /// Explicitly sets a pairing code with custom TTL (useful for deterministic tests).
    pub fn set_code_for_test(&self, code: &str, ttl: Duration) {
        let mut lock = match self.active_code.lock() {
            Ok(l) => l,
            Err(p) => p.into_inner(),
        };
        let now = Instant::now();
        *lock = Some(ActivePairingCode {
            code: code.to_string(),
            created_at: now,
            expires_at: now + ttl,
            failed_attempts: 0,
        });
    }

    /// Attempts to validate a pairing code. Caps failed attempts at 5 before invalidating.
    pub fn validate_code(&self, candidate: &str) -> PairingResult {
        let mut lock = match self.active_code.lock() {
            Ok(l) => l,
            Err(p) => p.into_inner(),
        };

        if let Some(mut active) = lock.take() {
            let now = Instant::now();
            if now <= active.expires_at {
                if active.code.trim().eq_ignore_ascii_case(candidate.trim()) {
                    return PairingResult::Success;
                }

                // Increment failed attempt counter
                active.failed_attempts += 1;
                if active.failed_attempts >= MAX_FAILED_PAIRING_ATTEMPTS {
                    warn!(
                        "Pairing code invalidated after {} consecutive failed attempts",
                        MAX_FAILED_PAIRING_ATTEMPTS
                    );
                    // Code remains invalidated (*lock = None)
                    return PairingResult::MaxAttemptsExceeded;
                }

                warn!(
                    "Failed pairing attempt ({}/{})",
                    active.failed_attempts, MAX_FAILED_PAIRING_ATTEMPTS
                );
                // Put back active code with incremented counter
                *lock = Some(active);
                return PairingResult::InvalidOrExpired;
            }
        }
        PairingResult::InvalidOrExpired
    }

    /// Clears any active pairing code (e.g. after pairing succeeds or is disconnected).
    pub fn clear(&self) {
        if let Ok(mut lock) = self.active_code.lock() {
            *lock = None;
        }
    }
}
