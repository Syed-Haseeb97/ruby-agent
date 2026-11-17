//! Ephemeral 5-minute pairing code manager.
//! The pairing code is displayed ONLY in the local Console UI and NEVER written to logs.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const PAIRING_CODE_TTL: Duration = Duration::from_secs(300); // 5 minutes

#[derive(Clone, Debug)]
pub struct ActivePairingCode {
    pub code: String,
    pub created_at: Instant,
    pub expires_at: Instant,
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

        // Generate 6-digit random code
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        let code = format!("{:06}", (nanos % 900000) + 100000);

        let pairing = ActivePairingCode {
            code: code.clone(),
            created_at: now,
            expires_at: now + PAIRING_CODE_TTL,
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
        });
    }

    /// Attempts to validate a pairing code. Returns true if valid and not expired.
    pub fn validate_code(&self, candidate: &str) -> bool {
        let mut lock = match self.active_code.lock() {
            Ok(l) => l,
            Err(p) => p.into_inner(),
        };

        if let Some(active) = lock.take() {
            let now = Instant::now();
            if now <= active.expires_at && active.code.trim().eq_ignore_ascii_case(candidate.trim())
            {
                return true;
            }
            // Put it back if candidate did not match and hasn't expired yet
            if now <= active.expires_at {
                *lock = Some(active);
            }
        }
        false
    }

    /// Clears any active pairing code (e.g. after pairing succeeds or is disconnected).
    pub fn clear(&self) {
        if let Ok(mut lock) = self.active_code.lock() {
            *lock = None;
        }
    }
}
