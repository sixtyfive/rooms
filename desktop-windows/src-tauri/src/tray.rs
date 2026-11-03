use crate::{autostart, commands, popup};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

pub const ID: &str = "main";

pub struct AutostartItem(CheckMenuItem<Wry>);

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)?;
    let autostart_item =
        CheckMenuItem::with_id(app, "autostart", "Start with Windows", true, autostart::is_enabled(), None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &settings, &autostart_item, &separator, &quit])?;
    app.manage(AutostartItem(autostart_item));

    TrayIconBuilder::with_id(ID)
        .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
        .tooltip("HA Rooms")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => popup::show(app, popup::anchor_from_tray(app)),
            "settings" => commands::show_settings(app),
            "autostart" => {
                let want = app.state::<AutostartItem>().0.is_checked().unwrap_or(false);
                if let Err(e) = autostart::set(want) {
                    eprintln!("autostart: {e}");
                }
                sync_autostart(app); // show what actually got stored
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // Act on mouse-up only (the down event is what takes focus away from the popup).
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                position,
                rect,
                ..
            } = event
            {
                let app = tray.app_handle();
                popup::toggle(app, Some(popup::anchor_from_event(&rect, position)));
            }
        })
        .build(app)?;
    Ok(())
}

/// Re-reads the registry into the menu's check mark.
pub fn sync_autostart(app: &AppHandle) {
    if let Some(item) = app.try_state::<AutostartItem>() {
        let _ = item.0.set_checked(autostart::is_enabled());
    }
}
