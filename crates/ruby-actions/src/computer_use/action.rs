//! Computer Use Action definitions, mouse/key representations, and coordinate normalization.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ComputerAction {
    Click {
        x: u32,
        y: u32,
        #[serde(default = "default_left_button")]
        button: MouseButton,
    },
    DoubleClick {
        x: u32,
        y: u32,
    },
    TripleClick {
        x: u32,
        y: u32,
    },
    RightClick {
        x: u32,
        y: u32,
    },
    Move {
        x: u32,
        y: u32,
    },
    Drag {
        start_x: u32,
        start_y: u32,
        end_x: u32,
        end_y: u32,
    },
    Scroll {
        x: u32,
        y: u32,
        #[serde(default)]
        delta_x: i32,
        #[serde(default)]
        delta_y: i32,
    },
    TypeText {
        text: String,
        #[serde(default)]
        press_enter: bool,
    },
    PressKey {
        key: String,
    },
    KeyDown {
        key: String,
    },
    KeyUp {
        key: String,
    },
    Hotkey {
        keys: Vec<String>,
    },
    Wait {
        ms: u64,
    },
    Finish {
        result: Option<String>,
    },
    Fail {
        reason: String,
    },
}

fn default_left_button() -> MouseButton {
    MouseButton::Left
}

impl ComputerAction {
    pub fn action_type_str(&self) -> &'static str {
        match self {
            ComputerAction::Click { .. } => "click",
            ComputerAction::DoubleClick { .. } => "double_click",
            ComputerAction::TripleClick { .. } => "triple_click",
            ComputerAction::RightClick { .. } => "right_click",
            ComputerAction::Move { .. } => "move",
            ComputerAction::Drag { .. } => "drag",
            ComputerAction::Scroll { .. } => "scroll",
            ComputerAction::TypeText { .. } => "type_text",
            ComputerAction::PressKey { .. } => "press_key",
            ComputerAction::KeyDown { .. } => "key_down",
            ComputerAction::KeyUp { .. } => "key_up",
            ComputerAction::Hotkey { .. } => "hotkey",
            ComputerAction::Wait { .. } => "wait",
            ComputerAction::Finish { .. } => "finish",
            ComputerAction::Fail { .. } => "fail",
        }
    }

    pub fn coordinates(&self) -> Option<(u32, u32)> {
        match self {
            ComputerAction::Click { x, y, .. }
            | ComputerAction::DoubleClick { x, y }
            | ComputerAction::TripleClick { x, y }
            | ComputerAction::RightClick { x, y }
            | ComputerAction::Move { x, y }
            | ComputerAction::Scroll { x, y, .. } => Some((*x, *y)),
            ComputerAction::Drag {
                start_x, start_y, ..
            } => Some((*start_x, *start_y)),
            _ => None,
        }
    }
}

/// Normalizes a [0, 1000] virtual coordinate space point to physical screen pixels.
pub fn map_normalized_to_physical(
    norm_x: u32,
    norm_y: u32,
    screen_width: u32,
    screen_height: u32,
) -> (u32, u32) {
    let phys_x = ((norm_x as u64 * screen_width as u64) / 1000)
        .min(screen_width.saturating_sub(1) as u64) as u32;
    let phys_y = ((norm_y as u64 * screen_height as u64) / 1000)
        .min(screen_height.saturating_sub(1) as u64) as u32;
    (phys_x, phys_y)
}

/// Normalizes physical screen pixels into [0, 1000] space.
pub fn map_physical_to_normalized(
    phys_x: u32,
    phys_y: u32,
    screen_width: u32,
    screen_height: u32,
) -> (u32, u32) {
    if screen_width == 0 || screen_height == 0 {
        return (0, 0);
    }
    let norm_x = ((phys_x as u64 * 1000) / screen_width as u64).min(1000) as u32;
    let norm_y = ((phys_y as u64 * 1000) / screen_height as u64).min(1000) as u32;
    (norm_x, norm_y)
}

/// Physical and virtual monitor geometry for multi-monitor desktop setups.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MonitorGeometry {
    pub id: u32,
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64, // e.g. 1.0 (100%), 1.25 (125%), 1.5 (150%), 2.0 (200%)
    pub is_primary: bool,
}

impl Default for MonitorGeometry {
    fn default() -> Self {
        Self {
            id: 0,
            origin_x: 0,
            origin_y: 0,
            width: 1920,
            height: 1080,
            scale_factor: 1.0,
            is_primary: true,
        }
    }
}

/// Maps normalized [0, 1000] monitor coordinates into virtual desktop space (incorporating monitor offset).
pub fn map_normalized_to_virtual(
    norm_x: u32,
    norm_y: u32,
    monitor: &MonitorGeometry,
) -> (i32, i32) {
    let local_x = ((norm_x as u64 * monitor.width as u64) / 1000)
        .min(monitor.width.saturating_sub(1) as u64) as i32;
    let local_y = ((norm_y as u64 * monitor.height as u64) / 1000)
        .min(monitor.height.saturating_sub(1) as u64) as i32;
    (monitor.origin_x + local_x, monitor.origin_y + local_y)
}

/// Maps virtual desktop coordinates back into normalized [0, 1000] coordinate space for a specific monitor.
pub fn map_virtual_to_normalized(
    virt_x: i32,
    virt_y: i32,
    monitor: &MonitorGeometry,
) -> (u32, u32) {
    if monitor.width == 0 || monitor.height == 0 {
        return (0, 0);
    }
    let local_x =
        (virt_x - monitor.origin_x).clamp(0, monitor.width.saturating_sub(1) as i32) as u64;
    let local_y =
        (virt_y - monitor.origin_y).clamp(0, monitor.height.saturating_sub(1) as i32) as u64;
    let norm_x = ((local_x * 1000) / monitor.width as u64).min(1000) as u32;
    let norm_y = ((local_y * 1000) / monitor.height as u64).min(1000) as u32;
    (norm_x, norm_y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coordinate_mapping() {
        let screen_w = 1920;
        let screen_h = 1080;

        // Top-left
        assert_eq!(map_normalized_to_physical(0, 0, screen_w, screen_h), (0, 0));
        assert_eq!(map_physical_to_normalized(0, 0, screen_w, screen_h), (0, 0));

        // Center
        assert_eq!(
            map_normalized_to_physical(500, 500, screen_w, screen_h),
            (960, 540)
        );
        assert_eq!(
            map_physical_to_normalized(960, 540, screen_w, screen_h),
            (500, 500)
        );

        // Bottom-right clamp
        assert_eq!(
            map_normalized_to_physical(1000, 1000, screen_w, screen_h),
            (1919, 1079)
        );
    }

    #[test]
    fn test_action_serialization_and_deserialization() {
        let json = r#"{"action":"click","x":500,"y":250,"button":"Left"}"#;
        let action: ComputerAction = serde_json::from_str(json).unwrap();
        assert_eq!(
            action,
            ComputerAction::Click {
                x: 500,
                y: 250,
                button: MouseButton::Left
            }
        );
        assert_eq!(action.action_type_str(), "click");
        assert_eq!(action.coordinates(), Some((500, 250)));
    }

    #[test]
    fn test_dpi_scaling_100_percent() {
        let monitor = MonitorGeometry {
            id: 0,
            origin_x: 0,
            origin_y: 0,
            width: 1920,
            height: 1080,
            scale_factor: 1.0,
            is_primary: true,
        };
        assert_eq!(map_normalized_to_virtual(0, 0, &monitor), (0, 0));
        assert_eq!(map_normalized_to_virtual(500, 500, &monitor), (960, 540));
        assert_eq!(
            map_normalized_to_virtual(1000, 1000, &monitor),
            (1919, 1079)
        );
        assert_eq!(map_virtual_to_normalized(960, 540, &monitor), (500, 500));
    }

    #[test]
    fn test_dpi_scaling_125_percent() {
        let monitor = MonitorGeometry {
            id: 0,
            origin_x: 0,
            origin_y: 0,
            width: 1920,
            height: 1080,
            scale_factor: 1.25,
            is_primary: true,
        };
        assert_eq!(map_normalized_to_virtual(500, 500, &monitor), (960, 540));
        assert_eq!(map_virtual_to_normalized(960, 540, &monitor), (500, 500));
    }

    #[test]
    fn test_dpi_scaling_150_percent() {
        let monitor = MonitorGeometry {
            id: 1,
            origin_x: 0,
            origin_y: 0,
            width: 2560,
            height: 1440,
            scale_factor: 1.5,
            is_primary: true,
        };
        assert_eq!(map_normalized_to_virtual(500, 500, &monitor), (1280, 720));
        assert_eq!(map_virtual_to_normalized(1280, 720, &monitor), (500, 500));
    }

    #[test]
    fn test_dpi_scaling_200_percent() {
        let monitor = MonitorGeometry {
            id: 0,
            origin_x: 0,
            origin_y: 0,
            width: 3840,
            height: 2160,
            scale_factor: 2.0,
            is_primary: true,
        };
        assert_eq!(map_normalized_to_virtual(500, 500, &monitor), (1920, 1080));
        assert_eq!(map_virtual_to_normalized(1920, 1080, &monitor), (500, 500));
    }

    #[test]
    fn test_multi_monitor_negative_origin_secondary() {
        // Secondary monitor positioned to the left of the primary monitor
        let secondary_monitor = MonitorGeometry {
            id: 2,
            origin_x: -1920,
            origin_y: 0,
            width: 1920,
            height: 1080,
            scale_factor: 1.0,
            is_primary: false,
        };

        // Top-left of secondary monitor
        assert_eq!(
            map_normalized_to_virtual(0, 0, &secondary_monitor),
            (-1920, 0)
        );
        // Center of secondary monitor
        assert_eq!(
            map_normalized_to_virtual(500, 500, &secondary_monitor),
            (-960, 540)
        );
        // Bottom-right of secondary monitor
        assert_eq!(
            map_normalized_to_virtual(1000, 1000, &secondary_monitor),
            (-1, 1079)
        );

        // Roundtrip mapping
        assert_eq!(
            map_virtual_to_normalized(-960, 540, &secondary_monitor),
            (500, 500)
        );
        assert_eq!(
            map_virtual_to_normalized(-1920, 0, &secondary_monitor),
            (0, 0)
        );
    }
}
