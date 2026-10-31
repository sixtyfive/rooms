PREFIX ?= /usr
DESTDIR ?=
BIN = $(DESTDIR)$(PREFIX)/bin/ha-rooms
DESKTOP = $(DESTDIR)$(PREFIX)/share/applications/dev.local.HaRooms.desktop
ICON = $(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/actions/ha-switches.svg
ICON_SRC ?= /usr/share/icons/breeze-dark/actions/22/gnumeric-object-checkbox.svg

.PHONY: all install uninstall clean

all:
	cargo build --release

install:
	install -Dm755 target/release/ha-rooms $(BIN)
	install -Dm644 $(ICON_SRC) $(ICON)
	install -Dm644 resources/dev.local.HaRooms.desktop $(DESKTOP)

uninstall:
	rm -f $(BIN) $(DESKTOP) $(ICON)

clean:
	cargo clean
