.PHONY: web acme drive studio examples serve-web test test-studio test-studio-browser

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

# Pascual DOS, a free 1541 DOS ROM (MIT), with breadbin's patch for fast
# loaders (tools/drive), for the demos that run code on the drive (needs cc65).
PDOS_COMMIT = 02a4b64ab7cf2a2e45b17025db7b2b6fe9351605
drive:
	test -d target/pdos || git clone -q https://github.com/Pascual-Candel-Palazon/Pascual_DOS-1541.git target/pdos
	git -C target/pdos checkout -q -f $(PDOS_COMMIT)
	git -C target/pdos apply $(CURDIR)/tools/drive/breadbin.patch
	$(MAKE) -C target/pdos dos.bin
	node tools/drive/sign.mjs target/pdos/dos.bin docs/play/dos1541.rom

# The Studio (the Hack tab): cc65, ca65 and ld65 as WebAssembly with the C64
# library and headers (needs a C compiler and emscripten's emcc), and the
# CodeMirror bundle (needs Node). See tools/studio/README.md.
studio:
	tools/studio/cc65.sh
	cd tools/studio && npm ci && node bundle.mjs

# The Studio's example projects (docs/play/studio/examples), from
# tools/studio/examples and tools/studio/make-examples.mjs.
examples:
	node tools/studio/make-examples.mjs

serve-web: web acme studio
	python3 -m http.server -d docs 8064

test:
	cargo test

# The Studio's tests in Node: the formats, cc65 in WebAssembly, the SID player
# (needs `make web` for breadbin.wasm) and the examples.
test-studio:
	cd tools/studio && node --test test/unit/ test/build/

# ...and in headless Chromium (CHROME=/path/to/chrome): the editors, saving,
# and the games played on vc64web (skipped offline).
test-studio-browser:
	cd tools/studio && node --test --test-concurrency=1 test/browser/
