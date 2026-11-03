//! Config persistence on Windows: everything except the token lives in
//! `%APPDATA%\ha-rooms\config.json` (via ha-core); the token lives in Credential Manager.

use ha_core::config::{config_path, load_config, save_config, Config};
use keyring::{Entry, Error};

const SERVICE: &str = "dev.local.HaRooms";
const USER: &str = "access-token";

fn entry() -> Result<Entry, String> {
    Entry::new(SERVICE, USER).map_err(|e| format!("Credential Manager: {e}"))
}

pub fn read_token() -> Result<String, String> {
    match entry()?.get_password() {
        Ok(t) => Ok(t.trim().to_string()),
        Err(Error::NoEntry) => Ok(String::new()),
        Err(e) => Err(format!("Credential Manager: {e}")),
    }
}

pub fn write_token(token: &str) -> Result<(), String> {
    let entry = entry()?;
    let token = token.trim();
    let result = if token.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(e) => Err(e),
        }
    } else {
        entry.set_password(token)
    };
    result.map_err(|e| format!("Credential Manager: {e}"))
}

/// Loads the file config and fills in the token from Credential Manager.
/// A token found in the file (hand-edited, or copied from another platform) is moved
/// into Credential Manager and scrubbed from the file.
pub fn load() -> Result<Config, String> {
    let mut cfg = load_config()?;
    if cfg.token.is_empty() {
        cfg.token = read_token()?;
    } else {
        write_token(&cfg.token)?;
        let mut scrubbed = cfg.clone();
        scrubbed.token.clear();
        save_config(&scrubbed)?;
    }
    Ok(cfg)
}

pub fn save(cfg: &Config) -> Result<(), String> {
    write_token(&cfg.token)?;
    let mut file = cfg.clone();
    file.token.clear();
    save_config(&file)
}

fn default_config() -> Config {
    Config {
        url: "http://homeassistant.local:8123".into(),
        token: String::new(),
        switch_label: "panel".into(),
        light_label: String::new(),
        climate_label: String::new(),
    }
}

/// Same contract as `ha_core::config::init_config`, but token-aware:
/// returns (config, needs_setup, error to show).
pub fn init() -> (Config, bool, Option<String>) {
    let mut error = None;
    if !config_path().map(|p| p.exists()).unwrap_or(false) {
        error = save_config(&default_config()).err();
    }
    match load() {
        Ok(c) => {
            let needs_setup = !c.is_complete();
            (c, needs_setup, error)
        }
        Err(e) => (default_config(), true, Some(e)),
    }
}
