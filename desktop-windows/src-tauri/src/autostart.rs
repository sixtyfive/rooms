//! "Start with Windows" through the per-user Run key (no admin rights needed).

use std::io::ErrorKind;
use winreg::{enums::*, RegKey};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Task Manager's Startup tab records its enable/disable switch here.
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const NAME: &str = "HaRooms";

fn command() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(format!("\"{}\"", exe.display()))
}

fn ignore_not_found(r: std::io::Result<()>) -> Result<(), String> {
    match r {
        Err(e) if e.kind() != ErrorKind::NotFound => Err(e.to_string()),
        _ => Ok(()),
    }
}

/// True if the Run entry exists and Task Manager has not switched it off.
pub fn is_enabled() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let registered = hkcu
        .open_subkey(RUN)
        .and_then(|k| k.get_value::<String, _>(NAME))
        .is_ok();
    if !registered {
        return false;
    }
    // StartupApproved value: first byte even (02/06) = enabled, odd (03) = disabled.
    let disabled = hkcu
        .open_subkey(APPROVED)
        .and_then(|k| k.get_raw_value(NAME))
        .map(|v| v.bytes.first().is_some_and(|b| b & 1 == 1))
        .unwrap_or(false);
    !disabled
}

pub fn set(enabled: bool) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if enabled {
        let (run, _) = hkcu.create_subkey(RUN).map_err(|e| e.to_string())?;
        run.set_value(NAME, &command()?).map_err(|e| e.to_string())?;
    } else {
        let run = hkcu
            .open_subkey_with_flags(RUN, KEY_SET_VALUE)
            .map_err(|e| e.to_string());
        if let Ok(run) = run {
            ignore_not_found(run.delete_value(NAME))?;
        }
    }
    // Drop any Task Manager override so our choice is what actually applies.
    if let Ok(approved) = hkcu.open_subkey_with_flags(APPROVED, KEY_SET_VALUE) {
        let _ = approved.delete_value(NAME);
    }
    Ok(())
}

/// If autostart is on but points at a moved/updated exe, repoint it.
pub fn refresh_path() {
    let Ok(cmd) = command() else { return };
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(run) = hkcu.open_subkey_with_flags(RUN, KEY_READ | KEY_SET_VALUE) else { return };
    if let Ok(current) = run.get_value::<String, _>(NAME) {
        if current != cmd {
            let _ = run.set_value(NAME, &cmd);
        }
    }
}
