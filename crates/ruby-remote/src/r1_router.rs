//! Milestone R1 Foundation: Policy-gated command dispatching & escalation.

use async_trait::async_trait;
use std::sync::Arc;

use ruby_actions::{ActionExecutor, LocalAction};
use ruby_policy::{PolicyError, SecurityPolicy};

use crate::confirmation::ConfirmationManager;
use crate::traits::RemoteCommandHandler;
use crate::types::{IncomingMessage, RemoteError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allowed(LocalAction),
    RequiresConfirmation {
        action: LocalAction,
        description: String,
    },
    Denied(String),
}

pub struct PolicyGatedCommandHandler {
    policy: Arc<dyn SecurityPolicy>,
    executor: Arc<dyn ActionExecutor>,
    confirmation_mgr: ConfirmationManager,
}

impl PolicyGatedCommandHandler {
    pub fn new(
        policy: Arc<dyn SecurityPolicy>,
        executor: Arc<dyn ActionExecutor>,
        confirmation_mgr: ConfirmationManager,
    ) -> Self {
        Self {
            policy,
            executor,
            confirmation_mgr,
        }
    }

    /// Evaluates an instruction against the security policy.
    pub fn evaluate_instruction(&self, instruction: &str) -> PolicyDecision {
        let trimmed = instruction.trim();
        if let Some(path_str) = trimmed.strip_prefix("open ") {
            let path = std::path::Path::new(path_str.trim());
            match self.policy.validate_read_path(path) {
                Ok(valid_path) => PolicyDecision::Allowed(LocalAction::OpenFile(
                    valid_path.to_string_lossy().to_string(),
                )),
                Err(PolicyError::SensitiveDirectoryBlocked(_)) => {
                    PolicyDecision::Denied("Access to sensitive directory blocked".to_string())
                }
                Err(PolicyError::PathDisallowed(_)) => {
                    PolicyDecision::Denied("Path outside allowed roots".to_string())
                }
                Err(e) => PolicyDecision::Denied(e.to_string()),
            }
        } else if let Some(setting) = trimmed.strip_prefix("settings ") {
            PolicyDecision::Allowed(LocalAction::OpenSystemSettings(setting.trim().to_string()))
        } else if let Some(app) = trimmed.strip_prefix("launch ") {
            let app_name = app.trim().to_string();
            PolicyDecision::RequiresConfirmation {
                action: LocalAction::LaunchApp(app_name.clone()),
                description: format!("Launch application '{app_name}'"),
            }
        } else {
            PolicyDecision::RequiresConfirmation {
                action: LocalAction::LaunchApp(trimmed.to_string()),
                description: format!("Execute action: {trimmed}"),
            }
        }
    }
}

#[async_trait]
impl RemoteCommandHandler for PolicyGatedCommandHandler {
    async fn handle_command(&self, message: IncomingMessage) -> Result<String, RemoteError> {
        match self.evaluate_instruction(&message.text) {
            PolicyDecision::Allowed(action) => {
                let res = self
                    .executor
                    .execute(action)
                    .await
                    .map_err(|e| RemoteError::Api(e.to_string()))?;
                Ok(res)
            }
            PolicyDecision::RequiresConfirmation {
                action: _action,
                description,
            } => {
                let code = self
                    .confirmation_mgr
                    .register_action(&format!("act_{}", message.update_id), &description);
                Ok(format!(
                    "Tool execution requires confirmation: {description}.\nReply /yes {code} or /no within 120 seconds."
                ))
            }
            PolicyDecision::Denied(reason) => {
                Ok(format!("❌ Action denied by security policy: {reason}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruby_actions::AllowlistedActionExecutor;
    use ruby_policy::DefaultSecurityPolicy;
    use std::path::PathBuf;

    #[test]
    fn test_policy_gated_allowed_settings() {
        let policy = Arc::new(DefaultSecurityPolicy::new(vec![PathBuf::from(
            r"D:\projects",
        )]));
        let executor = Arc::new(AllowlistedActionExecutor);
        let confirmation_mgr = ConfirmationManager::new();
        let handler = PolicyGatedCommandHandler::new(policy, executor, confirmation_mgr);

        let decision = handler.evaluate_instruction("settings display");
        assert_eq!(
            decision,
            PolicyDecision::Allowed(LocalAction::OpenSystemSettings("display".to_string()))
        );
    }

    #[test]
    fn test_policy_gated_denies_sensitive_directory() {
        let policy = Arc::new(DefaultSecurityPolicy::new(vec![PathBuf::from(r"C:\")]));
        let executor = Arc::new(AllowlistedActionExecutor);
        let confirmation_mgr = ConfirmationManager::new();
        let handler = PolicyGatedCommandHandler::new(policy, executor, confirmation_mgr);

        let decision = handler.evaluate_instruction(r"open C:\Windows\System32\cmd.exe");
        match decision {
            PolicyDecision::Denied(msg) => {
                assert!(msg.contains("Access to sensitive directory blocked"));
            }
            other => panic!("Expected Denied, got {:?}", other),
        }
    }

    #[test]
    fn test_policy_gated_escalates_to_confirmation() {
        let policy = Arc::new(DefaultSecurityPolicy::new(vec![PathBuf::from(
            r"D:\projects",
        )]));
        let executor = Arc::new(AllowlistedActionExecutor);
        let confirmation_mgr = ConfirmationManager::new();
        let handler = PolicyGatedCommandHandler::new(policy, executor, confirmation_mgr);

        let decision = handler.evaluate_instruction("launch notepad.exe");
        match decision {
            PolicyDecision::RequiresConfirmation { description, .. } => {
                assert!(description.contains("notepad.exe"));
            }
            other => panic!("Expected RequiresConfirmation, got {:?}", other),
        }
    }
}
