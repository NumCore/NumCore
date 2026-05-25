TARGET = thumbv7m-none-eabi

.PHONY: all build test check clean

all: build test

build:
	cargo build -p numcore --release --target $(TARGET)

test:
	cargo test -p numcore_math --tests

check:
	cargo check -p numcore --release --target $(TARGET)

clean:
	cargo clean
