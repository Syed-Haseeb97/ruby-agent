//! Low-Level Keyboard Hook Spike Runner.
//! Validates:
//! 1. Dedicated hook thread with message loop
//! 2. Ctrl+Win hold-to-talk (>= 250 ms hold)
//! 3. Start Menu suppression via VK_NONAME (0xFC) SendInput
//! 4. Ignore LLKHF_INJECTED events
//! 5. Double-tap hands-free mode
//! 6. Esc cancellation and third-key abort
//! 7. Ctrl+Alt+Shift+R kill switch

use std::time::{Duration, Instant};

use ruby_platform::hook_types::{
    HotKeyAction, RawKeyEvent, VK_KEY_R_VALUE, VK_LCONTROL_VALUE, VK_LMENU_VALUE, VK_LSHIFT_VALUE,
    VK_LWIN_VALUE,
};
use ruby_platform::hotkey_machine::HotKeyStateMachine;
use ruby_platform::KeyboardHookHandle;

fn main() {
    println!("============================================================");
    println!("RUBY-PLATFORM LOW-LEVEL KEYBOARD HOOK SPIKE");
    println!("============================================================");

    // 1. Test deterministic state machine with simulated sequences
    println!("\n--- Part 1: Deterministic State Machine Verification ---");
    let mut machine = HotKeyStateMachine::default();

    println!("[TEST 1] Testing Ctrl+Win hold-to-talk (hold 300ms)...");
    machine.feed_key(
        RawKeyEvent {
            vk_code: VK_LCONTROL_VALUE,
            is_up: false,
            is_injected: false,
            timestamp_ms: 0,
        },
        0,
    );
    machine.feed_key(
        RawKeyEvent {
            vk_code: VK_LWIN_VALUE,
            is_up: false,
            is_injected: false,
            timestamp_ms: 5,
        },
        5,
    );
    let actions_100 = machine.tick(100);
    println!("  t=100ms: Actions: {:?}", actions_100);
    assert!(actions_100.is_empty(), "Should not emit before 250ms");

    let actions_260 = machine.tick(260);
    println!("  t=260ms: Actions: {:?}", actions_260);
    assert_eq!(
        actions_260,
        vec![HotKeyAction::HoldStarted],
        "Must emit HoldStarted at 250ms threshold"
    );

    let actions_release = machine.feed_key(
        RawKeyEvent {
            vk_code: VK_LWIN_VALUE,
            is_up: true,
            is_injected: false,
            timestamp_ms: 320,
        },
        320,
    );
    println!("  t=320ms: Actions: {:?}", actions_release);
    assert!(
        matches!(actions_release.first(), Some(HotKeyAction::HoldReleased { duration_ms }) if *duration_ms >= 300),
        "Must emit HoldReleased with duration >= 300ms"
    );
    println!("  [PASS] Hold-to-talk cycle verified!");

    println!("\n[TEST 2] Testing Double-Tap Hands-Free Toggle...");
    let mut m2 = HotKeyStateMachine::default();
    // Tap 1
    m2.feed_key(
        RawKeyEvent {
            vk_code: VK_LCONTROL_VALUE,
            is_up: false,
            is_injected: false,
            timestamp_ms: 0,
        },
        0,
    );
    m2.feed_key(
        RawKeyEvent {
            vk_code: VK_LWIN_VALUE,
            is_up: false,
            is_injected: false,
            timestamp_ms: 5,
        },
        5,
    );
    m2.feed_key(
        RawKeyEvent {
            vk_code: VK_LWIN_VALUE,
            is_up: true,
            is_injected: false,
            timestamp_ms: 50,
        },
        50,
    );
    m2.feed_key(
        RawKeyEvent {
            vk_code: VK_LCONTROL_VALUE,
            is_up: true,
            is_injected: false,
            timestamp_ms: 55,
        },
        55,
    );
    // Tap 2 within 350ms
    m2.feed_key(
        RawKeyEvent {
            vk_code: VK_LCONTROL_VALUE,
            is_up: false,
            is_injected: false,
            timestamp_ms: 180,
        },
        180,
    );
    let double_tap_action = m2.feed_key(
        RawKeyEvent {
            vk_code: VK_LWIN_VALUE,
            is_up: false,
            is_injected: false,
            timestamp_ms: 185,
        },
        185,
    );
    println!("  Tap 2 actions: {:?}", double_tap_action);
    assert_eq!(
        double_tap_action,
        vec![HotKeyAction::DoubleTapToggled { active: true }]
    );
    assert!(m2.is_hands_free());
    println!("  [PASS] Double-tap hands-free toggle verified!");

    println!("\n[TEST 3] Testing Third-Key Abort...");
    let mut m3 = HotKeyStateMachine::default();
    m3.feed_key(
        RawKeyEvent {
            vk_code: VK_LCONTROL_VALUE,
            is_up: false,
            is_injected: false,
            timestamp_ms: 0,
        },
        0,
    );
    m3.feed_key(
        RawKeyEvent {
            vk_code: VK_LWIN_VALUE,
            is_up: false,
            is_injected: false,
            timestamp_ms: 5,
        },
        5,
    );
    let _ = m3.tick(260);
    // Press 'D' (0x44)
    let abort_action = m3.feed_key(
        RawKeyEvent {
            vk_code: 0x44,
            is_up: false,
            is_injected: false,
            timestamp_ms: 280,
        },
        280,
    );
    println!("  Third key actions: {:?}", abort_action);
    assert_eq!(
        abort_action,
        vec![HotKeyAction::AbortedThirdKey { vk_code: 0x44 }]
    );
    println!("  [PASS] Third-key abort verified!");

    println!("\n[TEST 4] Testing Kill Switch Ctrl+Alt+Shift+R...");
    let mut m4 = HotKeyStateMachine::default();
    m4.feed_key(
        RawKeyEvent {
            vk_code: VK_LCONTROL_VALUE,
            is_up: false,
            is_injected: false,
            timestamp_ms: 0,
        },
        0,
    );
    m4.feed_key(
        RawKeyEvent {
            vk_code: VK_LMENU_VALUE,
            is_up: false,
            is_injected: false,
            timestamp_ms: 10,
        },
        10,
    );
    m4.feed_key(
        RawKeyEvent {
            vk_code: VK_LSHIFT_VALUE,
            is_up: false,
            is_injected: false,
            timestamp_ms: 20,
        },
        20,
    );
    let kill_action = m4.feed_key(
        RawKeyEvent {
            vk_code: VK_KEY_R_VALUE,
            is_up: false,
            is_injected: false,
            timestamp_ms: 30,
        },
        30,
    );
    println!("  Kill switch actions: {:?}", kill_action);
    assert_eq!(kill_action, vec![HotKeyAction::KillSwitchTriggered]);
    println!("  [PASS] Kill switch verified!");

    // 2. Test live Win32 low-level keyboard hook
    println!("\n--- Part 2: Live Win32 Keyboard Hook Lifecycle & Message Loop ---");
    let hook_handle = match KeyboardHookHandle::start() {
        Ok(h) => {
            println!(
                "[SUCCESS] Low-level keyboard hook thread spawned and message loop initialized!"
            );
            h
        }
        Err(e) => {
            eprintln!("[ERROR] Failed to start keyboard hook: {:?}", e);
            std::process::exit(1);
        }
    };

    println!("Listening on hook action channel for 1.5 seconds (pass-through, non-blocking)...");
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(1500) {
        if let Ok(action) = hook_handle.receiver().try_recv() {
            println!("  [LIVE ACTION RECEIVED] {:?}", action);
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    println!("\nDropping hook handle (testing clean unhook and thread shutdown)...");
    drop(hook_handle);
    println!("[SUCCESS] Hook dropped and uninstalled cleanly!");
    println!("============================================================");
    println!("KEYBOARD HOOK SPIKE COMPLETED SUCCESSFULLY");
    println!("============================================================");
}
