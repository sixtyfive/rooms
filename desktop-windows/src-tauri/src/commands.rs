use crate::{autostart, popup, store, tray, view};
use ha_core::config::Config;
use ha_core::model::{parse_temperature, step_temperature, Data};
use ha_core::client;
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

#[derive(Default)]
pub struct AppState(Mutex<Inner>);

#[derive(Default)]
struct Inner {
    data: Data,
    error: Option<String>,
}

fn emit_view(app: &AppHandle) {
    let v = {
        let state = app.state::<AppState>();
        let g = state.0.lock().unwrap();
        view::build(&g.data, g.error.as_deref())
    };
    let _ = app.emit_to(popup::LABEL, "view", v);
}

pub fn set_error(app: &AppHandle, error: String) {
    app.state::<AppState>().0.lock().unwrap().error = Some(error);
}

/// Result of a network round trip: the latest data wins, errors keep the stale data on screen.
fn loaded(app: &AppHandle, r: Result<Data, String>) {
    {
        let state = app.state::<AppState>();
        let mut g = state.0.lock().unwrap();
        match r {
            Ok(d) => {
                g.data = d;
                g.error = None;
            }
            Err(e) => g.error = Some(e),
        }
    }
    emit_view(app);
}

async fn fetch_all() -> Result<Data, String> {
    client::fetch_all(&store::load()?).await
}

async fn set_state(id: String, on: bool) -> Result<Data, String> {
    client::set_state(&store::load()?, &id, on).await
}

async fn set_temperature(id: String, t: f64) -> Result<Data, String> {
    client::set_temperature(&store::load()?, &id, t).await
}

// ---- popup ----------------------------------------------------------------------------------

#[tauri::command]
pub fn refresh(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let r = fetch_all().await;
        loaded(&app, r);
    });
}

#[tauri::command]
pub fn toggle(app: AppHandle, id: String) {
    let on = {
        let state = app.state::<AppState>();
        let mut g = state.0.lock().unwrap();
        let Some(e) = g.data.find_mut(&id) else { return };
        let on = !e.is_on();
        e.state = if on { "on".into() } else { "off".into() };
        on
    };
    emit_view(&app); // optimistic
    tauri::async_runtime::spawn(async move {
        let r = set_state(id, on).await;
        loaded(&app, r);
    });
}

fn apply_temperature(app: &AppHandle, id: String, t: f64) {
    let t = {
        let state = app.state::<AppState>();
        let mut g = state.0.lock().unwrap();
        let Some(e) = g.data.find_mut(&id) else { return };
        let t = e.clamp_temperature(t);
        e.target = Some(t);
        t
    };
    emit_view(app); // optimistic
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let r = set_temperature(id, t).await;
        loaded(&app, r);
    });
}

#[tauri::command]
pub fn temp_submit(app: AppHandle, id: String, text: String) {
    match parse_temperature(&text) {
        Some(t) => apply_temperature(&app, id, t),
        None => emit_view(&app), // invalid input is discarded: restore the field
    }
}

#[tauri::command]
pub fn temp_step(app: AppHandle, id: String, dir: i32) {
    let current = {
        let state = app.state::<AppState>();
        let mut g = state.0.lock().unwrap();
        g.data.find_mut(&id).and_then(|e| e.target)
    };
    match current {
        Some(t) => apply_temperature(&app, id, step_temperature(t, dir)),
        None => emit_view(&app),
    }
}

#[tauri::command]
pub fn hide_popup(app: AppHandle) {
    popup::hide(&app);
}

#[tauri::command]
pub fn set_popup_height(app: AppHandle, height: f64) {
    popup::set_height(&app, height);
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    show_settings(&app);
}

/// Window creation must not happen on the main thread from a command/event handler on Windows
/// (it deadlocks), hence the spawn.
pub fn show_settings(app: &AppHandle) {
    popup::hide(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Some(w) = app.get_webview_window("settings") {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
            return;
        }
        let built = WebviewWindowBuilder::new(&app, "settings", WebviewUrl::App("settings.html".into()))
            .title("HomeAssistant Applet Settings")
            .inner_size(620.0, 600.0)
            .min_inner_size(520.0, 520.0)
            .center()
            .build();
        if let Err(e) = built {
            eprintln!("could not open settings window: {e}");
        }
    });
}

// ---- settings -------------------------------------------------------------------------------

#[derive(Serialize)]
pub struct Settings {
    config: Config,
    error: Option<String>,
    autostart: bool,
}

#[tauri::command]
pub fn get_settings() -> Settings {
    let (config, _, error) = store::init();
    Settings { config, error, autostart: autostart::is_enabled() }
}

/// Instant-apply: called on every edit. Returns an error message, or null.
#[tauri::command]
pub fn save_settings(form: Config) -> Option<String> {
    store::save(&form).err()
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let r = autostart::set(enabled);
    tray::sync_autostart(&app);
    r?;
    Ok(autostart::is_enabled())
}
