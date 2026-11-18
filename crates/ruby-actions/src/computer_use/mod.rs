//! Computer Use module: actions, native/mock executors, safety policies, and the agent loop.

pub mod action;
pub mod agent_loop;
pub mod executor;

pub use action::{
    map_normalized_to_physical, map_physical_to_normalized, ComputerAction, MouseButton,
};
pub use agent_loop::{
    ComputerUseAgentConfig, ComputerUseAgentLoop, ComputerUseModelClient, KillSwitchToken,
    MockComputerUseModelClient, StepRecord, TaskStatus,
};
pub use executor::{ComputerUseExecutor, HeldInputs, MockComputerUseExecutor, ScreenCapture};

#[cfg(windows)]
pub use executor::WindowsComputerUseExecutor;
