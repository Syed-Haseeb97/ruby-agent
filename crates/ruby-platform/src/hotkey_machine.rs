//! Deterministic state machine for Wispr Flow-style hold-to-talk,
//! pre-roll buffering, double-tap hands-free toggle, Esc cancellation, third-key abort, and kill switch.

use crate::hook_types::{HotKeyAction, RawKeyEvent};

pub const MIN_HOLD_DURATION_MS: u64 = 250;
pub const DOUBLE_TAP_MAX_INTERVAL_MS: u64 = 350;

/// Decision logic for Start Menu suppression dummy key injection.
/// Inject dummy key ONLY when the dictation chord (both Ctrl and Win) is active, NEVER on a plain Win tap.
pub fn should_inject_dummy_suppression(
    event: RawKeyEvent,
    ctrl_down: bool,
    win_down: bool,
) -> bool {
    if event.is_injected {
        return false;
    }

    // A plain Win press/release must NEVER inject the suppression key!
    if event.is_win() && !ctrl_down {
        return false;
    }

    // Dictation chord is active if both Ctrl and Win are engaged
    (ctrl_down && event.is_win() && !event.is_up)
        || (win_down && event.is_ctrl() && !event.is_up)
        || (ctrl_down && win_down)
}

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
    pub ctrl_down: bool,
    pub win_down: bool,
    pub alt_down: bool,
    pub shift_down: bool,
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

    /// Returns true only when both dictation chord modifiers (Ctrl and Win) are fully released.
    pub fn all_chord_modifiers_released(&self) -> bool {
        !self.ctrl_down && !self.win_down
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
                } else {
                    actions.push(HotKeyAction::PreRollDiscarded);
                }
                return actions;
            }
        }

        // 5. Hotkey transition logic
        if both_now && !both_prev {
            // Both Ctrl and Win just became pressed -> Start pre-roll capture immediately!
            actions.push(HotKeyAction::PreRollStarted);

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
                    // Short press (<250ms): discard pre-roll buffer and register as first tap
                    actions.push(HotKeyAction::PreRollDiscarded);
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
    fn test_plain_win_press_and_release_never_injects_suppression() {
        // Rule: Start-menu suppression must NEVER inject on plain Win tap
        let win_press = key(VK_LWIN_VALUE, false, 0);
        let win_release = key(VK_LWIN_VALUE, true, 80);

        // Plain Win pressed (Ctrl is NOT down)
        assert!(
            !should_inject_dummy_suppression(win_press, false, false),
            "Plain Win press must NOT trigger suppression"
        );
        assert!(
            !should_inject_dummy_suppression(win_release, false, true),
            "Plain Win release must NOT trigger suppression"
        );
    }

    #[test]
    fn test_chord_ctrl_win_triggers_suppression() {
        // Case 1: Ctrl is already down, then Win is pressed
        let win_press = key(VK_LWIN_VALUE, false, 10);
        assert!(
            should_inject_dummy_suppression(win_press, true, false),
            "Ctrl+Win chord must trigger suppression"
        );

        // Case 2: Win is down, then Ctrl is pressed
        let ctrl_press = key(VK_LCONTROL_VALUE, false, 10);
        assert!(
            should_inject_dummy_suppression(ctrl_press, false, true),
            "Win+Ctrl chord must trigger suppression"
        );
    }

    #[test]
    fn test_pre_roll_started_and_committed() {
        let mut machine = HotKeyStateMachine::default();

        // t=0: Ctrl down
        let a0 = machine.feed_key(key(VK_LCONTROL_VALUE, false, 0), 0);
        assert!(a0.is_empty());

        // t=5: Win down (chord active) -> PreRollStarted must be emitted immediately!
        let a1 = machine.feed_key(key(VK_LWIN_VALUE, false, 5), 5);
        assert_eq!(a1, vec![HotKeyAction::PreRollStarted]);

        // t=100: Under 250ms -> nothing more yet
        let a_tick1 = machine.tick(100);
        assert!(a_tick1.is_empty());

        // t=260: Passes 250ms hold threshold -> HoldStarted emitted (commits recording)
        let a_tick2 = machine.tick(260);
        assert_eq!(a_tick2, vec![HotKeyAction::HoldStarted]);
    }

    #[test]
    fn test_pre_roll_discarded_on_early_release() {
        let mut machine = HotKeyStateMachine::default();

        // Press chord
        machine.feed_key(key(VK_LCONTROL_VALUE, false, 0), 0);
        let a_start = machine.feed_key(key(VK_LWIN_VALUE, false, 5), 5);
        assert_eq!(a_start, vec![HotKeyAction::PreRollStarted]);

        // Release early at 80ms (< 250ms)
        let a_rel = machine.feed_key(key(VK_LWIN_VALUE, true, 80), 80);
        assert_eq!(
            a_rel,
            vec![HotKeyAction::PreRollDiscarded],
            "Early release before 250ms must discard pre-roll buffer"
        );
    }

    #[test]
    fn test_out_of_order_modifier_release() {
        let mut machine = HotKeyStateMachine::default();

        // Press chord
        machine.feed_key(key(VK_LCONTROL_VALUE, false, 0), 0);
        machine.feed_key(key(VK_LWIN_VALUE, false, 10), 10);
        let _ = machine.tick(260);

        // Win released first at 1000ms
        let a1 = machine.feed_key(key(VK_LWIN_VALUE, true, 1000), 1000);
        assert!(matches!(
            a1.first(),
            Some(HotKeyAction::HoldReleased { .. })
        ));
        // Ctrl is still down!
        assert!(!machine.all_chord_modifiers_released());

        // Ctrl released 120ms later at 1120ms
        let _ = machine.feed_key(key(VK_LCONTROL_VALUE, true, 1120), 1120);
        assert!(
            machine.all_chord_modifiers_released(),
            "Both modifiers now released"
        );
    }

    #[test]
    fn test_double_tap_hands_free_workflow() {
        let mut machine = HotKeyStateMachine::default();

        // Tap 1 (<100ms)
        machine.feed_key(key(VK_LCONTROL_VALUE, false, 0), 0);
        machine.feed_key(key(VK_LWIN_VALUE, false, 10), 10);
        machine.feed_key(key(VK_LWIN_VALUE, true, 60), 60);
        machine.feed_key(key(VK_LCONTROL_VALUE, true, 70), 70);

        // Tap 2 within 350ms window
        machine.feed_key(key(VK_LCONTROL_VALUE, false, 170), 170);
        let a = machine.feed_key(key(VK_LWIN_VALUE, false, 180), 180);
        assert_eq!(
            a,
            vec![
                HotKeyAction::PreRollStarted,
                HotKeyAction::DoubleTapToggled { active: true }
            ]
        );
        assert!(machine.is_hands_free());

        // Tap 3 turns hands-free off
        machine.feed_key(key(VK_LWIN_VALUE, true, 300), 300);
        machine.feed_key(key(VK_LCONTROL_VALUE, true, 310), 310);
        machine.feed_key(key(VK_LCONTROL_VALUE, false, 500), 500);
        let a = machine.feed_key(key(VK_LWIN_VALUE, false, 510), 510);
        assert_eq!(
            a,
            vec![
                HotKeyAction::PreRollStarted,
                HotKeyAction::DoubleTapToggled { active: false }
            ]
        );
        assert!(!machine.is_hands_free());
    }

    #[test]
    fn test_escape_cancels_hold_and_hands_free() {
        let mut machine = HotKeyStateMachine::default();

        machine.feed_key(key(VK_LCONTROL_VALUE, false, 0), 0);
        machine.feed_key(key(VK_LWIN_VALUE, false, 10), 10);
        let _ = machine.tick(270);

        let a = machine.feed_key(key(VK_ESCAPE_VALUE, false, 400), 400);
        assert_eq!(a, vec![HotKeyAction::Cancelled]);
    }

    #[test]
    fn test_third_key_aborts_hold() {
        let mut machine = HotKeyStateMachine::default();

        machine.feed_key(key(VK_LCONTROL_VALUE, false, 0), 0);
        machine.feed_key(key(VK_LWIN_VALUE, false, 10), 10);
        let _ = machine.tick(270);

        let a = machine.feed_key(key(0x44, false, 350), 350);
        assert_eq!(a, vec![HotKeyAction::AbortedThirdKey { vk_code: 0x44 }]);
    }

    #[test]
    fn test_emergency_kill_switch() {
        let mut machine = HotKeyStateMachine::default();

        machine.feed_key(key(VK_LCONTROL_VALUE, false, 0), 0);
        machine.feed_key(key(VK_LMENU_VALUE, false, 10), 10);
        machine.feed_key(key(VK_LSHIFT_VALUE, false, 20), 20);

        let a = machine.feed_key(key(VK_KEY_R_VALUE, false, 50), 50);
        assert_eq!(a, vec![HotKeyAction::KillSwitchTriggered]);
    }
}
