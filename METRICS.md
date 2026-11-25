# NumCore metrics

Measured 2026-11-25 for v0.7.0.

## Host tests

| Suite | Tests |
| --- | ---: |
| Engine API tests | 21 |
| History tests | 6 |
| Math tests | 313 |
| Total | 340 |

The math suite reports 6 ignored tests. Host tests run with
`--test-threads=1`.

## Firmware image

| Section | Bytes |
| --- | ---: |
| `.text` | 61,868 |
| `.data` | 3,321 |
| `.bss` including 4,096-byte stack | 4,624 |
| ELF file | 135,020 |

Flash used: 65,189 of 65,536 bytes (347 bytes free, 99.47 percent).
RAM used: 7,945 of 8,192 bytes (247 bytes free, 96.99 percent).

The expression history ring accounts for 512 bytes of BSS.

## WebAssembly

| Artifact | Bytes |
| --- | ---: |
| Debug WASM | 3,495,944 |
| Standalone HTML | 468,191 |

The standalone HTML embeds the WASM binary and the wasm-bindgen runtime.

## Verification

- `cargo fmt --check` passes.
- `cargo test -p numcore_math --tests -- --test-threads=1` passes 340 tests.
- `cargo build -p numcore-lm3s811 --release --target thumbv7m-none-eabi` succeeds.
- `cargo build -p hal-wasm --target wasm32-unknown-unknown` succeeds.
- `node hal-wasm/tests/smoke.js` passes against the generated WASM bindings.
- The standalone HTML renders text in the framebuffer under headless Firefox.
