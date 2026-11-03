#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod commands;
mod placement;
mod popup;
mod store;
mod tray;
mod tray_promote;
mod view;

use tauri::{Manager, RunEvent, WindowEvent};

fn main() {
    let app = tauri::Builder::default()
        // Launching the exe again (Start menu, shortcut) is the way to reach the app when the
        // tray icon is buried in the overflow flyout: show the popup.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            popup::show(app, popup::anchor_from_tray(app));
        }))
        .manage(commands::AppState::default())
        .manage(popup::PopupState::default())
        .invoke_handler(tauri::generate_handler![
            commands::refresh,
            commands::toggle,
            commands::temp_submit,
            commands::temp_step,
            commands::hide_popup,
            commands::set_popup_height,
            commands::open_settings,
            commands::get_settings,
            commands::save_settings,
            commands::set_autostart,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            let (_, needs_setup, error) = store::init();
            if let Some(e) = error {
                commands::set_error(&handle, e);
            }

            autostart::refresh_path();
            tray::create(&handle)?;

            let popup_window = app.get_webview_window(popup::LABEL).ok_or("popup window missing")?;
            let h = handle.clone();
            popup_window.on_window_event(move |event| match event {
                WindowEvent::Focused(false) => popup::on_blur(&h),
                // Alt+F4 must not destroy the popup; it is reused for the app's lifetime.
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    popup::hide(&h);
                }
                _ => {}
            });

            if needs_setup {
                commands::show_settings(&handle);
            }
            tray_promote::spawn();
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building HA Rooms");

    app.run(|_app, event| {
        // Closing the settings window must not end the tray app; only Quit (code Some) does.
        if let RunEvent::ExitRequested { api, code: None, .. } = event {
            api.prevent_exit();
        }
    });
}
