# ha-rooms

- `core/` – platform independent Rust: config, Home Assistant client, model, grouping
- `desktop-cosmic/` – COSMIC panel applet (`ha-rooms`) and settings window (`ha-rooms-settings`)
- `shared/` – Jinja template, behavior spec, JSON fixtures used by the tests of every frontend

    make && sudo make install && pkill cosmic-panel
