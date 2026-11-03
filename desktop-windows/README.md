# ha-rooms – Windows 10/11 (Tauri)

Tray icon + popup. Reuses `ha-core` unchanged; the UI is plain HTML/JS in `ui/` (no bundler).

## Build

Needs Rust (MSVC toolchain) and WebView2 (preinstalled on Windows 11).

    cargo install tauri-cli --version "^2"
    cd desktop-windows/src-tauri
    cargo tauri dev        # run
    cargo tauri build      # NSIS installer in target/release/bundle/nsis (per-user, no admin)

Tests (platform independent parts): `cargo test` inside `src-tauri` on Windows.

## Behaviour

- **Config**: `%APPDATA%\ha-rooms\config.json` without the token. The token is in
  Credential Manager (service `dev.local.HaRooms`, user `access-token`). A token found in the file is moved there.
- **Popup**: left click on the tray icon. Closes on focus loss, on a second tray click, and on Esc.
- **Tray menu** (right click): Open, Settings..., Start with Windows, Quit.
- **Autostart**: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\HaRooms`
  (respects the switch in Task Manager > Startup apps).
- **Windows 11 overflow**: once per exe path the icon is promoted to the taskbar corner via
  `HKCU\Control Panel\NotifyIconSettings\*\IsPromoted` (undocumented, best effort). Starting the exe
  a second time always opens the popup, even if the icon is hidden.

## Layout (`src-tauri/src`)

| file | |
|---|---|
| `placement.rs` | popup geometry (pure, unit-tested) |
| `popup.rs` | show/hide rules, blur / second-click handling |
| `tray.rs` | tray icon and menu |
| `commands.rs` | IPC commands; owns the data state, emits `view` events |
| `store.rs` | config file + Credential Manager |
| `autostart.rs`, `tray_promote.rs` | registry |
| `view.rs` | view model built with `ha_core::model::group_by_room` |
