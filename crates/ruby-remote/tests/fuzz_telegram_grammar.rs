use ruby_actions::LocalAction;
use ruby_policy::DefaultSecurityPolicy;
use ruby_remote::confirmation::ConfirmationManager;
use ruby_remote::r1_router::{PolicyDecision, PolicyGatedCommandHandler};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::tempdir;

struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.state >> 32) as u32
    }

    fn next_bounded(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            0
        } else {
            self.next_u32() % bound
        }
    }
}

struct DummyExecutor;
#[async_trait::async_trait]
impl ruby_actions::ActionExecutor for DummyExecutor {
    async fn execute(&self, _action: LocalAction) -> Result<String, ruby_actions::ActionError> {
        Ok("ok".into())
    }
}

#[test]
fn test_fuzz_telegram_instruction_evaluator_never_panics_or_escapes() {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path().to_path_buf();
    let policy = Arc::new(DefaultSecurityPolicy::new(vec![root.clone()]));
    let executor = Arc::new(DummyExecutor);
    let conf = ConfirmationManager::new();
    let handler = PolicyGatedCommandHandler::new(policy, executor, conf);

    let prefixes = [
        "list ", "ls ", "read ", "cat ", "head ", "fileinfo ", "sendfile ",
        "git status ", "git log ", "git diff ", "gh pr list ", "gh issue list ",
        "open ", "launch ", "close ", "settings ", "click ", "remind ", "startup ",
        "/start ", "/pair ", "/status ", "/screen ", "/rec ", "/jobs ", "/cancel ",
        "/logs ", "/stop ", "/quit ", "/confirm ", "/deny ",
    ];

    let attack_payloads = [
        "",
        "\0",
        "\0\0\0",
        "../../../../../../Windows/System32",
        "..\\..\\..\\Windows",
        "C:\\Windows\\explorer.exe",
        "D:\\Windows\\cmd.exe",
        "& calc.exe",
        "| notepad.exe",
        "; rm -rf /",
        "$(whoami)",
        "`id`",
        "100 200",
        "-1 -1",
        "99999999999999999999 99999999999999999999",
        "notanumber",
        "enable",
        "disable",
        "invalid_mode",
        "\u{FF0E}\u{FF0E}/secret",
        "\u{FE52}\u{FE52}\\secret",
        "CON", "PRN", "AUX", "NUL",
    ];

    // 1. Structured adversarial combinations
    for &prefix in &prefixes {
        for &payload in &attack_payloads {
            let cmd = format!("{}{}", prefix, payload);
            let decision = handler.evaluate_instruction(&cmd);
            match decision {
                PolicyDecision::Allowed(act) => {
                    // If allowed, ensure it never targets outside the root
                    match act {
                        LocalAction::ListFolder(p) | LocalAction::ReadFileHead { path: p, .. } | LocalAction::SendFile { path: p, .. } | LocalAction::FileInfo(p) => {
                            assert!(
                                PathBuf::from(&p).starts_with(&root),
                                "Allowed action escaped root: {:?} for cmd {:?}",
                                p,
                                cmd
                            );
                        }
                        _ => {}
                    }
                }
                PolicyDecision::RequiresConfirmation { .. } => {}
                PolicyDecision::Denied(_) => {}
            }
        }
    }

    // 2. Randomized generative fuzzing: 5,000 iterations
    let mut rng = SimpleRng::new(0x9876_5432_10FE_DCBA);
    let tokens = [
        " ", "/", "\\", ".", "..", "\0", "\n", "\r", "%00", "%2e", ";", "&", "|", "$", "`",
        "a", "B", "1", "9", "test", "root", "repo", "owner", "http", "https", "cmd", "sh",
        "🦀", "мир", "🌟",
    ];

    for _ in 0..5000 {
        let prefix = prefixes[rng.next_bounded(prefixes.len() as u32) as usize];
        let num_tokens = rng.next_bounded(8) as usize + 1;
        let mut instruction = prefix.to_string();
        for _ in 0..num_tokens {
            let tok = tokens[rng.next_bounded(tokens.len() as u32) as usize];
            instruction.push_str(tok);
        }

        let _ = handler.evaluate_instruction(&instruction);
    }
}
