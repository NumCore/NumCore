# Hacking on NumCore

A practical guide for working on the firmware day-to-day.

## Build commands

```bash
# Build firmware (release, optimised for size)
make build
# or
cargo build -p numcore --release --target thumbv7m-none-eabi

# Build firmware (debug, faster iteration)
cargo build -p numcore --features lm3s811 --target thumbv7m-none-eabi

# Run host-side unit tests
make test
# or
cargo test -p numcore_math --tests

# Run everything (firmware build + host tests)
make all

# Clean build artifacts
cargo clean
```

The release build uses `opt-level = "z"` (minimise code size) and LTO because the firmware must fit in 64 KB Flash. Debug builds omit optimisation for faster compile times during development.

The project is a **Cargo workspace**. The firmware binary (`firmware/`) targets `thumbv7m-none-eabi`. The HAL crate (`hal-lm3s811/`) compiles for the same target. The test-suite (`test-suite/`) compiles for the host. The workspace root `.cargo/config.toml` does not set a default build target — you must pass `--target thumbv7m-none-eabi` when building the firmware.

### Why no default target?

Previously `.cargo/config.toml` set `[build] target = "thumbv7m-none-eabi"` at the workspace level, which caused all workspace members (including the test-suite) to compile for the embedded target. Breaking changes:

- **IDE support:** rust-analyzer and JetBrains RustRover would try to compile test-suite for `thumbv7m-none-eabi`, which has no `std` or `test` crate, producing hundreds of errors.
- **`cargo test`:** running `cargo test -p numcore_math` (without `--target`) failed because the test binary needs `std`.

The fix: remove the default target from the workspace config. Target-specific linker flags remain in `[target.thumbv7m-none-eabi]`. Use the explicit `--target thumbv7m-none-eabi` flag for firmware builds, or use `make build`.

## Running in QEMU

### Development (fast iteration)

```bash
cargo build -p numcore --features lm3s811 --target thumbv7m-none-eabi && qemu-system-arm \
  -M lm3s811evb \
  -serial mon:stdio \
  -display none \
  -kernel target/thumbv7m-none-eabi/debug/NumCore
```

This runs headless with UART on stdio. Type expressions in the terminal, see results immediately. Press Ctrl+C to quit.

## With OLED display

```bash
cargo build -p numcore --target thumbv7m-none-eabi && qemu-system-arm \
  -M lm3s811evb \
  -serial mon:stdio \
  -display gtk \
  -kernel target/thumbv7m-none-eabi/debug/NumCore
```

The GTK window shows the OLED display. UART still works in the terminal. If the OLED window is selected, switch to the serial console from **View → serial0** to type.

## Release testing

```bash
make build && qemu-system-arm \
  -M lm3s811evb \
  -serial mon:stdio \
  -display none \
  -kernel target/thumbv7m-none-eabi/release/NumCore
```

## Quick expression tests

```bash
# Pipe a single expression
echo "2+2" | cargo run -p numcore --features lm3s811 --release --target thumbv7m-none-eabi
# → = 4

# Interactive session
qemu-system-arm -M lm3s811evb -serial mon:stdio -display none \
  -kernel target/thumbv7m-none-eabi/release/NumCore
# Type: sin(45)   → = 0.850903 (sin of 45 radians)
# Type: sin(deg(45)) → = 0.707106 (sin of 45 degrees)
```

## Host-side unit tests

The `test-suite/` workspace member includes every `firmware/src/math/*.rs` file via `#[path]` attributes and compiles them for the host. 143 tests exercise the entire math engine:

```bash
# Run all tests
cargo test -p numcore_math --tests

# Run a specific test
cargo test -p numcore_math --tests test_sqrt_perfect_squares

# List all tests
cargo test -p numcore_math --tests -- --list
```

11 tests are ignored on the host due to differences in overflow behaviour (CORDIC overflow, integrator limits, Stirling/Lanczos precision). They pass correctly on the embedded target.

## Debugging

### Binary size

```bash
# Flash occupancy
arm-none-eabi-size target/thumbv7m-none-eabi/release/NumCore

# Section breakdown
arm-none-eabi-objdump -h target/thumbv7m-none-eabi/release/NumCore
```

### Disassembly

```bash
# Full disassembly (pipe to less — it's long)
arm-none-eabi-objdump -d target/thumbv7m-none-eabi/release/NumCore | less

# Just the vector table
arm-none-eabi-objdump -s -j .vector_table target/thumbv7m-none-eabi/release/NumCore
```

### GDB in QEMU

```bash
# Terminal 1: start QEMU with GDB stub
qemu-system-arm -M lm3s811evb -serial mon:stdio -display none \
  -kernel target/thumbv7m-none-eabi/debug/NumCore \
  -s -S

# Terminal 2: connect GDB
arm-none-eabi-gdb target/thumbv7m-none-eabi/debug/NumCore
(gdb) target remote localhost:1234
(gdb) break runtime::start
(gdb) continue
```

## Verifying Q31.32 constants

All mathematical constants in `firmware/src/math/fixed_point.rs` are computed as:

```
round(value × 2³²)
```

Verify with Python:

```python
import math
scale = 2**32
pi = round(math.pi * scale)     # → 13493037705
e  = round(math.e * scale)      # → 11674931555
ln2 = round(math.log(2) * scale) # → 2977044472
```

To verify a CORDIC or Taylor-series result, compare the Q31.32 output against:

```python
def from_q31_32(val):
    return val / 2**32 if val >= 0 else (val + 2**64) / 2**32
```

## Working with scratch buffers

The `LexResult` and `ParseTree` buffers live in `CalcState` to avoid stack allocation. When adding a new stage to the pipeline:

1. Add the scratch buffer type as a field in `CalcState` (in `firmware/src/runtime/state.rs`)
2. Initialise it in `CalcState::new()`
3. Pass `&mut` references through the call chain from `runtime::handle_expression_submission()`
4. Never stack-allocate a buffer larger than ~128 bytes

## Adding a new math function

1. Add the function token to `Token` in `firmware/src/math/lexer.rs`
2. Add the identifier match in `parse_identifier()` — identifiers are **case-sensitive**; function names must be lowercase
3. Add the AST node variant in `parser.rs` (or reuse an existing pattern). If the function takes a register as an argument (like `sto`), add a dedicated node variant rather than reusing `TwoArgFunction`
4. Add the function to `MathFunction` enum and `token_to_single_arg_function()`
5. Implement the logic in `fixed_point.rs` or `distributions.rs`
6. Wire it up in `evaluator.rs` `apply_function()` (or in `evaluate_node()` match for special nodes like `Store`)
7. Add to the welcome banner in `firmware/src/runtime/mod.rs`
8. Add test cases in `test-suite/tests/math.rs` covering expected values, domain errors, and overflow edges

## Adding implicit multiplication support for a new token type

If you add a new token that can start a primary expression (e.g. a new literal type), update `is_primary_start()` in `firmware/src/math/parser.rs`. This function is the single gate for implicit multiplication detection in `parse_term()`.

## Adding a new HAL peripheral

1. Define register offsets and bit masks in a new file under `hal-lm3s811/src/` (or the relevant HAL crate)
2. Implement safe public functions using `mmio::read_register` / `mmio::write_register`
3. Add clock gating via `clock::enable_rcgc*_peripherals()`
4. Configure GPIO pins with `gpio::configure_pins_as_alternate_function()`
5. Add the module to `hal-lm3s811/src/lib.rs`
6. Call initialisation from `firmware/src/runtime/mod.rs` `initialise_all_hardware()`

## Adding support for a new MCU

1. Create a new HAL crate `hal-<mcu>/` (copy `hal-lm3s811/` as a starting template)
2. Implement all HAL modules (`uart`, `i2c`, `gpio`, `clock`, `mmio`, `oled`) for the new MCU
3. Create `firmware/src/boot_<mcu>.rs` with the vector table and Reset handler
4. Create a linker script `link-<mcu>.x` for the MCU's memory map
5. Add the target triple to `rust-toolchain.toml`
6. Add the HAL crate as a dependency in `firmware/Cargo.toml` (optional, feature-gated)
7. Add the feature gate and `#[path]` in `firmware/src/main.rs`
8. Add target-specific `rustflags` in `.cargo/config.toml` if needed
9. Update `Makefile` with the new MCU target and feature

## Memory budgeting

| Region   | Size  | Contents                        |
|----------|-------|---------------------------------|
| Flash    | 64 KB | Vector table (256 B), code, rodata |
| .data    | ~256 B| Initialised statics             |
| .bss     | ~2 KB | CalcState (~1.5 KB), other statics |
| Stack    | 2 KB  | Call frames, local variables    |
| Free RAM | ~3.5 KB | Available for future features |

Current release build is approximately **12 KB** Flash and **2.2 KB** RAM.
