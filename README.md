# NumCore
A bare-metal scientific calculator firmware written in Rust, targeting the
**ARM Cortex-M3 LM3S811** microcontroller, developed and tested entirely under
QEMU emulation.

No operating system. No heap allocator. No external crates. Every layer -
from the interrupt vector table to the statistical distribution functions - is
written from scratch.

---

## Table of contents

- [Features](#features)
- [Architecture](#architecture)
- [Project structure](#project-structure)
- [Supported expressions](#supported-expressions)
- [Memory layout](#memory-layout)
- [Prerequisites](#prerequisites)
- [Building](#building)
- [Running in QEMU](#running-in-qemu)
- [Debugging with GDB](#debugging-with-gdb)
- [Continuous integration](#continuous-integration)
- [Software philosophy](#software-philosophy)
- [Contributing](#contributing)
- [Roadmap](#roadmap)

---

## Features

- **Full PEMDAS** - operator precedence and right-associative exponentiation
- **Full trigonometry** - sin, cos, tan, asin, acos, atan with CORDIC
  quadrant folding across all four quadrants
- **Logarithms and exponentials** - ln, log₁₀, log₂, exp with range
  reduction for large arguments
- **Nth root** - `nthroot(x, n)` for any real n, including fractional roots
- **Rounding** - floor, ceil, round (correct for negative numbers)
- **Statistical distributions** - binomial, Poisson, chi-squared CDF
- **Log-gamma function** - Lanczos approximation with exact half-integer path
- **Symbolic summation** - `sum(body, var, start, end)`
- **Numerical integration** - `int(body, var, a, b)` via composite
  Simpson's rule (200 intervals), with automatic snap-to-integer
- **26 variable registers** - `A`–`Z` plus `Ans`
- **Portable math engine** - the entire `src/math/` layer compiles and
  runs tests on a standard PC with zero changes

---

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│  Layer 8 - Calculator Modes        src/modes/  [roadmap] │
├──────────────────────────────────────────────────────────┤
│  Layer 7 - UI / Interaction        src/ui/     [roadmap] │
├──────────────────────────────────────────────────────────┤
│  Layer 6 - Math Engine             src/math/             │
├──────────────────────────────────────────────────────────┤
│  Layer 5 - Core Runtime            src/runtime/          │
├──────────────────────────────────────────────────────────┤
│  Layer 4 - Hardware Abstraction    src/hal/              │
├──────────────────────────────────────────────────────────┤
│  Layer 3 - Boot & Startup          src/boot.rs           │
├──────────────────────────────────────────────────────────┤
│  Layer 2 - CPU / Cortex-M3         (silicon / QEMU)      │
├──────────────────────────────────────────────────────────┤
│  Layer 1 - Physical Hardware       LM3S811EVB            │
└──────────────────────────────────────────────────────────┘
```

**The one rule that never breaks:** nothing above the HAL touches a hardware
register directly. Every peripheral access goes through a named HAL function.
The entire math engine, runtime, and future UI code are therefore fully
portable - only `src/hal/` changes when the target board changes.

---

## Project structure

```
NumCore/
├── src/
│   ├── main.rs                  - crate root, module declarations, panic handler
│   ├── boot.rs                  - vector table, Reset handler, .data/.bss init
│   │
│   ├── hal/                     - Layer 4: the only code that touches registers
│   │   ├── mod.rs               - re-exports the HAL public surface
│   │   ├── mmio.rs              - read_register/write_register (all unsafe lives here)
│   │   ├── clock.rs             - SysCtl clock gating, SYSTEM_CLOCK_HZ
│   │   ├── gpio.rs              - pin direction, alternate function, open-drain
│   │   ├── uart.rs              - UART0: 115200-8-N-1, blocking TX and RX
│   │   └── i2c.rs               - I2C0 driver stub (ready for OLED display)
│   │
│   ├── runtime/                 - Layer 5: event loop, state machine
│   │   ├── mod.rs               - start(), event loop, input handlers
│   │   ├── state.rs             - CalcState: input buffer, variables, scratch buffers
│   │   └── event.rs             - CalcEvent enum, raw byte → event translation
│   │
│   └── math/                    - Layer 6: fully hardware-independent
│       ├── mod.rs               - module declarations
│       ├── fixed_point.rs       - Q31.32 internals: CORDIC trig, ln, exp
│       ├── distributions.rs     - lnGamma (Lanczos), binomial, Poisson, chi-squared
│       ├── vars.rs              - VariableStore: Ans + registers A–Z
│       ├── lexer.rs             - expression bytes → Token stream
│       ├── parser.rs            - tokens → AST (recursive descent, full precedence)
│       ├── evaluator.rs         - AST → CalcNum (tree walker, loop evaluation)
│       └── engine.rs            - public API: evaluate_expression(), format_result()
│
├── link.x                       - linker script: Flash 64 KB, SRAM 8 KB
├── Cargo.toml                   - zero external dependencies
├── rust-toolchain.toml          - pins stable + thumbv7m-none-eabi
├── .cargo/config.toml           - default target, -Tlink.x linker flag
├── .github/workflows/ci.yml     - build + QEMU integration test
├── README.md                    - this file
├── CONTRIBUTING.md              - rules, style guide, PR checklist
└── ROADMAP.md                   - OLED, UI layer, scientific mode plan
```

---

## Supported expressions

### Operators

| Syntax | Operation |
|--------|-----------|
| `+` `-` `*` `/` | Arithmetic (full PEMDAS) |
| `^` | Exponentiation, right-associative (`2^3^4` = `2^(3^4)`) |
| `%` | Modulo |
| `-x` | Unary negation |
| `(` `)` | Grouping |

### Single-argument functions

| Function | Description |
|----------|-------------|
| `sin(x)` `cos(x)` `tan(x)` | Trigonometry - argument in **radians** |
| `asin(x)` `acos(x)` `atan(x)` | Inverse trig - result in radians |
| `sinh(x)` `cosh(x)` `tanh(x)` | Hyperbolic funcs |
| `asinh(x)` `acosh(x)` `atanh(x)` | Inverse Hyperbolic funcs |
| `deg(x)` | Degrees → radians: `sin(deg(90))` = 1 |
| `rad(x)` | Radians → degrees: `rad(pi)` = 180 |
| `sqrt(x)` | Square root |
| `abs(x)` | Absolute value |
| `exp(x)` | eˣ |
| `ln(x)` | Natural logarithm |
| `log(x)` | Log base 10 |
| `log2(x)` | Log base 2 |
| `floor(x)` `ceil(x)` `round(x)` | Rounding |
| `lnGamma(x)` | ln(Γ(x)) - log-gamma function |

### Two-argument functions

| Function | Description |
|----------|-------------|
| `nthroot(x, n)` | nth root: `nthroot(8, 3)` = 2 |
| `poissonP(λ, k)` | P(X=k) for X ~ Poisson(λ) |
| `chiCDF(x, k)` | P(X≤x) for X ~ χ²(k degrees of freedom) |

### Three-argument functions

| Function | Description |
|----------|-------------|
| `binomP(n, k, p)` | P(X=k) for X ~ Binomial(n, p) |

### Loop aggregates

The loop variable must be a register letter A–Z (not E, which is Euler's
number).

| Function | Description |
|----------|-------------|
| `sum(body, var, start, end)` | Σ body for var = start … end (integer steps) |
| `int(body, var, a, b)` | ∫ body dvar from a to b (Simpson's rule, n=200) |

### Constants and variables

| Name | Value |
|------|-------|
| `pi` | 3.141592654 |
| `e` | 2.718281828 |
| `Ans` | Result of the most recent successful evaluation |
| `A`–`Z` | General-purpose registers (except `E`) |

### Example expressions

```
2 + 3 * 4                        → 14
(2 + 3) * 4                      → 20
99^5                              → 9509900499
2^64                              → 1.844674407e19
sin(pi/2)                        → 1
sin(deg(30))                     → 0.5
sqrt(2)                          → 1.414213562
nthroot(27, 3)                   → 3
ln(e)                            → 1
log(1000)                        → 3
(sin(A))^2 + (cos(A))^2          → 1   (for any A)
sum(K^2, K, 1, 10)               → 385
int(sin(X), X, 0, pi)            → 2   (exact, snapped)
int(X^2, X, 0, 1)                → 0.333333333
binomP(10, 3, 0.5)               → 0.117187500
poissonP(3, 2)                   → 0.224042436
chiCDF(3.841, 1)                  → 0.949986349
5 * 6                            → 30
Ans + 2                          → 32
```

---

## Memory layout

| Region | Base address | Size |
|--------|-------------|------|
| Flash | `0x0000_0000` | 64 KB |
| SRAM | `0x2000_0000` | 8 KB |

Key SRAM residents:

| Item | Approximate size |
|------|-----------------|
| `CalcState.input_buffer` | 64 B |
| `CalcState.lex_scratch` (32 tokens × 20 B) | ~640 B |
| `CalcState.parse_scratch` (64 nodes × 24 B) | ~1536 B |
| Stack (grows down from top of SRAM) | ~5.3 KB |

The lexer and parser scratch buffers live as fields of `CalcState` in static
RAM. Without this arrangement, a single evaluation call would stack-allocate
~2 KB and immediately overflow the 8 KB device.

---

## Prerequisites

| Tool | Install |
|------|---------|
| Rust stable | `curl https://sh.rustup.rs -sSf \| sh` |
| `thumbv7m-none-eabi` target | `rustup target add thumbv7m-none-eabi` |
| `qemu-system-arm` | `apt install qemu-system-arm` / `brew install qemu` |
| `arm-none-eabi-gdb` (optional) | `apt install gdb-arm-none-eabi` |

`rust-toolchain.toml` pins the toolchain and target - `rustup` installs them
automatically on the first `cargo build`.

---

## Building

```sh
cargo build --release
```

Output: `target/thumbv7m-none-eabi/release/calculator-fw`

| Profile | Command | Notes |
|---------|---------|-------|
| Release | `cargo build --release` | `-Oz` + LTO - fits in 64 KB Flash |
| Debug | `cargo build` | Unoptimised - may exceed Flash |

Both profiles use `panic = "abort"` to eliminate unwinding machinery.

---

## Running in QEMU

```sh
qemu-system-arm \
    -M lm3s811evb \
    -nographic \
    -serial mon:stdio \
    -kernel target/thumbv7m-none-eabi/release/NumCore
```

Type any expression at the `>` prompt and press Enter. Exit with `Ctrl-A X`.

### Flag reference

| Flag | Purpose |
|------|---------|
| `-M lm3s811evb` | Emulate the LM3S811 evaluation board |
| `-nographic` | Disables default QEMU GUI |
| `-serial mon:stdio` | Creates multiplexed stdio backend connected to serial port and QEMU monitor |
| `-kernel <elf>` | Load and run the firmware ELF |

---

## Debugging with GDB

```sh
# Terminal 1 - QEMU paused, GDB server on port 1234
qemu-system-arm \
    -M lm3s811evb \
    -nographic \
    -serial mon:stdio \
    -kernel target/thumbv7m-none-eabi/release/NumCore \
    -s -S

# Terminal 2
arm-none-eabi-gdb target/thumbv7m-none-eabi/release/NumCore \
    -ex "target remote :1234" \
    -ex "break Reset" \
    -ex "continue"
```

Useful GDB commands:

```
break runtime::start              # break when firmware enters the event loop
break evaluator::evaluate_node    # break on every AST node evaluation
info registers                    # Cortex-M register dump
x/10i $pc                         # disassemble around the current instruction
```

---

## Continuous integration

`.github/workflows/ci.yml` runs on every push and pull request:

1. Installs stable Rust and the `thumbv7m-none-eabi` target
2. Builds the firmware in release mode
3. Starts the firmware in QEMU and pipes test inputs via stdin with timing
   delays
4. Asserts expected strings appear in the output

```sh
# How inputs are fed to the running firmware
(sleep 1
 echo "2+2";      sleep 0.3
 echo "sin(pi)";  sleep 0.3
 echo "(2+3)/10"; sleep 0.3
) | timeout 10s qemu-system-arm \
      -M lm3s811evb -display none -monitor null -serial stdio \
      -kernel target/thumbv7m-none-eabi/release/NumCore
```

The initial `sleep 1` gives UART initialisation time to complete before the
first expression arrives. The `sleep 0.3` gaps ensure each result is fully
transmitted before the next expression is sent.

To add a test case, add an `echo` + `sleep` line to the input block and a
`check` call for the expected output string.

---

## Software philosophy

Ten principles applied consistently throughout this codebase:

1. **Intention-revealing names** - a name should communicate purpose without
   requiring the reader to inspect the implementation
2. **No disinformation** - names must never mislead about what a value holds
   or what a function does
3. **Meaningful distinctions** - if two things have different names, they are
   meaningfully different
4. **Pronounceable names** - if you cannot say it, you cannot discuss it in
   a code review
5. **Searchable names** - single-letter identifiers appear only as loop
   variables in tight, obvious scopes
6. **No type encoding** - `uart_u8_byte` adds noise; `byte` is sufficient
7. **No mental mappings** - the code is written for the next reader, not for
   the author who already holds the context
8. **Comments on everything** - every function, constant, and non-obvious
   expression has a comment explaining *why*, not just *what*
9. **Modularity and browsability** - the module tree reflects the conceptual
   layer diagram; related things are together, unrelated things are apart
10. **Rust naming conventions** - `snake_case` for functions and variables
    (compiler-enforced), `CamelCase` for types, `SCREAMING_SNAKE_CASE` for
    constants

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full guide, including:

- The HAL boundary rule (nothing above Layer 4 touches registers)
- How to add a new mathematical function end-to-end
- How to add a new HAL peripheral
- How to run the math unit tests on a host PC (no QEMU needed)
- The pull request checklist

---

## Roadmap

Full details in [ROADMAP.md](ROADMAP.md). Upcoming phases:

**Phase 1 - OLED display**

The LM3S811EVB carries an OSRAM Pictiva 96×16 OLED with an SSD0303
controller on the I2C bus. `hal/i2c.rs` and the full UI layer architecture
are already designed. Remaining work: complete the I2C driver, implement the
SSD0303 init sequence and framebuffer flush, write the 5×8 bitmap font, build
the canvas and renderer, and wire the renderer into the event loop.

**Phase 2 - Code size optimisation**

The firmware is approaching the 64 KB Flash limit in release mode. Planned
work: dead code elimination, merging small functions, and evaluating shared
lookup tables across modules.

**Phase 3 - Scientific mode**

Hyperbolic trig, complex exponentiation, unit conversion, and matrix
operations behind a mode-switch key, using the `ModeHandler` trait in
`src/modes/mod.rs`.

**Phase 4 - Physical hardware**

Deploying to real LM3S811EVB silicon with OLED output and a physical keypad
replacing the UART terminal.
