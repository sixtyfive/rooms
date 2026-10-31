use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Config {
    pub url: String,
    pub token: String,
    #[serde(default)]
    pub switch_label: String,
    #[serde(default)]
    pub light_label: String,
}

impl Config {
    pub fn is_complete(&self) -> bool {
        !self.url.trim().is_empty() && !self.token.trim().is_empty()
    }
}

pub fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

pub fn config_path() -> Result<PathBuf, String> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .or_else(|_| std::env::var("HOME").map(|h| format!("{h}/.config")))
        .map_err(err)?;
    Ok(PathBuf::from(base).join("ha-rooms/config.json"))
}

pub fn load_config() -> Result<Config, String> {
    let path = config_path()?;
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut c: Config = serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", path.display()))?;
    c.url = c.url.trim().to_string();
    c.token = c.token.trim().to_string();
    c.light_label = c.light_label.trim().to_string();
    c.switch_label = c.switch_label.trim().to_string();
    Ok(c)
}

pub fn save_config(cfg: &Config) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let path = config_path()?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(err)?;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(err)?;
    f.write_all(serde_json::to_string_pretty(cfg).map_err(err)?.as_bytes())
        .map_err(err)
}

pub fn init_config() -> (Config, bool, Option<String>) {
    let default = Config {
        url: "http://homeassistant.local:8123".into(),
        token: String::new(),
        switch_label: "panel".into(),
        light_label: String::new(),
    };
    match load_config() {
        Ok(c) => {
            let needs_setup = !c.is_complete();
            (c, needs_setup, None)
        }
        Err(_) if !config_path().map(|p| p.exists()).unwrap_or(false) => {
            let e = save_config(&default).err();
            (default, true, e)
        }
        Err(e) => (default, true, Some(e)),
    }
}
