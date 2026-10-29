.PHONY: web serve-web test

# The engine (crates/breadbin) as WebAssembly, next to the page that loads it.
web:
	cargo build --release -p breadbin --target wasm32-unknown-unknown
	cp target/wasm32-unknown-unknown/release/breadbin.wasm docs/play/breadbin.wasm

serve-web: web
	python3 -m http.server -d docs 8064

test:
	cargo test
