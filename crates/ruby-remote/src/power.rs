//! Windows power, battery, and system uptime inspection via Win32 API.

#[derive(Debug, Clone)]
pub struct SystemPowerSummary {
    pub is_ac_plugged: bool,
    pub battery_life_percent: Option<u8>,
    pub battery_status_desc: String,
    pub uptime_desc: String,
}

/// Retrieves the current power and uptime status of the Windows host.
pub fn get_system_power_status() -> SystemPowerSummary {
    #[cfg(windows)]
    {
        use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
        use windows::Win32::System::SystemInformation::GetTickCount64;

        let mut status = SYSTEM_POWER_STATUS::default();
        let ac_online = unsafe {
            if GetSystemPowerStatus(&mut status).is_ok() {
                status.ACLineStatus == 1
            } else {
                true // default assumption
            }
        };

        let battery_percent = if status.BatteryLifePercent <= 100 {
            Some(status.BatteryLifePercent)
        } else {
            None
        };

        let battery_desc = match battery_percent {
            Some(pct) => {
                if ac_online {
                    format!("{}% (Charging / AC)", pct)
                } else {
                    format!("{}% (On Battery)", pct)
                }
            }
            None => {
                if ac_online {
                    "AC Power (No Battery / Desktop)".to_string()
                } else {
                    "Battery Status Unknown".to_string()
                }
            }
        };

        let uptime_ms = unsafe { GetTickCount64() };
        let uptime_secs = uptime_ms / 1000;
        let hours = uptime_secs / 3600;
        let mins = (uptime_secs % 3600) / 60;
        let uptime_desc = format!("{}h {}m", hours, mins);

        SystemPowerSummary {
            is_ac_plugged: ac_online,
            battery_life_percent: battery_percent,
            battery_status_desc: battery_desc,
            uptime_desc,
        }
    }

    #[cfg(not(windows))]
    {
        SystemPowerSummary {
            is_ac_plugged: true,
            battery_life_percent: Some(100),
            battery_status_desc: "100% (AC)".to_string(),
            uptime_desc: "Unknown".to_string(),
        }
    }
}
