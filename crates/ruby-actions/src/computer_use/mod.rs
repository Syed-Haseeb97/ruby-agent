//! Computer Use module: actions, native/mock executors, safety policies, and the agent loop.

pub mod action;
pub mod agent_loop;
pub mod executor;
pub mod gemini_vision;

pub use action::{
    map_normalized_to_physical, map_normalized_to_virtual, map_physical_to_normalized,
    map_virtual_to_normalized, ComputerAction, MonitorGeometry, MouseButton,
};
pub use agent_loop::{
    ComputerUseAgentConfig, ComputerUseAgentLoop, ComputerUseModelClient, KillSwitchToken,
    MockComputerUseModelClient, PlatformSecurityInspector, StepRecord, TaskStatus,
};
pub use executor::{ComputerUseExecutor, HeldInputs, MockComputerUseExecutor, ScreenCapture};
pub use gemini_vision::GeminiVisionComputerUseModelClient;

#[cfg(windows)]
pub use executor::WindowsComputerUseExecutor;
