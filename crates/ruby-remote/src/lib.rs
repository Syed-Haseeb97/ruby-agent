//! `ruby-remote`: Multi-platform Remote Control engine for Ruby.
//!
//! Phase R0: Telegram channel adapter, state machine, confirmation plumbing,
//! power queries, and security audit log.

pub mod audit;
pub mod confirmation;
pub mod keep_awake;
pub mod key_storage;
pub mod power;
pub mod r1_router;
pub mod state_machine;
pub mod telegram;
pub mod traits;
pub mod types;

pub use audit::AuditLogger;
pub use confirmation::{ConfirmationManager, PendingAction, CONFIRMATION_TTL};
pub use keep_awake::{update_keep_awake_state, KeepAwakeGuard};
pub use key_storage::{
    get_ruby_data_dir, get_telegram_bot_token, load_persisted_offset, load_remote_config,
    save_persisted_offset, save_remote_config, set_telegram_bot_token, RemoteConfig,
    RemoteOffsetState,
};
pub use power::{get_system_power_status, SystemPowerSummary};
pub use r1_router::{PolicyDecision, PolicyGatedCommandHandler};
pub use state_machine::TaskManager;
pub use telegram::TelegramAdapter;
pub use traits::{ChannelAdapter, NoopCommandHandler, RemoteCommandHandler};
pub use types::{IncomingMessage, RemoteError, RemoteStatus, RemoteTask, TaskState};
