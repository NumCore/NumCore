# Contributing to NumCore

## Development environment

### Required tools

```bash
# Rust toolchain
rustup target add thumbv7m-none-eabi

# QEMU for ARM (for running the firmware)
# Debian/Ubuntu:
apt install qemu-system-arm

# Arch Linux:
pacman -S qemu-system-arm

# macOS (Homebrew):
brew install qemu
```

### IDE setup

The project is a Cargo workspace with four members — the shared lib crate (`numcore/`, no target restriction), the per-MCU binary crate (`numcore-lm3s811/`, target `thumbv7m-none-eabi`), the HAL crate (`hal-lm3s811/`, same target), and the test suite (host target). The workspace root `.cargo/config.toml` does **not** set a default build target (removed because it breaks host-side test compilation in IDEs).

**JetBrains RustRover:** Open the workspace root. The test-suite crate is automatically analysed for the host target. For firmware crates, configure the target in **Settings → Languages & Frameworks → Rust → Cargo → Default target** (set to `thumbv7m-none-eabi`), or use the explicit target in run configurations.

**VS Code with rust-analyzer:**
```json
// .vscode/settings.json
{
    "rust-analyzer.cargo.target": "thumbv7m-none-eabi",
    "rust-analyzer.checkOnSave.allTargets": false
}
```

For full test analysis in VS Code, run `cargo test -p numcore_math --tests` from the terminal.

## Code conventions

### Style

- Format all code with `rustfmt` (run `cargo fmt` before every commit)
- Follow the existing naming conventions: `snake_case` for functions/variables, `CamelCase` for types, `SCREAMING_SNAKE_CASE` for constants
- Maximum line length: 100 characters
- No trailing whitespace
- Alphabetise module declarations, match arms, and import lists

### No comments

Do not add comments unless the code cannot be made self-documenting. Prefer:
- Meaningful identifier names
- Small, single-purpose functions
- Clear type systems (enums over bools, newtypes over raw primitives)

The existing doc comments on modules, types, and public functions are exceptions — they should remain.

### Safety

NumCore's **safety contract** is strict and non-negotiable:

- **Only `hal-<mcu>/`** may perform memory-mapped I/O via `unsafe` MMIO access. All `unsafe` for hardware is contained in `hal-<mcu>/src/mmio.rs`.
- **`numcore-<mcu>/src/boot.rs`** is the only exception — `unsafe` for `.bss`/`.data` memory initialisation before the HAL is online.
- **`numcore/src/runtime/`, `numcore/src/math/`, and `numcore/src/ui/` must contain zero `unsafe` blocks.** Every hardware interaction goes through the `Uart` and `Display` traits defined in `numcore::hal`.
- Every `unsafe` block must have a `// SAFETY:` comment explaining why the invariants hold.
- If you must add `unsafe` outside `hal-<mcu>/` or `boot.rs`, discuss it in the PR first — it will receive extra scrutiny.

### Layer rules

The architecture enforces strict layering. Violations will be rejected:

| Layer | Location | May import | Must not import | Contains `unsafe` |
|-------|----------|------------|-----------------|-------------------|
| Boot | `numcore-<mcu>/src/boot.rs` | `core` | `hal-*`, `numcore::*` | Yes (memory init) |
| HAL | `hal-<mcu>/src/*` | `core`, `numcore::hal` (traits), other HAL submodules | `numcore::runtime`, `numcore::math`, `numcore::ui` | Yes (MMIO only) |
| Runtime | `numcore/src/runtime/` | `numcore::hal` (traits), `numcore::math`, `numcore::ui` | Concrete HAL crates | No |
| Math | `numcore/src/math/` | `core` only | Any HAL, `runtime/`, `ui/` | No |
| UI | `numcore/src/ui/` | `numcore::hal` (`Display` trait), `core` | Any HAL, `runtime/`, `math/` | No |

## Testing

### Host-side unit test suite

The `math/` layer is hardware-independent, and **143 automated tests** in the `test-suite/` workspace member verify every public function in the math engine. Run them from the workspace root:

```bash
# Full suite
cargo test -p numcore_math --tests

# Or via Makefile
make test

# Run a single test
cargo test -p numcore_math --tests test_sin_standard_angles

# Run all tests including skipped host-embedded ones (some will fail)
cargo test -p numcore_math
```

The test suite covers:
- **Constants** — bit-exact verification of `FIXED_PI`, `FIXED_E`, etc.
- **Core arithmetic** — `from_integer`, `to_integer_truncated`, `to_integer_rounded`
- **Multiply/Divide** — exact, fractional, negative, overflow, and rounding cases
- **Rounding & abs** — `floor`, `ceil`, `round`, `abs` (all integer rounding modes)
- **Square root** — perfect squares, fractional, large values, domain errors
- **Power & integer power** — integer exponents, fractional exponents, negative base, overflow
- **N-th root** — exact roots, negative n, negative base, domain errors
- **Trigonometry** — sin/cos/tan for standard angles, CORDIC precision, sin²+cos² identity
- **Inverse trig** — asin/acos/atan for standard values, domain bounds
- **Hyperbolic** — sinh/cosh/tanh basic values and saturation
- **Inverse hyperbolic** — asinh/acosh/atanh basic values and domain
- **Exponential & log** — exp/ln/log10/log2 basic values, overflow/underflow, roundtrip
- **Angle conversion** — deg/rad roundtrip, standard conversions
- **Formatting** — zero, integer, fractional, trailing zeros, negative, large numbers
- **VariableStore** — read/write ans and registers, invalid register rejection, Copy semantics
- **Distributions** — ln_factorial, ln_gamma, binomial, Poisson, chi-squared
- **Full pipeline** — lex→parse→eval for arithmetic, functions, constants, sto, sum, int
- **QEMU smoke-test parity** — exactly matches the expressions in `test_inputs.txt`

11 tests are **ignored** on the host due to known differences in overflow behaviour between the host compiler and the embedded target (CORDIC overflow, integrator limits, Stirling/Lanczos precision). These pass correctly on the real hardware.

### Adding new tests

When adding a math function:

1. If you added a constant: add a `test_constants_are_bit_exact()` assertion in `test-suite/tests/math.rs`
2. If you added a function: add dedicated tests in `test-suite/tests/math.rs` covering:
   - Expected values for representative inputs
   - Domain errors (invalid inputs → `None`)
   - Overflow/underflow at boundaries
   - Roundtrip consistency where applicable
3. Run `cargo test -p numcore_math --tests` to verify
4. Verify the result against Python (use `Decimal` or `int(value * 2**32)` for Q31.32)

### Firmware integration (QEMU)

Run in QEMU and pipe test inputs:

```bash
echo "sin(pi/2)" | cargo run -p numcore-lm3s811 --release --target thumbv7m-none-eabi
# Expected output: = 1
```

Compare against `test_inputs.txt`:

```bash
cat test_inputs.txt | cargo run -p numcore-lm3s811 --release --target thumbv7m-none-eabi
```

### Manual QEMU testing

```bash
make build && qemu-system-arm \
  -M lm3s811evb \
  -serial mon:stdio \
  -display gtk \
  -kernel target/thumbv7m-none-eabi/release/NumCore
```

## Pull request process

1. Create a feature branch from `main`
2. Make your changes, following the conventions above
3. Run `cargo test -p numcore_math --tests` and verify all tests pass
4. Run `make build` and verify the firmware compiles
5. Run in QEMU and verify existing functionality still works
6. Open a PR with a clear description of:
   - What the change does
   - Why it's needed
   - How it was tested
   - Any layer-rule implications

### What to include in your PR

- If adding a HAL feature: include the relevant register constants and bit masks
- If adding a math function: include test cases in `test-suite/tests/math.rs` (exact expected values, domain errors, overflow edges)
- If touching the parser: include example expressions that exercise the new grammar (including implicit multiplication: `3(5)`, `(a)b`, `2sin(x)`, `sto()`, case-sensitive identifier edge cases)
- If adding UI rendering: include a screenshot or ASCII-art of the display output

## Getting help

Open an issue on GitHub for bugs, questions, or feature requests. For architecture questions, reference `docs/ARCHITECTURE.md` and the module-level doc comments in the source code. For the project roadmap and long-term vision, see `ROADMAP.md`.
