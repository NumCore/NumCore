# NumCore

Bare-metal scientific calculator firmware for the LM3S811 ARM Cortex-M3
(64 KB Flash, 8 KB SRAM), written in Rust with #![no_std] and #![no_main].

Version 0.7.0.

Full documentation: **[numcore.tech](https://numcore.tech)**

## Features

- Q31.32 fixed-point arithmetic with complex-number support.
- Standard, Advanced, Matrix, and Scientific modes.
- Scientific functions, statistics, distributions, integration, and summation.
- Matrix arithmetic, determinants, inverses, and related operations.
- Eight-entry expression history with Up and Down recall.
- Serial console and 96x16 OLED output.
- Browser simulator compiled to WebAssembly.

## Quick start

```bash
rustup target add thumbv7m-none-eabi
cargo build -p numcore-lm3s811 --release --target thumbv7m-none-eabi
cargo test -p numcore_math --tests -- --test-threads=1
# The maths engine keeps a small global overflow and integration state, so
# host tests run serially.

qemu-system-arm -M lm3s811evb -serial mon:stdio -display none \
  -kernel target/thumbv7m-none-eabi/release/NumCore
```

## Controls

| Key | Action |
| --- | --- |
| Enter | Evaluate the current expression |
| Backspace | Delete the character before the cursor |
| Up | Recall an older expression |
| Down | Recall a newer expression, or clear the current recall |
| Left/Right | Move the input cursor, or scroll the result |
| Esc | Cycle Standard, Advanced, Matrix, and Scientific modes |
| Ctrl+D | Toggle radians and degrees |

History keeps the last eight submitted expressions. Submitting the same
expression twice in a row does not add a duplicate entry. Editing a recalled
expression leaves history mode while keeping the edited text.

## Web simulator

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.122 --locked
cargo build -p hal-wasm --target wasm32-unknown-unknown
wasm-bindgen target/wasm32-unknown-unknown/debug/hal_wasm.wasm \
  --out-dir hal-wasm/www/pkg --target no-modules
python3 -m http.server 8080 --directory hal-wasm/www
```

Open `http://localhost:8080`.

To create a single offline HTML file:

```bash
python3 hal-wasm/build-single-html.py hal-wasm/www/index-standalone.html
```

The simulator uses the same `CalcState`, expression evaluator, and framebuffer
as the firmware. The WASM smoke test verifies startup rendering, scalar
results, matrix results, and history recall:

```bash
wasm-bindgen target/wasm32-unknown-unknown/debug/hal_wasm.wasm \
  --out-dir /tmp/numcore-wasm --target nodejs
node hal-wasm/tests/smoke.js /tmp/numcore-wasm/hal_wasm.js
```

## Testing and metrics

```bash
cargo fmt --check
cargo test -p numcore_math --tests -- --test-threads=1
cargo build -p numcore-lm3s811 --release --target thumbv7m-none-eabi
cargo build -p hal-wasm --target wasm32-unknown-unknown
```

Current test counts, firmware sizes, and simulator sizes are recorded in
[METRICS.md](METRICS.md).

## License

GPL-3.0
