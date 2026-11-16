//! Event and action types for low-level keyboard input and hotkey detection.

pub const VK_NONAME_VALUE: u16 = 0xFC;
pub const VK_ESCAPE_VALUE: u16 = 0x1B;
pub const VK_LCONTROL_VALUE: u16 = 0xA2;
pub const VK_RCONTROL_VALUE: u16 = 0xA3;
pub const VK_CONTROL_VALUE: u16 = 0x11;
pub const VK_LWIN_VALUE: u16 = 0x5B;
pub const VK_RWIN_VALUE: u16 = 0x5C;
pub const VK_LSHIFT_VALUE: u16 = 0xA0;
pub const VK_RSHIFT_VALUE: u16 = 0xA1;
pub const VK_LMENU_VALUE: u16 = 0xA4;
pub const VK_RMENU_VALUE: u16 = 0xA5;
pub const VK_KEY_R_VALUE: u16 = 0x52;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawKeyEvent {
    pub vk_code: u16,
    pub is_up: bool,
    pub is_injected: bool,
    pub timestamp_ms: u64,
}

impl RawKeyEvent {
    pub fn is_ctrl(&self) -> bool {
        matches!(
            self.vk_code,
            VK_LCONTROL_VALUE | VK_RCONTROL_VALUE | VK_CONTROL_VALUE
        )
    }

    pub fn is_win(&self) -> bool {
        matches!(self.vk_code, VK_LWIN_VALUE | VK_RWIN_VALUE)
    }

    pub fn is_esc(&self) -> bool {
        self.vk_code == VK_ESCAPE_VALUE
    }

    pub fn is_shift(&self) -> bool {
        matches!(self.vk_code, VK_LSHIFT_VALUE | VK_RSHIFT_VALUE)
    }

    pub fn is_alt(&self) -> bool {
        matches!(self.vk_code, VK_LMENU_VALUE | VK_RMENU_VALUE)
    }

    pub fn is_key_r(&self) -> bool {
        self.vk_code == VK_KEY_R_VALUE
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotKeyAction {
    /// Ctrl+Win held down and duration has passed minimum hold threshold (~250 ms)
    HoldStarted,
    /// Ctrl+Win released after holding
    HoldReleased { duration_ms: u64 },
    /// Quick double-tap of Ctrl+Win toggled hands-free mode
    DoubleTapToggled { active: bool },
    /// Escape pressed during dictation
    Cancelled,
    /// Third key pressed during hold (aborts dictation, passes through normal shortcut)
    AbortedThirdKey { vk_code: u16 },
    /// Ctrl+Alt+Shift+R emergency kill switch triggered
    KillSwitchTriggered,
}
