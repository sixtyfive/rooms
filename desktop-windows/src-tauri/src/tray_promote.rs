//! Windows 11 puts every new tray icon into the overflow flyout. The per-icon "show in
//! taskbar corner" switch is `IsPromoted` under HKCU\Control Panel\NotifyIconSettings\<id>;
//! Explorer creates the subkey once the icon is registered. There is no official API for this,
//! so this is best-effort, runs once per exe path, and never touches the value again
//! (a user who hides the icon afterwards keeps their choice).

use std::time::Duration;
use winreg::{enums::*, RegKey};

const SETTINGS: &str = r"Control Panel\NotifyIconSettings";
const MARKER_KEY: &str = r"Software\dev.local.HaRooms";
const MARKER_VALUE: &str = "TrayPromotedFor";

pub fn spawn() {
    std::thread::spawn(run);
}

/// Explorer stores some paths with a known-folder GUID prefix
/// (`{7C5A40EF-…}\Kaseya\x.exe` instead of `C:\Program Files (x86)\Kaseya\x.exe`).
fn same_exe(stored: &str, exe: &str) -> bool {
    if stored.eq_ignore_ascii_case(exe) {
        return true;
    }
    if stored.starts_with('{') {
        if let Some((_, rest)) = stored.split_once('\\') {
            let suffix = format!("\\{}", rest.to_ascii_lowercase());
            return exe.to_ascii_lowercase().ends_with(&suffix);
        }
    }
    false
}

fn run() {
    let Ok(exe) = std::env::current_exe() else { return };
    let exe = exe.to_string_lossy().into_owned();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    let done = hkcu
        .open_subkey(MARKER_KEY)
        .and_then(|k| k.get_value::<String, _>(MARKER_VALUE))
        .is_ok_and(|v| v.eq_ignore_ascii_case(&exe));
    if done {
        return;
    }

    // Explorer needs a moment after the icon is added; poll for up to 30 s.
    for _ in 0..60 {
        let Ok(root) = hkcu.open_subkey(SETTINGS) else { return }; // not a Windows with this store
        let mut found = false;
        for name in root.enum_keys().flatten() {
            let Ok(key) = root.open_subkey_with_flags(&name, KEY_READ | KEY_SET_VALUE) else { continue };
            let Ok(path) = key.get_value::<String, _>("ExecutablePath") else { continue };
            if !same_exe(&path, &exe) {
                continue;
            }
            found = true;
            if key.get_value::<u32, _>("IsPromoted").unwrap_or(0) == 0 {
                let _ = key.set_value("IsPromoted", &1u32);
            }
        }
        if found {
            if let Ok((m, _)) = hkcu.create_subkey(MARKER_KEY) {
                let _ = m.set_value(MARKER_VALUE, &exe);
            }
            return;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[cfg(test)]
mod tests {
    use super::same_exe;

    #[test]
    fn matches_plain_and_guid_prefixed_paths() {
        let exe = r"C:\Users\me\AppData\Local\HA Rooms\ha-rooms.exe";
        assert!(same_exe(exe, exe));
        assert!(same_exe(&exe.to_uppercase(), exe));
        assert!(same_exe(r"{F1B32785-6FBA-4FCF-9D55-7B8E7F157091}\HA Rooms\ha-rooms.exe", exe));
        assert!(!same_exe(r"{F1B32785-6FBA-4FCF-9D55-7B8E7F157091}\Other\ha-rooms.exe", exe));
        assert!(!same_exe(r"C:\Other\ha-rooms.exe", exe));
    }
}
