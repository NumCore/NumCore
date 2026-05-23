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

The project includes `.idea/` workspace files for JetBrains IDEs (CLion/RustRover). For VS Code, install `rust-analyzer` and configure the target:

```json
// .vscode/settings.json
{
    "rust-analyzer.cargo.target": "thumbv7m-none-eabi"
}
```

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

- **Only `hal/`** may perform memory-mapped I/O via `unsafe` MMIO access. All `unsafe` for hardware is contained in `hal/mmio.rs`.
- **`boot.rs`** is the only exception — `unsafe` for `.bss`/`.data` memory initialisation before the HAL is online.
- **`runtime/`, `math/`, and `ui/` must contain zero `unsafe` blocks.** Every hardware interaction must go through the HAL's safe public API.
- Every `unsafe` block must have a `// SAFETY:` comment explaining why the invariants hold.
- If you must add `unsafe` outside `hal/` or `boot.rs`, discuss it in the PR first — it will receive extra scrutiny.

### Layer rules

The architecture enforces strict layering. Violations will be rejected:

| Layer       | May import                              | Must not import                    | Contains `unsafe` |
|-------------|-----------------------------------------|------------------------------------|-------------------|
| `boot.rs`   | `core`                                  | `hal/`, `runtime/`, `math/`, `ui/` | Yes (memory init) |
| `hal/`      | `core`, other `hal/` submodules         | `runtime/`, `math/`, `ui/`         | Yes (MMIO only)   |
| `runtime/`  | `hal/`, `math/`, `ui/`                  | —                                  | No                |
| `math/`     | `core` only                             | `hal/`, `runtime/`, `ui/`          | No                |
| `ui/`       | `hal/` (framebuffer type), `core`       | `runtime/`, `math/`                | No                |

## Testing

### Math engine on host

The `math/` layer is hardware-independent and can be compiled for the host:

```bash
# Compile just the math module for testing on your development machine
rustc --edition 2021 src/math/fixed_point.rs --crate-type lib

# Or set up a test binary in a workspace member
```

We do not yet have an automated test suite. When adding math functions:

1. Verify the result against Python (use `Decimal` or `int(value * 2**32)` for Q31.32)
2. Test edge cases: zero, negative values, overflow boundaries, domain errors
3. For probability functions, test known values from statistical tables

### Firmware integration

Run in QEMU and pipe test inputs:

```bash
echo "sin(pi/2)" | cargo run --release
# Expected output: = 1
```

Compare against `test_inputs.txt`:

```bash
cat test_inputs.txt | cargo run --release
```

### Manual QEMU testing

```bash
cargo build --release && qemu-system-arm \
  -M lm3s811evb \
  -serial mon:stdio \
  -display gtk \
  -kernel target/thumbv7m-none-eabi/release/NumCore
```

## Pull request process

1. Create a feature branch from `main`
2. Make your changes, following the conventions above
3. Run `cargo build --release` and verify it compiles
4. Run in QEMU and verify existing functionality still works
5. Open a PR with a clear description of:
   - What the change does
   - Why it's needed
   - How it was tested
   - Any layer-rule implications

### What to include in your PR

- If adding a HAL feature: include the relevant register constants and bit masks
- If adding a math function: include test vectors in the PR description (for `sto`, verify the register holds the stored value in a subsequent expression)
- If touching the parser: include example expressions that exercise the new grammar (including implicit multiplication: `3(5)`, `(a)b`, `2sin(x)`, `sto()`, case-sensitive identifier edge cases)
- If adding UI rendering: include a screenshot or ASCII-art of the display output

## Getting help

Open an issue on GitHub for bugs, questions, or feature requests. For architecture questions, reference `docs/ARCHITECTURE.md` and the module-level doc comments in the source code. For the project roadmap and long-term vision, see `ROADMAP.md`.
