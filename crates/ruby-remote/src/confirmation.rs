//! Random 4-digit confirmation plumbing for dangerous / escalated actions with 120s TTL.

use crate::types::RemoteError;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const CONFIRMATION_TTL: Duration = Duration::from_secs(120);

#[derive(Clone, Debug)]
pub struct PendingAction {
    pub action_id: String,
    pub code: String,
    pub description: String,
    pub created_at: Instant,
    pub expires_at: Instant,
}

#[derive(Clone)]
pub struct ConfirmationManager {
    pending: Arc<Mutex<Option<PendingAction>>>,
}

impl Default for ConfirmationManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfirmationManager {
    pub fn new() -> Self {
        Self {
            pending: Arc::new(Mutex::new(None)),
        }
    }

    /// Registers a pending action and returns the random 4-digit code.
    pub fn register_action(&self, action_id: &str, description: &str) -> String {
        // Generate random 4-digit code [1000..9999]
        let code = format!(
            "{:04}",
            (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos()
                % 9000)
                + 1000
        );

        let now = Instant::now();
        let action = PendingAction {
            action_id: action_id.to_string(),
            code: code.clone(),
            description: description.to_string(),
            created_at: now,
            expires_at: now + CONFIRMATION_TTL,
        };

        if let Ok(mut lock) = self.pending.lock() {
            *lock = Some(action);
        }

        code
    }

    /// Explicitly registers with a known code (useful for deterministic testing).
    pub fn register_action_with_code(
        &self,
        action_id: &str,
        description: &str,
        code: &str,
        ttl: Duration,
    ) {
        let now = Instant::now();
        let action = PendingAction {
            action_id: action_id.to_string(),
            code: code.to_string(),
            description: description.to_string(),
            created_at: now,
            expires_at: now + ttl,
        };
        if let Ok(mut lock) = self.pending.lock() {
            *lock = Some(action);
        }
    }

    /// Verifies the provided 4-digit code. If matched and not expired, clears pending and returns description.
    pub fn verify_code(&self, input_code: &str) -> Result<PendingAction, RemoteError> {
        let mut lock = self
            .pending
            .lock()
            .map_err(|_| RemoteError::Auth("Lock poisoned".to_string()))?;
        if let Some(action) = lock.take() {
            if Instant::now() > action.expires_at {
                return Err(RemoteError::InvalidCode(
                    "Confirmation code has expired (120s limit)".to_string(),
                ));
            }
            if action.code.trim() == input_code.trim() {
                Ok(action)
            } else {
                // Restore if mismatched so user can retry within TTL
                *lock = Some(action);
                Err(RemoteError::InvalidCode(
                    "Incorrect confirmation code".to_string(),
                ))
            }
        } else {
            Err(RemoteError::InvalidCode(
                "No pending action waiting for confirmation".to_string(),
            ))
        }
    }

    /// Cancels any currently pending action (e.g. on /no or /stop).
    pub fn cancel_pending(&self) -> Option<PendingAction> {
        if let Ok(mut lock) = self.pending.lock() {
            lock.take()
        } else {
            None
        }
    }

    /// Returns the active pending action if not expired.
    pub fn get_active_pending(&self) -> Option<PendingAction> {
        if let Ok(lock) = self.pending.lock() {
            if let Some(ref act) = *lock {
                if Instant::now() <= act.expires_at {
                    return Some(act.clone());
                }
            }
        }
        None
    }
}
