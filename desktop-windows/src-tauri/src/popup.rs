//! The tray popup: positioning, and the show/hide rules Windows does not give us for free.
//!
//! * Close on focus loss: the popup hides itself on `Focused(false)`.
//! * Close on second click: pressing the tray icon steals focus first (-> hide on blur), and the
//!   click event only arrives on mouse-up. Without help the click would immediately re-open the
//!   popup, so a hide caused by blur is remembered and the click that follows it is swallowed.

use crate::placement::{corner_anchor, place, Rect as Px};
use crate::{commands, store, tray};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{
    AppHandle, Emitter, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewWindow,
};

pub const LABEL: &str = "popup";

/// Logical (DIP) sizes; converted with the target monitor's scale factor.
const WIDTH: f64 = 960.0;
const GAP: f64 = 8.0;
const MIN_HEIGHT: f64 = 80.0;

/// A tray click within this long after a blur-hide is the click that caused the blur.
const CLICK_GRACE: Duration = Duration::from_millis(500);

pub struct PopupState {
    anchor: Mutex<Option<Px>>,
    /// Content height reported by the webview, logical px.
    height: Mutex<f64>,
    blur_hid_at: Mutex<Option<Instant>>,
}

impl Default for PopupState {
    fn default() -> Self {
        Self { anchor: Mutex::new(None), height: Mutex::new(300.0), blur_hid_at: Mutex::new(None) }
    }
}

fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(LABEL)
}

fn px_from_rect(rect: &tauri::Rect) -> Px {
    let p = rect.position.to_physical::<i32>(1.0);
    let s = rect.size.to_physical::<i32>(1.0);
    Px { x: p.x, y: p.y, w: s.width, h: s.height }
}

/// Anchor from a tray click: the icon rectangle if Windows reported one, else the cursor.
pub fn anchor_from_event(rect: &tauri::Rect, cursor: PhysicalPosition<f64>) -> Px {
    let r = px_from_rect(rect);
    if r.w > 0 && r.h > 0 {
        r
    } else {
        Px { x: cursor.x.round() as i32, y: cursor.y.round() as i32, w: 1, h: 1 }
    }
}

/// Current tray icon rectangle, if Windows will tell us.
pub fn anchor_from_tray(app: &AppHandle) -> Option<Px> {
    let rect = app.tray_by_id(tray::ID)?.rect().ok()??;
    let r = px_from_rect(&rect);
    (r.w > 0 && r.h > 0).then_some(r)
}

fn mon_rect(m: &Monitor) -> Px {
    Px { x: m.position().x, y: m.position().y, w: m.size().width as i32, h: m.size().height as i32 }
}

fn work_rect(m: &Monitor) -> Px {
    let w = m.work_area();
    Px { x: w.position.x, y: w.position.y, w: w.size.width as i32, h: w.size.height as i32 }
}

fn layout(app: &AppHandle, win: &WebviewWindow) {
    let state = app.state::<PopupState>();
    let anchor = *state.anchor.lock().unwrap();
    let monitor = anchor
        .and_then(|a| {
            let (cx, cy) = a.center();
            app.monitor_from_point(cx as f64, cy as f64).ok().flatten()
        })
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };

    let scale = monitor.scale_factor();
    let (mon, work) = (mon_rect(&monitor), work_rect(&monitor));
    let anchor = anchor.unwrap_or_else(|| corner_anchor(work, mon));
    let want_w = (WIDTH * scale).round() as i32;
    let want_h = (*state.height.lock().unwrap() * scale).round() as i32;
    let gap = (GAP * scale).round() as i32;
    let r = place(anchor, work, mon, want_w, want_h, gap);

    // Move first so the window adopts the target monitor's DPI, then size, then settle the position.
    let _ = win.set_position(PhysicalPosition::new(r.x, r.y));
    let _ = win.set_size(PhysicalSize::new(r.w as u32, r.h as u32));
    let _ = win.set_position(PhysicalPosition::new(r.x, r.y));
}

/// Shows the popup (or the settings window when nothing is configured yet).
pub fn show(app: &AppHandle, anchor: Option<Px>) {
    let Some(win) = window(app) else { return };
    let state = app.state::<PopupState>();
    if let Some(a) = anchor {
        *state.anchor.lock().unwrap() = Some(a);
    }
    *state.blur_hid_at.lock().unwrap() = None;

    if !store::load().map(|c| c.is_complete()).unwrap_or(false) {
        commands::show_settings(app);
        return;
    }

    layout(app, &win);
    let _ = win.show();
    let _ = win.set_focus();
    let _ = app.emit_to(LABEL, "popup-shown", ());

    // Windows may refuse the first focus request (foreground lock); try once more shortly after.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(80));
        if win.is_visible().unwrap_or(false) && !win.is_focused().unwrap_or(true) {
            let _ = win.set_focus();
        }
    });
}

/// Explicit hide (Esc, close request, opening settings).
pub fn hide(app: &AppHandle) {
    if let Some(win) = window(app) {
        let _ = win.hide();
    }
    *app.state::<PopupState>().blur_hid_at.lock().unwrap() = None;
}

/// Left click on the tray icon.
pub fn toggle(app: &AppHandle, anchor: Option<Px>) {
    let Some(win) = window(app) else { return };
    if win.is_visible().unwrap_or(false) {
        hide(app);
        return;
    }
    let caused_by_this_click = app
        .state::<PopupState>()
        .blur_hid_at
        .lock()
        .unwrap()
        .take()
        .is_some_and(|t| t.elapsed() < CLICK_GRACE);
    if caused_by_this_click {
        return;
    }
    show(app, anchor);
}

/// `Focused(false)` on the popup.
pub fn on_blur(app: &AppHandle) {
    let Some(win) = window(app) else { return };
    if win.is_visible().unwrap_or(false) {
        let _ = win.hide();
        *app.state::<PopupState>().blur_hid_at.lock().unwrap() = Some(Instant::now());
    }
}

/// The webview reports its natural content height; grow/shrink the window to match.
pub fn set_height(app: &AppHandle, logical_height: f64) {
    if !logical_height.is_finite() {
        return;
    }
    let state = app.state::<PopupState>();
    *state.height.lock().unwrap() = logical_height.max(MIN_HEIGHT);
    if let Some(win) = window(app) {
        if win.is_visible().unwrap_or(false) {
            layout(app, &win);
        }
    }
}
