//! Random 4-digit confirmation plumbing for dangerous / escalated actions with 120s TTL
//! backed by `ruby_policy::UnifiedConfirmationManager`.

use crate::types::RemoteError;
use ruby_policy::UnifiedConfirmationManager;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const CONFIRMATION_TTL: Duration = ruby_policy::UNIFIED_CONFIRMATION_TTL;

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
    inner: Arc<UnifiedConfirmationManager>,
}

impl Default for ConfirmationManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfirmationManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(UnifiedConfirmationManager::new()),
        }
    }

    pub fn from_unified(inner: Arc<UnifiedConfirmationManager>) -> Self {
        Self { inner }
    }

    pub fn unified(&self) -> Arc<UnifiedConfirmationManager> {
        self.inner.clone()
    }

    /// Generates a cryptographically secure 6-digit code using OS CSPRNG.
    pub fn generate_secure_code_6_digits() -> String {
        UnifiedConfirmationManager::generate_secure_code_6_digits()
    }

    /// Alias for backwards compatibility, returning 6 digits everywhere per Z2.
    pub fn generate_secure_code_4_digits() -> String {
        UnifiedConfirmationManager::generate_secure_code_6_digits()
    }

    /// Registers a pending action and returns the random 4-digit code.
    pub fn register_action(&self, action_id: &str, description: &str) -> String {
        self.inner
            .register_action("remote", action_id, serde_json::json!({}), description)
    }

    /// Explicitly registers with a known code (useful for deterministic testing).
    pub fn register_action_with_code(
        &self,
        action_id: &str,
        description: &str,
        code: &str,
        ttl: Duration,
    ) {
        self.inner.register_action_with_code(
            "remote",
            action_id,
            serde_json::json!({}),
            description,
            code,
            ttl,
        );
    }

    /// Verifies the provided 4-digit code. If matched and not expired, clears pending and returns description.
    pub fn verify_code(&self, input_code: &str) -> Result<PendingAction, RemoteError> {
        match self.inner.verify_and_consume(input_code) {
            Ok(action) => Ok(PendingAction {
                action_id: action.tool_name,
                code: action.code,
                description: action.description,
                created_at: action.created_at,
                expires_at: action.expires_at,
            }),
            Err(e) => Err(RemoteError::InvalidCode(e.to_string())),
        }
    }

    /// Cancels any currently pending action (e.g. on /no or /stop).
    pub fn cancel_pending(&self) -> Option<PendingAction> {
        self.inner.cancel_pending().map(|a| PendingAction {
            action_id: a.tool_name,
            code: a.code,
            description: a.description,
            created_at: a.created_at,
            expires_at: a.expires_at,
        })
    }

    /// Returns the active pending action if not expired.
    pub fn get_active_pending(&self) -> Option<PendingAction> {
        self.inner
            .get_pending_action()
            .map(|a| PendingAction {
                action_id: a.id,
                code: a.code,
                description: a.description,
                created_at: a.created_at,
                expires_at: a.expires_at,
            })
    }
}
