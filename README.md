# NumCore

Bare-metal scientific calculator firmware for the **LM3S811** ARM Cortex-M3 microcontroller, written entirely in Rust with `#![no_std]` and `#![no_main]`. Features a complete fixed-point math engine, an interactive UART console, and an I2C-driven OLED display.

## Features

### Expression evaluation
- Full recursive-descent parser with **PEMDAS precedence**
- **Right-associative exponentiation** (`2^2^3` = 256, not 64)
- Parenthesised sub-expressions, implicit precedence via grammar
- 64-character input buffer, 32-token lexer budget, 64-node AST arena

### Mathematical functions

**Operators:** `+` `-` `*` `/` `^` `%`

**Trigonometry** (all functions take/return radians):
| Function | Description |
|----------|-------------|
| `sin(x)` `cos(x)` `tan(x)` | Standard trig |
| `asin(x)` `acos(x)` `atan(x)` | Inverse trig |
| `sinh(x)` `cosh(x)` `tanh(x)` | Hyperbolic |
| `asinh(x)` `acosh(x)` `atanh(x)` | Inverse hyperbolic |

**Powers and roots:**
| Function | Description |
|----------|-------------|
| `sqrt(x)` | Square root (`x ≥ 0`) |
| `nthroot(x,n)` | n-th root with domain checking |
| `x ^ y` | Arbitrary power |

**Logarithms and exponentials:**
| Function | Description |
|----------|-------------|
| `exp(x)` | `e^x` |
| `ln(x)` | Natural logarithm (`x > 0`) |
| `log(x)` | Base-10 logarithm (`x > 0`) |
| `log2(x)` | Base-2 logarithm (`x > 0`) |

**Rounding and special:**
| Function | Description |
|----------|-------------|
| `floor(x)` `ceil(x)` `round(x)` | Integer rounding |
| `abs(x)` | Absolute value |
| `deg(x)` | Degrees → radians |
| `rad(x)` | Radians → degrees |

**Statistical distributions** (all computed in log space to avoid overflow):
| Function | Description |
|----------|-------------|
| `binomp(n,k,p)` | Binomial probability `P(X=k)` |
| `poissonp(λ,k)` | Poisson probability `P(X=k)` |
| `chicdf(x,k)` | Chi-squared CDF `P(X≤x)` with `k` d.f. |
| `lngamma(x)` | Log-gamma `ln(Γ(x))` |

**Loop aggregates:**
| Function | Description |
|----------|-------------|
| `sum(expr, var, start, end)` | Summation Σ over integer range |
| `int(expr, var, a, b)` | Numeric integration via Simpson's rule |

### Constants and variables
- **Built-in constants:** `pi`, `e` (with 9-digit Q31.32 precision)
- **Ans:** automatically stores the last result
- **Registers A–Z:** 26 user-writable storage registers

### Q31.32 fixed-point engine
- **i64** storage: 32 integer bits (signed) + 32 fractional bits
- **±2³¹ range**, **~2.33×10⁻¹⁰ precision** (~9 decimal digits)
- **i128 intermediates** for multiplication to prevent overflow
- **CORDIC** for sin/cos/atan (24 iterations, full precision)
- **Taylor series** for exp (12 terms) and ln (20 terms, with range reduction to [1/√2, √2))
- Zero `unsafe`, zero HAL imports, zero heap — pure stack-allocated arithmetic

### Hardware interface
- **UART console**: Through terminal emulator on serial0, type expressions and see results
- **96×16 OLED display**: I2C-driven OSRAM Pictiva SSD0303 with 5×7 bitmap font
- Pretty-printed formulas with π glyph, ×÷− symbols, and tall ∫/Σ notation
- **Scratch buffers** pre-allocated in static RAM to avoid stack overflow on 8 KB SRAM
- No heap, no allocator, no OS — fully deterministic, all memory static

## Safety contract

NumCore enforces a strict **hardware access boundary**:

- **`hal/`** — the **only** layer permitted to touch hardware registers directly. All `unsafe` for MMIO access is confined to `hal/mmio.rs`. Every `unsafe` block has a `// SAFETY:` justification.
- **`boot.rs`** — the only other file with `unsafe`, strictly for RAM initialisation (`.bss`/`.data`) before the HAL is online.
- **Everything else** — `runtime/`, `math/`, `ui/` — contains zero `unsafe`. They interact with hardware exclusively through the HAL's safe public API.

This design means porting to a new MCU only requires rewriting `hal/` (and possibly `boot.rs`). The math engine, event loop, and UI remain completely untouched.

## Portability

The current firmware targets the **Luminary Micro Stellaris LM3S811** (ARM Cortex-M3) for testing and development. The strict layered architecture is explicitly designed to enable porting to many different architectures and microprocessors:

| Layer     | MCU-specific | Portable          |
|-----------|-------------|-------------------|
| `boot.rs` | Yes — vector table format | — |
| `hal/`    | Yes — register maps, peripherals | — |
| `runtime/`| No          | Event loop, state machine |
| `math/`   | No          | Entire math engine |
| `ui/`     | No          | Font, formula renderer |

To port: rewrite `hal/` for the new MCU, update `boot.rs` to match the new vector table and memory layout, and optionally adjust `link.x`. Nothing else changes.

## Quick start

### Prerequisites

```bash
rustup target add thumbv7m-none-eabi
```

### Build

```bash
cargo build --release
```

The resulting ELF binary lives at `target/thumbv7m-none-eabi/release/NumCore`.

### Run in QEMU

**Terminal (UART on stdio):**
```bash
qemu-system-arm \
  -M lm3s811evb \
  -serial mon:stdio \
  -display gtk \
  -kernel target/thumbv7m-none-eabi/release/NumCore
```

**Windowed (UART on virtual console):**
```bash
qemu-system-arm \
  -M lm3s811evb \
  -serial vc \
  -display gtk \
  -kernel target/thumbv7m-none-eabi/release/NumCore
```

Switch to the serial virtual console from QEMU's **View** menu to type expressions.

### Batch test

```bash
cat test_inputs.txt | qemu-system-arm \
  -M lm3s811evb \
  -serial mon:stdio \
  -display none \
  -kernel target/thumbv7m-none-eabi/release/NumCore
```

## Hardware target

| Property        | Value                          |
|-----------------|--------------------------------|
| MCU             | LM3S811 (ARM Cortex-M3 r1p1)  |
| Flash           | 64 KB                          |
| SRAM            | 8 KB                           |
| System clock    | 12 MHz (internal oscillator)   |
| UART            | 115200-8-N-1 on PA0/PA1       |
| I2C             | 100 kHz on PB2/PB3            |
| Display         | OSRAM Pictiva 96×16 (SSD0303) |

## Further reading

| Document | Contents |
|----------|----------|
| `docs/ARCHITECTURE.md` | Full layered architecture, data flow, memory layout, design decisions |
| `docs/CONTRIBUTING.md` | Development setup, coding standards, testing, PR process |
| `docs/HACKING.md`      | Day-to-day commands, QEMU tips, debugging, adding functions/peripherals |
| `ROADMAP.md`           | Future development plans and milestones |
