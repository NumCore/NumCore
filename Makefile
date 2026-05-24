TARGET = thumbv7m-none-eabi

.PHONY: all build test check clean

all: build test

build:
	cargo build --release --target $(TARGET)

test:
	cargo test -p numcore_math --tests

check:
	cargo check --release --target $(TARGET)

clean:
	cargo clean
