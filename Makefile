.PHONY: web acme serve-web test

# The engine (crates/breadbin) as WebAssembly, next to the page that loads it.
web:
	cargo build --release -p breadbin --target wasm32-unknown-unknown
	cp target/wasm32-unknown-unknown/release/breadbin.wasm docs/play/breadbin.wasm

# ACME, the cross-assembler of many C64 demos, as WebAssembly for the Hack tab
# (needs emscripten's emcc).
ACME_COMMIT = 99fae48b1fc44a3fc25b84d51857d36c1d6967e1
acme:
	test -d target/acme || git clone -q https://github.com/meonwax/acme.git target/acme
	git -C target/acme checkout -q $(ACME_COMMIT)
	emcc -O2 -w $$(ls target/acme/src/*.c | grep -v '/_') -o docs/play/acme.mjs \
		-sMODULARIZE -sEXPORT_ES6 -sEXPORT_NAME=createAcme -sINVOKE_RUN=0 -sFORCE_FILESYSTEM \
		-sALLOW_MEMORY_GROWTH -sEXPORTED_RUNTIME_METHODS=FS,callMain -sENVIRONMENT=web

serve-web: web acme
	python3 -m http.server -d docs 8064

test:
	cargo test
