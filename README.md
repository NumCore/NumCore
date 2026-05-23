# NumCore

Bare-metal scientific calculator firmware for the **LM3S811** ARM Cortex-M3 microcontroller, written entirely in Rust with `#![no_std]` and `#![no_main]`. Features a complete fixed-point math engine, an interactive UART console, and an I2C-driven OLED display.

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

## Features

- **Q31.32 fixed-point arithmetic** — 9 decimal digits of precision, ±2³¹ range, no floating-point hardware required
- **Expression evaluator** — full recursive-descent parser with PEMDAS precedence and right-associative exponentiation
- **70+ built-in functions** — trig (sin/cos/tan + inverses + hyperbolics), log/ln/log2/exp, floor/ceil/round, deg/rad conversion
- **Statistical distributions** — binomial probability, Poisson probability, chi-squared CDF, log-gamma function
- **Loop aggregates** — summation (Σ) and numeric integration (∫ via Simpson's rule) over register-bound variables
- **26 variable registers** (A–Z) plus automatic `Ans` storage
- **Interactive UART terminal** — type expressions, get results at 115200 baud
- **96×16 OLED display** — I2C-driven OSRAM Pictiva SSD0303, with 5×7 bitmap font and pretty-printed formulas
- **No heap, no allocator, no OS** — all memory statically allocated, deterministic execution

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
| `docs/CONTRIBUTING.md` | Development setup, coding standards, testing, PR process, roadmap |
| `docs/HACKING.md`      | Day-to-day commands, QEMU tips, debugging, adding functions/peripherals |

## Project status

- [x] Boot layer: vector table, `.bss`/`.data` init, Reset handler
- [x] HAL: UART, I2C, GPIO, clock configuration, OLED driver
- [x] Runtime: event loop, state machine, input handling
- [x] Math engine: Q31.32 fixed-point, lexer, parser, evaluator
- [x] UI: 5×7 bitmap font, formula renderer, Σ/∫ glyphs
- [ ] Graphing mode
- [ ] Unit tests on host (math engine already hardware-independent)
- [ ] Advanced calculator modes (scientific, graphing)
