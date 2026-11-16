//! Ruby Desktop Tauri 2.0 Application Shell.
//! Coordinates the transparent floating Liquid Orb window,
//! the Editorial Control Console, typed IPC, and Win32 window properties.

use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Mutex;
use tauri::{Manager, Window};
use windows::Win32::{
    Foundation::HWND,
    Graphics::Dwm::{
        DwmSetWindowAttribute, DWMNCRENDERINGPOLICY, DWMNCRP_DISABLED, DWMWA_NCRENDERING_POLICY,
    },
    UI::WindowsAndMessaging::{
        GetWindowLongW, SetWindowDisplayAffinity, SetWindowLongW, GWL_EXSTYLE,
        WINDOW_DISPLAY_AFFINITY, WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW,
    },
};

use ruby_core::OrbVisualState;

pub mod commands;

pub struct AppState {
    pub orb_state: Mutex<OrbVisualState>,
    pub seq_counter: AtomicU64,
    pub click_through: AtomicBool,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct SequencedOrbEvent {
    pub seq: u64,
    pub state: OrbVisualState,
}

pub fn configure_orb_window_win32(window: &Window) {
    if let Ok(hwnd_val) = window.hwnd() {
        let hwnd = HWND(hwnd_val.0 as *mut _);
        unsafe {
            // Apply WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED, and strictly CLEAR WS_EX_APPWINDOW
            let current_ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
            let target_ex = (current_ex & !(WS_EX_APPWINDOW.0 as i32))
                | (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_LAYERED.0) as i32;
            SetWindowLongW(hwnd, GWL_EXSTYLE, target_ex);

            // Exclude from captures using WDA_EXCLUDEFROMCAPTURE (0x00000011)
            let _ = SetWindowDisplayAffinity(hwnd, WINDOW_DISPLAY_AFFINITY(0x00000011));

            // Disable DWM non-client rendering policy to ensure zero shadow
            let disable_nc: DWMNCRENDERINGPOLICY = DWMNCRP_DISABLED;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_NCRENDERING_POLICY,
                &disable_nc as *const _ as *const _,
                std::mem::size_of::<DWMNCRENDERINGPOLICY>() as u32,
            );
        }
    }
}

pub fn run() {
    let builder = tauri::Builder::default().manage(AppState {
        orb_state: Mutex::new(OrbVisualState::default()),
        seq_counter: AtomicU64::new(1),
        click_through: AtomicBool::new(false),
    });

    #[cfg(feature = "test-capture")]
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::get_orb_state,
        commands::set_orb_state,
        commands::toggle_orb_click_through,
        commands::save_orb_snapshot
    ]);

    #[cfg(not(feature = "test-capture"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::get_orb_state,
        commands::set_orb_state,
        commands::toggle_orb_click_through
    ]);

    builder
        .setup(|app| {
            if let Some(orb_win) = app.get_webview_window("orb") {
                configure_orb_window_win32(&orb_win.as_ref().window());
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running ruby application");
}
