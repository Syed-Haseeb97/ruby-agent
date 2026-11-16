//! Deterministic state machine for Wispr Flow-style hold-to-talk,
//! double-tap hands-free toggle, Esc cancellation, third-key abort, and kill switch.

use crate::hook_types::{HotKeyAction, RawKeyEvent};

pub const MIN_HOLD_DURATION_MS: u64 = 250;
pub const DOUBLE_TAP_MAX_INTERVAL_MS: u64 = 350;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Holding {
        start_ms: u64,
        started_emitted: bool,
    },
    TapPending {
        first_tap_at: u64,
    },
    HandsFree,
}

pub struct HotKeyStateMachine {
    state: State,
    ctrl_down: bool,
    win_down: bool,
    alt_down: bool,
    shift_down: bool,
    min_hold_ms: u64,
    double_tap_interval_ms: u64,
}

impl Default for HotKeyStateMachine {
    fn default() -> Self {
        Self::new(MIN_HOLD_DURATION_MS, DOUBLE_TAP_MAX_INTERVAL_MS)
    }
}

impl HotKeyStateMachine {
    pub fn new(min_hold_ms: u64, double_tap_interval_ms: u64) -> Self {
        Self {
            state: State::Idle,
            ctrl_down: false,
            win_down: false,
            alt_down: false,
            shift_down: false,
            min_hold_ms,
            double_tap_interval_ms,
        }
    }

    pub fn is_hands_free(&self) -> bool {
        matches!(self.state, State::HandsFree)
    }

    pub fn is_holding(&self) -> bool {
        matches!(self.state, State::Holding { .. })
    }

    /// Process an incoming key event and return any triggered hotkey actions.
    pub fn feed_key(&mut self, event: RawKeyEvent, now_ms: u64) -> Vec<HotKeyAction> {
        // Ignore synthetic/injected events to prevent feedback loops
        if event.is_injected {
            return Vec::new();
        }

        let mut actions = Vec::new();

        // 1. Emergency Kill Switch: Ctrl + Alt + Shift + R
        if event.is_key_r() && !event.is_up && self.ctrl_down && self.alt_down && self.shift_down {
            self.state = State::Idle;
            actions.push(HotKeyAction::KillSwitchTriggered);
            return actions;
        }

        // 2. Escape: Cancels any active hold or hands-free session
        if event.is_esc()
            && !event.is_up
            && matches!(self.state, State::Holding { .. } | State::HandsFree)
        {
            self.state = State::Idle;
            actions.push(HotKeyAction::Cancelled);
            return actions;
        }

        // 3. Update modifier states
        let prev_ctrl = self.ctrl_down;
        let prev_win = self.win_down;

        if event.is_ctrl() {
            self.ctrl_down = !event.is_up;
        } else if event.is_win() {
            self.win_down = !event.is_up;
        } else if event.is_alt() {
            self.alt_down = !event.is_up;
        } else if event.is_shift() {
            self.shift_down = !event.is_up;
        }

        let both_now = self.ctrl_down && self.win_down;
        let both_prev = prev_ctrl && prev_win;

        // 4. Third-key abort: If user pressed an unrelated key while holding Ctrl+Win
        if !event.is_ctrl()
            && !event.is_win()
            && !event.is_alt()
            && !event.is_shift()
            && !event.is_up
        {
            if let State::Holding {
                started_emitted, ..
            } = self.state
            {
                self.state = State::Idle;
                if started_emitted {
                    actions.push(HotKeyAction::AbortedThirdKey {
                        vk_code: event.vk_code,
                    });
                }
                return actions;
            }
        }

        // 5. Hotkey transition logic
        if both_now && !both_prev {
            // Both Ctrl and Win just became pressed
            match self.state {
                State::HandsFree => {
                    // Tap again during hands-free turns hands-free off
                    self.state = State::Idle;
                    actions.push(HotKeyAction::DoubleTapToggled { active: false });
                }
                State::TapPending { first_tap_at } => {
                    let interval = now_ms.saturating_sub(first_tap_at);
                    if interval <= self.double_tap_interval_ms {
                        // Successful double-tap! Turn on hands-free dictation
                        self.state = State::HandsFree;
                        actions.push(HotKeyAction::DoubleTapToggled { active: true });
                    } else {
                        // First tap expired, begin new hold
                        self.state = State::Holding {
                            start_ms: now_ms,
                            started_emitted: false,
                        };
                    }
                }
                _ => {
                    self.state = State::Holding {
                        start_ms: now_ms,
                        started_emitted: false,
                    };
                }
            }
        } else if !both_now && both_prev {
            // At least one of Ctrl or Win was released
            if let State::Holding {
                start_ms,
                started_emitted,
            } = self.state
            {
                let duration = now_ms.saturating_sub(start_ms);
                if duration >= self.min_hold_ms || started_emitted {
                    // Valid hold completed
                    self.state = State::Idle;
                    actions.push(HotKeyAction::HoldReleased {
                        duration_ms: duration,
                    });
                } else {
                    // Short press (<250ms): register as potential first tap of a double-tap
                    self.state = State::TapPending {
                        first_tap_at: now_ms,
                    };
                }
            }
        }

        actions
    }

    /// Periodic timer tick to emit HoldStarted or expire pending taps.
    pub fn tick(&mut self, now_ms: u64) -> Vec<HotKeyAction> {
        let mut actions = Vec::new();

        match &mut self.state {
            State::Holding {
                start_ms,
                started_emitted,
            } => {
                if !*started_emitted && now_ms.saturating_sub(*start_ms) >= self.min_hold_ms {
                    *started_emitted = true;
                    actions.push(HotKeyAction::HoldStarted);
                }
            }
            State::TapPending { first_tap_at }
                if now_ms.saturating_sub(*first_tap_at) > self.double_tap_interval_ms =>
            {
                self.state = State::Idle;
            }
            _ => {}
        }

        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hook_types::*;

    fn key(vk: u16, is_up: bool, ts: u64) -> RawKeyEvent {
        RawKeyEvent {
            vk_code: vk,
            is_up,
            is_injected: false,
            timestamp_ms: ts,
        }
    }

    #[test]
    fn test_hold_to_talk_workflow() {
        let mut machine = HotKeyStateMachine::default();

        // t=0: Ctrl down
        let a = machine.feed_key(key(VK_LCONTROL_VALUE, false, 0), 0);
        assert!(a.is_empty());

        // t=10: Win down (both now down)
        let a = machine.feed_key(key(VK_LWIN_VALUE, false, 10), 10);
        assert!(a.is_empty());

        // t=100: Tick before threshold (no action yet)
        let a = machine.tick(100);
        assert!(a.is_empty());

        // t=260: Threshold reached (250ms hold) -> HoldStarted emitted!
        let a = machine.tick(260);
        assert_eq!(a, vec![HotKeyAction::HoldStarted]);

        // t=1200: Win released -> HoldReleased emitted!
        let a = machine.feed_key(key(VK_LWIN_VALUE, true, 1200), 1200);
        assert_eq!(a, vec![HotKeyAction::HoldReleased { duration_ms: 1190 }]);
    }

    #[test]
    fn test_double_tap_hands_free_workflow() {
        let mut machine = HotKeyStateMachine::default();

        // Tap 1: press and release quickly (<100ms)
        machine.feed_key(key(VK_LCONTROL_VALUE, false, 0), 0);
        machine.feed_key(key(VK_LWIN_VALUE, false, 10), 10);
        let a = machine.feed_key(key(VK_LWIN_VALUE, true, 60), 60);
        machine.feed_key(key(VK_LCONTROL_VALUE, true, 70), 70);
        assert!(a.is_empty()); // No hold released, registered as tap 1

        // Tap 2: within 350ms window (at t=180ms)
        machine.feed_key(key(VK_LCONTROL_VALUE, false, 170), 170);
        let a = machine.feed_key(key(VK_LWIN_VALUE, false, 180), 180);
        assert_eq!(a, vec![HotKeyAction::DoubleTapToggled { active: true }]);
        assert!(machine.is_hands_free());

        // Tap 3: turns hands-free off
        machine.feed_key(key(VK_LWIN_VALUE, true, 300), 300);
        machine.feed_key(key(VK_LCONTROL_VALUE, true, 310), 310);
        machine.feed_key(key(VK_LCONTROL_VALUE, false, 500), 500);
        let a = machine.feed_key(key(VK_LWIN_VALUE, false, 510), 510);
        assert_eq!(a, vec![HotKeyAction::DoubleTapToggled { active: false }]);
        assert!(!machine.is_hands_free());
    }

    #[test]
    fn test_escape_cancels_hold_and_hands_free() {
        let mut machine = HotKeyStateMachine::default();

        // Start hold
        machine.feed_key(key(VK_LCONTROL_VALUE, false, 0), 0);
        machine.feed_key(key(VK_LWIN_VALUE, false, 10), 10);
        let _ = machine.tick(270);

        // Press Esc
        let a = machine.feed_key(key(VK_ESCAPE_VALUE, false, 400), 400);
        assert_eq!(a, vec![HotKeyAction::Cancelled]);
    }

    #[test]
    fn test_third_key_aborts_hold() {
        let mut machine = HotKeyStateMachine::default();

        // Start hold
        machine.feed_key(key(VK_LCONTROL_VALUE, false, 0), 0);
        machine.feed_key(key(VK_LWIN_VALUE, false, 10), 10);
        let _ = machine.tick(270);

        // Press 'D' (0x44) as third key
        let a = machine.feed_key(key(0x44, false, 350), 350);
        assert_eq!(a, vec![HotKeyAction::AbortedThirdKey { vk_code: 0x44 }]);
    }

    #[test]
    fn test_emergency_kill_switch() {
        let mut machine = HotKeyStateMachine::default();

        // Ctrl + Alt + Shift held down
        machine.feed_key(key(VK_LCONTROL_VALUE, false, 0), 0);
        machine.feed_key(key(VK_LMENU_VALUE, false, 10), 10);
        machine.feed_key(key(VK_LSHIFT_VALUE, false, 20), 20);

        // Press 'R'
        let a = machine.feed_key(key(VK_KEY_R_VALUE, false, 50), 50);
        assert_eq!(a, vec![HotKeyAction::KillSwitchTriggered]);
    }

    #[test]
    fn test_injected_events_are_ignored() {
        let mut machine = HotKeyStateMachine::default();

        let injected_key = RawKeyEvent {
            vk_code: VK_LWIN_VALUE,
            is_up: false,
            is_injected: true,
            timestamp_ms: 100,
        };

        let a = machine.feed_key(injected_key, 100);
        assert!(a.is_empty());
        assert!(!machine.win_down);
    }
}
