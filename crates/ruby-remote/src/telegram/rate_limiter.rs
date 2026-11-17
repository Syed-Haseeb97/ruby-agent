//! Rate limiter for unknown/unauthorized senders.
//! Ensures at most one warning per unknown sender per 60 seconds without logging message content.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct SenderRateLimiter {
    last_logged: Arc<Mutex<HashMap<i64, Instant>>>,
    cooldown: Duration,
}

impl Default for SenderRateLimiter {
    fn default() -> Self {
        Self::new(Duration::from_secs(60))
    }
}

impl SenderRateLimiter {
    pub fn new(cooldown: Duration) -> Self {
        Self {
            last_logged: Arc::new(Mutex::new(HashMap::new())),
            cooldown,
        }
    }

    /// Checks if a log warning should be emitted for an unknown sender.
    /// Returns true if cooldown has elapsed (and records new timestamp), false otherwise.
    pub fn should_log(&self, sender_id: i64) -> bool {
        let now = Instant::now();
        if let Ok(mut lock) = self.last_logged.lock() {
            if let Some(last) = lock.get(&sender_id) {
                if now.duration_since(*last) < self.cooldown {
                    return false;
                }
            }
            lock.insert(sender_id, now);
            true
        } else {
            false
        }
    }
}
