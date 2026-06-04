TARGET    = thumbv7m-none-eabi
MCUS      = lm3s811
WASM_TARGET = wasm32-unknown-unknown
WASM_DIR  = hal-wasm
WWW_DIR   = $(WASM_DIR)/www
PKG_DIR   = $(WWW_DIR)/pkg

.PHONY: all build build-lm3s811 test check clean \
        wasm wasm-bindgen wasm-serve wasm-standalone

all: build test

build: build-lm3s811

build-lm3s811:
	cargo build -p numcore-lm3s811 --release --target $(TARGET)

test:
	cargo test -p numcore_math --tests

check:
	cargo check -p numcore-lm3s811 --release --target $(TARGET)

wasm:
	cargo build -p hal-wasm --target $(WASM_TARGET)

wasm-bindgen: wasm
	wasm-bindgen target/$(WASM_TARGET)/debug/hal_wasm.wasm \
		--out-dir $(PKG_DIR) --target no-modules

wasm-serve: wasm-bindgen
	python3 -m http.server 8080 --directory $(WWW_DIR)

wasm-standalone: wasm-bindgen
	python3 $(WASM_DIR)/build-single-html.py \
		target/$(WASM_TARGET)/debug/index-standalone.html

clean:
	cargo clean
	rm -rf $(PKG_DIR)
