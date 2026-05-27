# Hacking on NumCore

A practical guide for working on the firmware day-to-day.

## Build commands

```bash
# Build firmware (release, optimised for size)
make build
# or
cargo build -p numcore-lm3s811 --release --target thumbv7m-none-eabi

# Build firmware (debug, faster iteration)
cargo build -p numcore-lm3s811 --target thumbv7m-none-eabi

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

The project is a **Cargo workspace**. The shared lib crate (`numcore/`) has no target restriction and compiles on any platform. The per-MCU binary crate (`numcore-lm3s811/`) targets `thumbv7m-none-eabi`. The HAL crate (`hal-lm3s811/`) compiles for the same target. The test-suite (`test-suite/`) compiles for the host. The workspace root `.cargo/config.toml` does not set a default build target — you must pass `--target thumbv7m-none-eabi` when building the firmware.

### Why no default target?

Previously `.cargo/config.toml` set `[build] target = "thumbv7m-none-eabi"` at the workspace level, which caused all workspace members (including the test-suite) to compile for the embedded target. Breaking changes:

- **IDE support:** rust-analyzer and JetBrains RustRover would try to compile test-suite for `thumbv7m-none-eabi`, which has no `std` or `test` crate, producing hundreds of errors.
- **`cargo test`:** running `cargo test -p numcore_math` (without `--target`) failed because the test binary needs `std`.

The fix: remove the default target from the workspace config. Target-specific linker flags remain in `[target.thumbv7m-none-eabi]`. Use the explicit `--target thumbv7m-none-eabi` flag for firmware builds, or use `make build`.

## Running in QEMU

### Development (fast iteration)

```bash
cargo build -p numcore-lm3s811 --target thumbv7m-none-eabi && qemu-system-arm \
  -M lm3s811evb \
  -serial mon:stdio \
  -display none \
  -kernel target/thumbv7m-none-eabi/debug/NumCore
```

This runs headless with UART on stdio. Type expressions in the terminal, see results immediately. Press Ctrl+C to quit.

## With OLED display

```bash
cargo build -p numcore-lm3s811 --target thumbv7m-none-eabi && qemu-system-arm \
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
echo "2+2" | cargo run -p numcore-lm3s811 --release --target thumbv7m-none-eabi
# → = 4

# Interactive session
qemu-system-arm -M lm3s811evb -serial mon:stdio -display none \
  -kernel target/thumbv7m-none-eabi/release/NumCore
# Type: sin(45)   → = 0.850903 (sin of 45 radians)
# Type: sin(deg(45)) → = 0.707106 (sin of 45 degrees)
```

## Host-side unit tests

The `test-suite/` workspace member includes every `numcore/src/math/*.rs` file via `#[path]` attributes and compiles them for the host. 250 tests exercise the entire math engine:

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
# Quick summary (text/data/bss/dec)
cargo size -p numcore-lm3s811 --release --target thumbv7m-none-eabi

# Flash occupancy (same output, via arm-none-eabi-size)
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
(gdb) break numcore::runtime::start
(gdb) continue
```

## Verifying Q31.32 constants

All mathematical constants in `numcore/src/math/fixed_point.rs` are computed as:

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

1. Add the scratch buffer type as a field in `CalcState` (in `numcore/src/runtime/state.rs`)
2. Initialise it in `CalcState::new()`
3. Pass `&mut` references through the call chain from `runtime::handle_expression_submission()`
4. Never stack-allocate a buffer larger than ~128 bytes

## Adding a new math function

1. Add the function token to `Token` in `numcore/src/math/lexer.rs`
2. Add the identifier match in `parse_identifier()` — identifiers are **case-sensitive**; function names must be lowercase
3. Add the AST node variant in `parser.rs` (or reuse an existing pattern). If the function takes a register as an argument (like `sto`), add a dedicated node variant rather than reusing `TwoArgFunction`
4. Add the function to `MathFunction` enum and `token_to_single_arg_function()`
5. Implement the logic in `fixed_point.rs` or `distributions.rs`
6. Wire it up in `evaluator.rs` `apply_function()` (or in `evaluate_node()` match for special nodes like `Store`)
7. Add to the welcome banner in `numcore/src/runtime/mod.rs`
8. Add test cases in `test-suite/tests/math.rs` covering expected values, domain errors, and overflow edges

## Adding implicit multiplication support for a new token type

If you add a new token that can start a primary expression (e.g. a new literal type), update `is_primary_start()` in `numcore/src/math/parser.rs`. This function is the single gate for implicit multiplication detection in `parse_term()`.

## Adding a new HAL peripheral

1. Define register offsets and bit masks in a new file under `hal-lm3s811/src/` (or the relevant HAL crate)
2. Implement safe public functions using `mmio::read_register` / `mmio::write_register`
3. Add clock gating via `clock::enable_rcgc*_peripherals()`
4. Configure GPIO pins with `gpio::configure_pins_as_alternate_function()`
5. Add the module to `hal-lm3s811/src/lib.rs`
6. If the peripheral is used in `Display::init()`, add the call in the trait impl in `lib.rs`

## Adding support for a new MCU

1. Create a new HAL crate `hal-<mcu>/` implementing `numcore::hal::Uart` and `numcore::hal::Display`
2. Copy `hal-lm3s811/` as a starting template for register definitions
3. Create a new binary crate `numcore-<mcu>/` with:
   - `Cargo.toml`: depends on `numcore` and `hal-<mcu>`
   - `src/main.rs`: thin wrapper calling `numcore::runtime::start::<HalUart, HalDisplay>()` + panic handler
   - `src/boot.rs`: vector table, Reset handler calling `crate::start()`
4. Create a linker script `link.x` for the MCU's memory map, place in the HAL crate
5. Add the target triple to `rust-toolchain.toml`
6. Add target-specific `rustflags` in `.cargo/config.toml` pointing to `-Thal-<mcu>/link.x`
7. Add `numcore-<mcu>` to workspace `Cargo.toml`
8. Add `build-<mcu>` target to `Makefile`

No changes to `numcore/` required.

## Memory budgeting

Actual numbers from `cargo size` and stack canary measurement (release build):

| Region               | Usage          | Budget | Usage |
|----------------------|----------------|--------|-------|
| Flash                | 50 343 bytes   | 64 KB  | 77%   |
| .data                | 0 bytes        | —      | —     |
| .bss (statics)       | 2 128 bytes    |  8 KB  | 26%   |
| Stack (reserved)     | 3 072 bytes    |  8 KB  | 37%   |
| Stack (actual max)   | 3 064 bytes    |  3 KB  | 99%   |
| **Peak RAM**         | **5 192 bytes**| **8 KB** | **63%** |

Stack is 3 KB as set in `hal-lm3s811/link.x` (increased from 2 KB after complex number and cursor-editing features pushed utilization to 99%). `.data` is 0 — no initialised statics. Actual max stack depth was measured empirically by instrumenting the evaluator to track `min_sp` (see below).

### Measuring peak stack usage

Peak stack depth is measured by instrumenting the deepest recursive function (`evaluate_node` in `numcore/src/math/evaluator.rs`) to track the minimum SP seen:

1. **Add a global SP watermark** to `numcore/src/math/mod.rs`:
   ```rust
   #[no_mangle]
   pub static mut MIN_SP: usize = 0x2000_2000;

   pub fn track_sp() {
       let sp: usize;
       unsafe { core::arch::asm!("mov {0}, sp", out(reg) sp); }
       unsafe { if sp < MIN_SP { MIN_SP = sp; } }
   }
   ```

2. **Call it from `evaluate_node`** (evaluator.rs, first line of function body):
   ```rust
   crate::math::track_sp();
   ```

3. **Build without stripping** (comment out `strip = "symbols"` in `Cargo.toml`), then run the workload:
   ```bash
   cargo build -p numcore-lm3s811 --release --target thumbv7m-none-eabi
   qemu-system-arm -M lm3s811evb -serial mon:stdio -display none \
     -kernel target/thumbv7m-none-eabi/release/NumCore \
     -gdb tcp::1234 < test_inputs.txt
   ```

4. **Read the watermark via GDB** (in another terminal):
   ```bash
   gdb -batch -nx \
     -ex "target remote :1234" \
     -ex "symbol-file target/thumbv7m-none-eabi/release/NumCore" \
     -ex "x/gx &MIN_SP" \
     target/thumbv7m-none-eabi/release/NumCore
   ```
   Stack used = `0x20002000 − MIN_SP`.

5. **Revert** all instrumentation changes and restore `strip = "symbols"` before committing.

> **Note:** The older canary-fill technique (`write 0xDEADBEEF` at boot, then scan for overwrites) is unreliable because `sub sp, #N` stack-pointer adjustments (used for large local arrays like the 192-byte OLED framebuffer) can jump over canary words without overwriting them, leading to a severe underestimate of peak depth.
