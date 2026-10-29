PREFIX ?= /usr

.PHONY: build install uninstall web serve-web

build:
	packaging/install.sh build

install: build
	sudo PREFIX=$(PREFIX) packaging/install.sh

uninstall:
	sudo rm -f $(PREFIX)/bin/breadbin
	sudo rm -f $(PREFIX)/share/glib-2.0/schemas/io.github.jacobandresen.Breadbin.gschema.xml
	sudo rm -f $(PREFIX)/share/applications/breadbin.desktop
	sudo rm -f $(PREFIX)/share/icons/hicolor/scalable/apps/breadbin.svg
	sudo glib-compile-schemas $(PREFIX)/share/glib-2.0/schemas
	sudo gtk-update-icon-cache -q -t -f $(PREFIX)/share/icons/hicolor 2>/dev/null || true
	sudo update-desktop-database -q $(PREFIX)/share/applications 2>/dev/null || true

# The browser version (docs/play): build the SID player as WebAssembly.
web:
	cargo build --release -p breadbin-sid-wasm --target wasm32-unknown-unknown
	cp target/wasm32-unknown-unknown/release/breadbin_sid_wasm.wasm docs/play/sid.wasm

serve-web: web
	python3 -m http.server -d docs 8064
