//! Battery state from Linux's `/sys/class/power_supply`. The parsing is
//! plain file reading, so it is compiled and tested everywhere.

use std::path::Path;

/// What the power supplies say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PowerState {
    /// Running on battery (no charger connected and a battery discharging).
    pub on_battery: bool,
    /// Charge of the battery, if there is one.
    pub percent: Option<u8>,
}

/// Read every supply under `root` (normally `/sys/class/power_supply`).
/// Missing folders mean a desktop machine: not on battery.
pub fn read_power_supplies(root: &Path) -> PowerState {
    let Ok(entries) = std::fs::read_dir(root) else {
        return PowerState::default();
    };
    let mut mains_online = false;
    let mut discharging = false;
    let mut levels = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        let read = |name: &str| {
            std::fs::read_to_string(dir.join(name))
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        };
        match read("type").as_str() {
            "Mains" | "USB" => {
                if read("online") == "1" {
                    mains_online = true;
                }
            }
            "Battery" => {
                // Peripheral batteries (mice, headsets) report scope Device.
                if read("scope") == "Device" {
                    continue;
                }
                if read("status") == "Discharging" {
                    discharging = true;
                }
                if let Ok(c) = read("capacity").parse::<u8>() {
                    levels.push(c.min(100));
                }
            }
            _ => {}
        }
    }
    PowerState {
        on_battery: discharging && !mains_online,
        percent: levels.into_iter().min(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn supply(root: &Path, name: &str, files: &[(&str, &str)]) {
        let d = root.join(name);
        std::fs::create_dir_all(&d).unwrap();
        for (f, v) in files {
            std::fs::write(d.join(f), format!("{v}\n")).unwrap();
        }
    }

    #[test]
    fn laptop_on_battery() {
        let t = tempfile::tempdir().unwrap();
        supply(t.path(), "AC", &[("type", "Mains"), ("online", "0")]);
        supply(
            t.path(),
            "BAT0",
            &[
                ("type", "Battery"),
                ("status", "Discharging"),
                ("capacity", "42"),
            ],
        );
        supply(
            t.path(),
            "hidpp_battery_0",
            &[
                ("type", "Battery"),
                ("scope", "Device"),
                ("status", "Discharging"),
                ("capacity", "5"),
            ],
        );
        assert_eq!(
            read_power_supplies(t.path()),
            PowerState {
                on_battery: true,
                percent: Some(42)
            }
        );
    }

    #[test]
    fn laptop_charging_and_desktop() {
        let t = tempfile::tempdir().unwrap();
        supply(t.path(), "AC", &[("type", "Mains"), ("online", "1")]);
        supply(
            t.path(),
            "BAT0",
            &[
                ("type", "Battery"),
                ("status", "Charging"),
                ("capacity", "80"),
            ],
        );
        assert!(!read_power_supplies(t.path()).on_battery);
        assert_eq!(
            read_power_supplies(&t.path().join("missing")),
            PowerState::default()
        );
    }
}
