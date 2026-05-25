TARGET = thumbv7m-none-eabi
MCUS   = lm3s811

.PHONY: all build build-lm3s811 test check clean

all: build test

build: build-lm3s811

build-lm3s811:
	cargo build -p numcore-lm3s811 --release --target $(TARGET)

test:
	cargo test -p numcore_math --tests

check:
	cargo check -p numcore-lm3s811 --release --target $(TARGET)

clean:
	cargo clean
