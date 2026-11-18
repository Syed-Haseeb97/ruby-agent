//! Computer Use Executor: native SendInput dispatch, DPI-aware GDI screen capture, and Mock implementation.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

use super::action::{map_normalized_to_physical, ComputerAction, MouseButton};
use crate::ActionError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenCapture {
    pub width: u32,
    pub height: u32,
    pub jpeg_bytes: Vec<u8>,
}

#[derive(Default)]
pub struct HeldInputs {
    pub mouse_left_down: bool,
    pub mouse_right_down: bool,
    pub mouse_middle_down: bool,
    pub held_keys: HashSet<u16>,
}

#[async_trait]
pub trait ComputerUseExecutor: Send + Sync {
    async fn capture_screen(&self) -> Result<ScreenCapture, ActionError>;
    async fn execute_action(&self, action: &ComputerAction) -> Result<(), ActionError>;
    async fn release_all_held_inputs(&self) -> Result<(), ActionError>;
    fn screen_dimensions(&self) -> (u32, u32);
    fn is_dry_run(&self) -> bool;
}

#[cfg(windows)]
mod windows_native {
    use super::*;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        HBITMAP, HDC, SRCCOPY,
    };
    use windows::Win32::UI::HiDpi::{
        SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN,
        MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
        MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEINPUT,
        MOUSE_EVENT_FLAGS, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE,
        VK_HOME, VK_LEFT, VK_LWIN, VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SHIFT,
        VK_SPACE, VK_TAB, VK_UP,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    pub struct WindowsComputerUseExecutor {
        pub dry_run: bool,
        pub held_inputs: Arc<Mutex<HeldInputs>>,
    }

    impl WindowsComputerUseExecutor {
        pub fn new(dry_run: bool) -> Self {
            unsafe {
                let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            }
            Self {
                dry_run,
                held_inputs: Arc::new(Mutex::new(HeldInputs::default())),
            }
        }

        fn get_system_screen_size() -> (u32, u32) {
            unsafe {
                let w = GetSystemMetrics(SM_CXSCREEN) as u32;
                let h = GetSystemMetrics(SM_CYSCREEN) as u32;
                (w.max(800), h.max(600))
            }
        }

        fn capture_screen_raw() -> Result<ScreenCapture, ActionError> {
            unsafe {
                let (width, height) = Self::get_system_screen_size();
                let hdc_screen: HDC = GetDC(HWND(std::ptr::null_mut()));
                if hdc_screen.0.is_null() {
                    return Err(ActionError::Failed("GetDC failed".into()));
                }

                let hdc_mem: HDC = CreateCompatibleDC(hdc_screen);
                if hdc_mem.0.is_null() {
                    let _ = ReleaseDC(HWND(std::ptr::null_mut()), hdc_screen);
                    return Err(ActionError::Failed("CreateCompatibleDC failed".into()));
                }

                let hbm: HBITMAP = CreateCompatibleBitmap(hdc_screen, width as i32, height as i32);
                if hbm.0.is_null() {
                    let _ = DeleteDC(hdc_mem);
                    let _ = ReleaseDC(HWND(std::ptr::null_mut()), hdc_screen);
                    return Err(ActionError::Failed("CreateCompatibleBitmap failed".into()));
                }

                let old_obj = SelectObject(hdc_mem, hbm);
                let blt_res = BitBlt(
                    hdc_mem,
                    0,
                    0,
                    width as i32,
                    height as i32,
                    hdc_screen,
                    0,
                    0,
                    SRCCOPY,
                );

                if blt_res.is_err() {
                    let _ = SelectObject(hdc_mem, old_obj);
                    let _ = DeleteObject(hbm);
                    let _ = DeleteDC(hdc_mem);
                    let _ = ReleaseDC(HWND(std::ptr::null_mut()), hdc_screen);
                    return Err(ActionError::Failed("BitBlt failed".into()));
                }

                let mut bi = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: width as i32,
                        biHeight: -(height as i32), // Top-down DIB
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB.0,
                        ..Default::default()
                    },
                    ..Default::default()
                };

                let mut raw_pixels = vec![0u8; (width * height * 4) as usize];
                let get_res = GetDIBits(
                    hdc_mem,
                    hbm,
                    0,
                    height,
                    Some(raw_pixels.as_mut_ptr() as *mut _),
                    &mut bi,
                    DIB_RGB_COLORS,
                );

                let _ = SelectObject(hdc_mem, old_obj);
                let _ = DeleteObject(hbm);
                let _ = DeleteDC(hdc_mem);
                let _ = ReleaseDC(HWND(std::ptr::null_mut()), hdc_screen);

                if get_res == 0 {
                    return Err(ActionError::Failed("GetDIBits failed".into()));
                }

                // Convert BGRA to RGB for image encoding
                let mut rgb_pixels = Vec::with_capacity((width * height * 3) as usize);
                for chunk in raw_pixels.as_chunks::<4>().0 {
                    rgb_pixels.push(chunk[2]); // R
                    rgb_pixels.push(chunk[1]); // G
                    rgb_pixels.push(chunk[0]); // B
                }

                let img = image::RgbImage::from_raw(width, height, rgb_pixels)
                    .ok_or_else(|| ActionError::Failed("Failed to construct RgbImage".into()))?;

                // Resize down if width > 1280 to save model latency & tokens
                let final_img = if width > 1280 {
                    let new_width = 1280;
                    let new_height = ((height as f32 * 1280.0) / width as f32) as u32;
                    image::imageops::resize(
                        &img,
                        new_width,
                        new_height,
                        image::imageops::FilterType::Triangle,
                    )
                } else {
                    img
                };

                let mut jpeg_bytes = Vec::new();
                let mut cursor = std::io::Cursor::new(&mut jpeg_bytes);
                final_img
                    .write_to(&mut cursor, image::ImageFormat::Jpeg)
                    .map_err(|e| ActionError::Failed(format!("JPEG encoding failed: {e}")))?;

                Ok(ScreenCapture {
                    width: final_img.width(),
                    height: final_img.height(),
                    jpeg_bytes,
                })
            }
        }

        fn send_mouse_input(
            flags: MOUSE_EVENT_FLAGS,
            dx: i32,
            dy: i32,
            data: u32,
        ) -> Result<(), ActionError> {
            let input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx,
                        dy,
                        mouseData: data,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            unsafe {
                let sent = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
                if sent == 0 {
                    return Err(ActionError::Failed("SendInput mouse failed".into()));
                }
            }
            Ok(())
        }

        fn send_key_event(vk: VIRTUAL_KEY, keyup: bool) -> Result<(), ActionError> {
            let mut flags = KEYBD_EVENT_FLAGS(0);
            if keyup {
                flags |= KEYEVENTF_KEYUP;
            }
            let input = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: vk,
                        wScan: 0,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            unsafe {
                let sent = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
                if sent == 0 {
                    return Err(ActionError::Failed("SendInput keyboard failed".into()));
                }
            }
            Ok(())
        }

        fn key_name_to_vk(key_name: &str) -> Option<VIRTUAL_KEY> {
            match key_name.to_ascii_lowercase().as_str() {
                "enter" | "return" => Some(VK_RETURN),
                "tab" => Some(VK_TAB),
                "escape" | "esc" => Some(VK_ESCAPE),
                "backspace" => Some(VK_BACK),
                "delete" => Some(VK_DELETE),
                "control" | "ctrl" => Some(VK_CONTROL),
                "shift" => Some(VK_SHIFT),
                "alt" => Some(VK_MENU),
                "win" | "meta" | "windows" => Some(VK_LWIN),
                "up" => Some(VK_UP),
                "down" => Some(VK_DOWN),
                "left" => Some(VK_LEFT),
                "right" => Some(VK_RIGHT),
                "space" => Some(VK_SPACE),
                "home" => Some(VK_HOME),
                "end" => Some(VK_END),
                "pageup" => Some(VK_PRIOR),
                "pagedown" => Some(VK_NEXT),
                s if s.len() == 1 => {
                    let c = s.chars().next().unwrap();
                    if c.is_ascii_alphanumeric() {
                        Some(VIRTUAL_KEY(c.to_ascii_uppercase() as u16))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
    }

    #[async_trait]
    impl ComputerUseExecutor for WindowsComputerUseExecutor {
        async fn capture_screen(&self) -> Result<ScreenCapture, ActionError> {
            Self::capture_screen_raw()
        }

        async fn execute_action(&self, action: &ComputerAction) -> Result<(), ActionError> {
            let (screen_w, screen_h) = Self::get_system_screen_size();

            if self.dry_run {
                info!("[DRY-RUN] ComputerUse action: {:?}", action);
                return Ok(());
            }

            match action {
                ComputerAction::Move { x, y } => {
                    let (px, py) = map_normalized_to_physical(*x, *y, screen_w, screen_h);
                    let abs_x = ((px as u64 * 65535) / screen_w.max(1) as u64) as i32;
                    let abs_y = ((py as u64 * 65535) / screen_h.max(1) as u64) as i32;
                    Self::send_mouse_input(
                        MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE,
                        abs_x,
                        abs_y,
                        0,
                    )?;
                }
                ComputerAction::Click { x, y, button } => {
                    let (px, py) = map_normalized_to_physical(*x, *y, screen_w, screen_h);
                    let abs_x = ((px as u64 * 65535) / screen_w.max(1) as u64) as i32;
                    let abs_y = ((py as u64 * 65535) / screen_h.max(1) as u64) as i32;
                    Self::send_mouse_input(
                        MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE,
                        abs_x,
                        abs_y,
                        0,
                    )?;
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

                    match button {
                        MouseButton::Left => {
                            Self::send_mouse_input(MOUSEEVENTF_LEFTDOWN, abs_x, abs_y, 0)?;
                            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                            Self::send_mouse_input(MOUSEEVENTF_LEFTUP, abs_x, abs_y, 0)?;
                        }
                        MouseButton::Right => {
                            Self::send_mouse_input(MOUSEEVENTF_RIGHTDOWN, abs_x, abs_y, 0)?;
                            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                            Self::send_mouse_input(MOUSEEVENTF_RIGHTUP, abs_x, abs_y, 0)?;
                        }
                        MouseButton::Middle => {
                            Self::send_mouse_input(MOUSEEVENTF_MIDDLEDOWN, abs_x, abs_y, 0)?;
                            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                            Self::send_mouse_input(MOUSEEVENTF_MIDDLEUP, abs_x, abs_y, 0)?;
                        }
                    }
                }
                ComputerAction::DoubleClick { x, y } => {
                    let (px, py) = map_normalized_to_physical(*x, *y, screen_w, screen_h);
                    let abs_x = ((px as u64 * 65535) / screen_w.max(1) as u64) as i32;
                    let abs_y = ((py as u64 * 65535) / screen_h.max(1) as u64) as i32;
                    Self::send_mouse_input(
                        MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE,
                        abs_x,
                        abs_y,
                        0,
                    )?;
                    Self::send_mouse_input(MOUSEEVENTF_LEFTDOWN, abs_x, abs_y, 0)?;
                    Self::send_mouse_input(MOUSEEVENTF_LEFTUP, abs_x, abs_y, 0)?;
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    Self::send_mouse_input(MOUSEEVENTF_LEFTDOWN, abs_x, abs_y, 0)?;
                    Self::send_mouse_input(MOUSEEVENTF_LEFTUP, abs_x, abs_y, 0)?;
                }
                ComputerAction::TripleClick { x, y } => {
                    let (px, py) = map_normalized_to_physical(*x, *y, screen_w, screen_h);
                    let abs_x = ((px as u64 * 65535) / screen_w.max(1) as u64) as i32;
                    let abs_y = ((py as u64 * 65535) / screen_h.max(1) as u64) as i32;
                    Self::send_mouse_input(
                        MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE,
                        abs_x,
                        abs_y,
                        0,
                    )?;
                    for _ in 0..3 {
                        Self::send_mouse_input(MOUSEEVENTF_LEFTDOWN, abs_x, abs_y, 0)?;
                        Self::send_mouse_input(MOUSEEVENTF_LEFTUP, abs_x, abs_y, 0)?;
                        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
                    }
                }
                ComputerAction::RightClick { x, y } => {
                    let (px, py) = map_normalized_to_physical(*x, *y, screen_w, screen_h);
                    let abs_x = ((px as u64 * 65535) / screen_w.max(1) as u64) as i32;
                    let abs_y = ((py as u64 * 65535) / screen_h.max(1) as u64) as i32;
                    Self::send_mouse_input(
                        MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE,
                        abs_x,
                        abs_y,
                        0,
                    )?;
                    Self::send_mouse_input(MOUSEEVENTF_RIGHTDOWN, abs_x, abs_y, 0)?;
                    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                    Self::send_mouse_input(MOUSEEVENTF_RIGHTUP, abs_x, abs_y, 0)?;
                }
                ComputerAction::Drag {
                    start_x,
                    start_y,
                    end_x,
                    end_y,
                } => {
                    let (sx, sy) =
                        map_normalized_to_physical(*start_x, *start_y, screen_w, screen_h);
                    let (ex, ey) = map_normalized_to_physical(*end_x, *end_y, screen_w, screen_h);

                    let abs_sx = ((sx as u64 * 65535) / screen_w.max(1) as u64) as i32;
                    let abs_sy = ((sy as u64 * 65535) / screen_h.max(1) as u64) as i32;
                    let abs_ex = ((ex as u64 * 65535) / screen_w.max(1) as u64) as i32;
                    let abs_ey = ((ey as u64 * 65535) / screen_h.max(1) as u64) as i32;

                    Self::send_mouse_input(
                        MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE,
                        abs_sx,
                        abs_sy,
                        0,
                    )?;
                    Self::send_mouse_input(MOUSEEVENTF_LEFTDOWN, abs_sx, abs_sy, 0)?;
                    self.held_inputs.lock().unwrap().mouse_left_down = true;

                    // Interpolate drag in small increments
                    let steps = 10;
                    for step in 1..=steps {
                        let cur_x = abs_sx + ((abs_ex - abs_sx) * step) / steps;
                        let cur_y = abs_sy + ((abs_ey - abs_sy) * step) / steps;
                        Self::send_mouse_input(
                            MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE,
                            cur_x,
                            cur_y,
                            0,
                        )?;
                        tokio::time::sleep(std::time::Duration::from_millis(15)).await;
                    }

                    Self::send_mouse_input(MOUSEEVENTF_LEFTUP, abs_ex, abs_ey, 0)?;
                    self.held_inputs.lock().unwrap().mouse_left_down = false;
                }
                ComputerAction::Scroll { x, y, delta_y, .. } => {
                    let (px, py) = map_normalized_to_physical(*x, *y, screen_w, screen_h);
                    let abs_x = ((px as u64 * 65535) / screen_w.max(1) as u64) as i32;
                    let abs_y = ((py as u64 * 65535) / screen_h.max(1) as u64) as i32;
                    Self::send_mouse_input(
                        MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE,
                        abs_x,
                        abs_y,
                        0,
                    )?;
                    let wheel_amount = (delta_y * 120) as u32;
                    Self::send_mouse_input(MOUSEEVENTF_WHEEL, 0, 0, wheel_amount)?;
                }
                ComputerAction::TypeText { text, press_enter } => {
                    // Type text using KEYEVENTF_UNICODE
                    for c in text.chars() {
                        let mut utf16_buf = [0u16; 2];
                        let encoded = c.encode_utf16(&mut utf16_buf);
                        for &u16_char in encoded.iter() {
                            let down_input = INPUT {
                                r#type: INPUT_KEYBOARD,
                                Anonymous: INPUT_0 {
                                    ki: KEYBDINPUT {
                                        wVk: VIRTUAL_KEY(0),
                                        wScan: u16_char,
                                        dwFlags: KEYEVENTF_UNICODE,
                                        time: 0,
                                        dwExtraInfo: 0,
                                    },
                                },
                            };
                            let up_input = INPUT {
                                r#type: INPUT_KEYBOARD,
                                Anonymous: INPUT_0 {
                                    ki: KEYBDINPUT {
                                        wVk: VIRTUAL_KEY(0),
                                        wScan: u16_char,
                                        dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                                        time: 0,
                                        dwExtraInfo: 0,
                                    },
                                },
                            };
                            unsafe {
                                let _ = SendInput(
                                    &[down_input, up_input],
                                    std::mem::size_of::<INPUT>() as i32,
                                );
                            }
                        }
                    }

                    if *press_enter {
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                        Self::send_key_event(VK_RETURN, false)?;
                        Self::send_key_event(VK_RETURN, true)?;
                    }
                }
                ComputerAction::PressKey { key } => {
                    if let Some(vk) = Self::key_name_to_vk(key) {
                        Self::send_key_event(vk, false)?;
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                        Self::send_key_event(vk, true)?;
                    } else {
                        warn!("Unrecognized key: {}", key);
                    }
                }
                ComputerAction::KeyDown { key } => {
                    if let Some(vk) = Self::key_name_to_vk(key) {
                        Self::send_key_event(vk, false)?;
                        self.held_inputs.lock().unwrap().held_keys.insert(vk.0);
                    }
                }
                ComputerAction::KeyUp { key } => {
                    if let Some(vk) = Self::key_name_to_vk(key) {
                        Self::send_key_event(vk, true)?;
                        self.held_inputs.lock().unwrap().held_keys.remove(&vk.0);
                    }
                }
                ComputerAction::Hotkey { keys } => {
                    let mut pressed = Vec::new();
                    for k in keys {
                        if let Some(vk) = Self::key_name_to_vk(k) {
                            Self::send_key_event(vk, false)?;
                            pressed.push(vk);
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                    for vk in pressed.into_iter().rev() {
                        Self::send_key_event(vk, true)?;
                    }
                }
                ComputerAction::Wait { ms } => {
                    tokio::time::sleep(std::time::Duration::from_millis(*ms)).await;
                }
                ComputerAction::Finish { .. } | ComputerAction::Fail { .. } => {}
            }

            Ok(())
        }

        async fn release_all_held_inputs(&self) -> Result<(), ActionError> {
            let mut held = self.held_inputs.lock().unwrap();

            if held.mouse_left_down {
                let _ = Self::send_mouse_input(MOUSEEVENTF_LEFTUP, 0, 0, 0);
                held.mouse_left_down = false;
            }
            if held.mouse_right_down {
                let _ = Self::send_mouse_input(MOUSEEVENTF_RIGHTUP, 0, 0, 0);
                held.mouse_right_down = false;
            }
            if held.mouse_middle_down {
                let _ = Self::send_mouse_input(MOUSEEVENTF_MIDDLEUP, 0, 0, 0);
                held.mouse_middle_down = false;
            }

            for vk_code in held.held_keys.drain() {
                let _ = Self::send_key_event(VIRTUAL_KEY(vk_code), true);
            }

            info!("All held mouse buttons and keys safely released.");
            Ok(())
        }

        fn screen_dimensions(&self) -> (u32, u32) {
            Self::get_system_screen_size()
        }

        fn is_dry_run(&self) -> bool {
            self.dry_run
        }
    }
}

#[cfg(windows)]
pub use windows_native::WindowsComputerUseExecutor;

/// Mock Computer Use Executor for offline and hermetic testing.
pub struct MockComputerUseExecutor {
    pub screen_width: u32,
    pub screen_height: u32,
    pub dry_run: bool,
    pub recorded_actions: Arc<Mutex<Vec<ComputerAction>>>,
    pub held_inputs: Arc<Mutex<HeldInputs>>,
    pub canned_capture: Arc<Mutex<Option<ScreenCapture>>>,
}

impl Default for MockComputerUseExecutor {
    fn default() -> Self {
        Self {
            screen_width: 1920,
            screen_height: 1080,
            dry_run: false,
            recorded_actions: Arc::new(Mutex::new(Vec::new())),
            held_inputs: Arc::new(Mutex::new(HeldInputs::default())),
            canned_capture: Arc::new(Mutex::new(None)),
        }
    }
}

impl MockComputerUseExecutor {
    pub fn with_canned_capture(capture: ScreenCapture) -> Self {
        Self {
            canned_capture: Arc::new(Mutex::new(Some(capture))),
            ..Default::default()
        }
    }

    pub fn get_recorded_actions(&self) -> Vec<ComputerAction> {
        self.recorded_actions.lock().unwrap().clone()
    }
}

#[async_trait]
impl ComputerUseExecutor for MockComputerUseExecutor {
    async fn capture_screen(&self) -> Result<ScreenCapture, ActionError> {
        if let Some(canned) = self.canned_capture.lock().unwrap().clone() {
            return Ok(canned);
        }
        // Minimal valid JPEG header for tests
        Ok(ScreenCapture {
            width: self.screen_width,
            height: self.screen_height,
            jpeg_bytes: vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46],
        })
    }

    async fn execute_action(&self, action: &ComputerAction) -> Result<(), ActionError> {
        self.recorded_actions.lock().unwrap().push(action.clone());

        match action {
            ComputerAction::KeyDown { key } => {
                self.held_inputs
                    .lock()
                    .unwrap()
                    .held_keys
                    .insert(key.len() as u16);
            }
            ComputerAction::KeyUp { key } => {
                self.held_inputs
                    .lock()
                    .unwrap()
                    .held_keys
                    .remove(&(key.len() as u16));
            }
            ComputerAction::Drag { .. } => {
                // Simulates drag holding and releasing left button
            }
            _ => {}
        }

        Ok(())
    }

    async fn release_all_held_inputs(&self) -> Result<(), ActionError> {
        let mut held = self.held_inputs.lock().unwrap();
        held.mouse_left_down = false;
        held.mouse_right_down = false;
        held.mouse_middle_down = false;
        held.held_keys.clear();
        Ok(())
    }

    fn screen_dimensions(&self) -> (u32, u32) {
        (self.screen_width, self.screen_height)
    }

    fn is_dry_run(&self) -> bool {
        self.dry_run
    }
}
