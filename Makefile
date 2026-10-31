PREFIX ?= /usr
DESTDIR ?=
BIN = $(DESTDIR)$(PREFIX)/bin/ha-rooms
BIN_SETTINGS = $(DESTDIR)$(PREFIX)/bin/ha-rooms-settings
DESKTOP = $(DESTDIR)$(PREFIX)/share/applications/dev.local.HaRooms.desktop
DESKTOP_SETTINGS = $(DESTDIR)$(PREFIX)/share/applications/dev.local.HaRoomsSettings.desktop
ICON = $(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/actions/ha-switches.svg
ICON_SRC ?= /usr/share/icons/breeze-dark/actions/22/gnumeric-object-checkbox.svg

.PHONY: all install uninstall clean

all:
	cargo build --release

install:
	install -Dm755 target/release/ha-rooms $(BIN)
	install -Dm755 target/release/ha-rooms-settings $(BIN_SETTINGS)
	install -Dm644 $(ICON_SRC) $(ICON)
	install -Dm644 resources/dev.local.HaRooms.desktop $(DESKTOP)
	install -Dm644 resources/dev.local.HaRoomsSettings.desktop $(DESKTOP_SETTINGS)

uninstall:
	rm -f $(BIN) $(BIN_SETTINGS) $(DESKTOP) $(DESKTOP_SETTINGS) $(ICON)

clean:
	cargo clean
