use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, State, Window};
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, WS_EX_TRANSPARENT},
};

use ruby_core::{Emotion, OperationalState};

use crate::{AppState, SequencedOrbEvent};

#[tauri::command]
pub fn get_orb_state(state: State<'_, AppState>) -> SequencedOrbEvent {
    let current = *state.orb_state.lock().unwrap();
    let seq = state.seq_counter.load(Ordering::SeqCst);
    SequencedOrbEvent {
        seq,
        state: current,
    }
}

#[tauri::command]
pub fn set_orb_state(
    app: AppHandle,
    state: State<'_, AppState>,
    operational: OperationalState,
    emotion: Emotion,
    thinking_spin: Option<bool>,
    speaking_active: Option<bool>,
) -> Result<SequencedOrbEvent, String> {
    let mut current = state.orb_state.lock().unwrap();
    current.operational = operational;
    current.emotion = emotion;
    if let Some(spin) = thinking_spin {
        current.thinking_spin_active = spin;
    }
    if let Some(speaking) = speaking_active {
        current.speaking_active = speaking;
    }
    let seq = state.seq_counter.fetch_add(1, Ordering::SeqCst) + 1;
    let event = SequencedOrbEvent {
        seq,
        state: *current,
    };

    app.emit("orb_state_changed", &event)
        .map_err(|e| e.to_string())?;

    Ok(event)
}

#[tauri::command]
pub fn toggle_orb_click_through(
    window: Window,
    state: State<'_, AppState>,
    enable: bool,
) -> Result<bool, String> {
    if window.label() != "orb" {
        return Ok(false);
    }
    let hwnd_val = window.hwnd().map_err(|e| e.to_string())?;
    let hwnd = HWND(hwnd_val.0 as *mut _);

    unsafe {
        let current_ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
        let new_ex = if enable {
            current_ex | WS_EX_TRANSPARENT.0 as i32
        } else {
            current_ex & !(WS_EX_TRANSPARENT.0 as i32)
        };
        SetWindowLongW(hwnd, GWL_EXSTYLE, new_ex);
    }

    state.click_through.store(enable, Ordering::SeqCst);
    Ok(enable)
}

#[cfg(feature = "test-capture")]
#[tauri::command]
pub fn save_orb_snapshot(name: String, base64_png: String) -> Result<String, String> {
    use base64::Engine;
    let clean = if let Some(idx) = base64_png.find("base64,") {
        &base64_png[idx + 7..]
    } else {
        &base64_png
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(clean.trim())
        .map_err(|e| e.to_string())?;

    let out_dir = std::path::PathBuf::from(r"D:\projects\ruby-agent\acceptance\tauri_rendered");
    let _ = std::fs::create_dir_all(&out_dir);
    let target = out_dir.join(format!("{}.png", name));
    std::fs::write(&target, bytes).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().to_string())
}
