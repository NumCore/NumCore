# NumCore

Bare-metal scientific calculator firmware for the LM3S811 ARM Cortex-M3
(64 KB Flash, 8 KB SRAM), written in Rust with `#![no_std]` `#![no_main]`.

Full documentation: **[numcore.tech](https://numcore.tech)**

## Quick start

```bash
rustup target add thumbv7m-none-eabi
cargo build -p numcore-lm3s811 --release --target thumbv7m-none-eabi
cargo test -p numcore_math --tests

qemu-system-arm -M lm3s811evb -serial mon:stdio -display none \
  -kernel target/thumbv7m-none-eabi/release/NumCore
```

## License

GPL-3.0
