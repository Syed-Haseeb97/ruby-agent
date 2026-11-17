//! Windows Keep-Awake implementation via SetThreadExecutionState.
//!
//! IMPORTANT OS LIMITATION:
//! SetThreadExecutionState prevents idle system sleep, screen timeout, and standby while AC is plugged in.
//! However, the physical laptop lid-close action is an ACPI hardware power setting controlled exclusively
//! by Windows Power Options ("Choose what closing the lid does -> When I close the lid: Do nothing").
//! Applications cannot override the user's lid-close ACPI action programmatically.

use crate::power::get_system_power_status;
use tracing::{debug, warn};

/// Configures continuous system execution state to prevent sleep when plugged in.
pub fn update_keep_awake_state(enabled: bool) {
    #[cfg(windows)]
    {
        use windows::Win32::System::Power::{
            SetThreadExecutionState, ES_AWAYMODE_REQUIRED, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
        };

        let power = get_system_power_status();
        if enabled && power.is_ac_plugged {
            unsafe {
                // Prevent system sleep and enable away mode while plugged into AC
                SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_AWAYMODE_REQUIRED);
            }
            debug!("Configured Windows SetThreadExecutionState (AC plugged, sleep inhibited)");
        } else {
            unsafe {
                // Reset to default standard execution state
                SetThreadExecutionState(ES_CONTINUOUS);
            }
            if enabled && !power.is_ac_plugged {
                warn!("remote.keep_awake is enabled but laptop is running on battery; sleep inhibition disabled to conserve battery.");
            } else {
                debug!("Reset Windows SetThreadExecutionState to normal continuous mode.");
            }
        }
    }
}

/// RAII Guard that enables keep-awake on construction and resets to normal continuous mode on drop.
pub struct KeepAwakeGuard {
    enabled: bool,
}

impl KeepAwakeGuard {
    pub fn new(enabled: bool) -> Self {
        update_keep_awake_state(enabled);
        Self { enabled }
    }
}

impl Drop for KeepAwakeGuard {
    fn drop(&mut self) {
        if self.enabled {
            update_keep_awake_state(false);
        }
    }
}
