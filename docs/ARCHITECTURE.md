# Architecture

NumCore is organised as a strict **layered architecture**. Each layer has well-defined responsibilities and import rules enforced at the Cargo crate boundary.

## Safety contract

NumCore's safety model is explicit and auditable:

1. **HAL crate** (`hal-lm3s811/`) is the **only** crate permitted to perform memory-mapped I/O. All `unsafe` for hardware register access is confined to `mmio.rs` (two functions: `read_register` and `write_register`). Every other HAL module calls through these primitives — no module outside `mmio.rs` issues raw pointer reads or writes.

2. **`numcore-<mcu>/src/boot.rs`** uses `unsafe` for a single, narrow purpose: zeroing `.bss` and copying `.data` from Flash to RAM before the HAL or any Rust code can run. This is unavoidable on bare metal — there is no OS loader to do it.

3. **`runtime/`, `math/`, and `ui/` contain zero `unsafe` code.** They interact with hardware exclusively through the `Uart` and `Display` traits defined in `numcore::hal`. This is verified by inspection and enforced at the crate boundary.

4. **Every `unsafe` block** in the codebase has an adjacent `// SAFETY:` comment explaining why the invariants hold.

Porting to a new MCU means auditing and rewriting only the HAL crate and boot crate. Everything in `numcore/` is architecture-agnostic.

## Cargo workspace structure

The project is a Cargo workspace with four members:

| Member | Path | Target | Purpose |
|--------|------|--------|---------|
| `numcore` | `numcore/` | any (host or embedded) | MCU-agnostic lib crate: traits, math engine, runtime, UI |
| `numcore-lm3s811` | `numcore-lm3s811/` | `thumbv7m-none-eabi` | Per-MCU binary crate for LM3S811 |
| `hal-lm3s811` | `hal-lm3s811/` | `thumbv7m-none-eabi` | HAL implementation + trait impls for LM3S811 |
| `numcore_math` | `test-suite/` | Host (e.g. `x86_64`) | Host-side unit tests for the math engine |

The shared crate (`numcore/`) depends on no HAL crate. Each per-MCU binary depends on `numcore` and its corresponding HAL crate:

```toml
# numcore-lm3s811/Cargo.toml
[dependencies]
numcore = { path = "../numcore" }
hal-lm3s811 = { path = "../hal-lm3s811" }
```

Each HAL crate depends on `numcore` to access the trait definitions:

```toml
# hal-lm3s811/Cargo.toml
[dependencies]
numcore = { path = "../numcore" }
```

The workspace root `.cargo/config.toml` does **not** set a default build target. All firmware commands require `--target thumbv7m-none-eabi`. The test-suite compiles for the host by default. Use `make build` / `make test` for convenience.

## Portability

The firmware is currently developed and tested on the **Luminary Micro Stellaris LM3S811** (ARM Cortex-M3, 64 KB Flash, 8 KB SRAM). The layered design is explicitly engineered to support future ports to other architectures and microprocessors:

- **`numcore/src/hal.rs`** — `Uart` and `Display` traits. This is the only HAL dependency of the shared code.
- **`math/`** — zero HAL imports, zero `unsafe`, zero platform dependencies. Compiles on any target Rust supports. The `test-suite/` workspace member includes `numcore/src/math/` sources via `#[path]` and runs 250 automated tests on the host.
- **`runtime/`** — generic over `<U: Uart, D: Display>`. Touches hardware only through trait method calls.
- **`ui/`** — generic over `<D: Display>`. Renders to `D::Buffer` via `AsMut<[u8]>` / `AsRef<[u8]>`.
- **HAL crate** (`hal-<mcu>/`) — the only crate that needs rewriting per target. Peripheral register maps, clock trees, and pin muxing are encapsulated here. Must implement `numcore::hal::Uart` and `numcore::hal::Display`.
- **Per-MCU binary crate** (`numcore-<mcu>/`) — contains `boot.rs` (vector table, Reset handler) and `link.x` (memory map). These depend on the MCU's memory layout.

A port to a new architecture therefore involves: writing a new HAL crate (with trait impls), creating a new binary crate (with `boot.rs` + `link.x`), and adding the target triple. No application logic changes.

## Layer map

```
  ┌─────────────────────────────────────────────┐
  │  Layer 8:  modes/              [ROADMAP]    │
  │  Standard, Scientific, Graphing modes       │
  ├─────────────────────────────────────────────┤
  │  Layer 7:  ui/  (numcore/src/ui/)           │
  │  OLED rendering, font, formula pretty-print │
  ├─────────────────────────────────────────────┤
  │  Layer 6:  math/  (numcore/src/math/)       │
  │  Fixed-point, lexer, parser, evaluator,     │
  │  variables, distributions                   │
  ├─────────────────────────────────────────────┤
  │  Layer 5:  runtime/  (numcore/src/runtime/) │
  │  Event loop, state machine, CalcState,      │
  │  event dispatch                             │
  ├─────────────────────────────────────────────┤
  │  Layer 4:  HAL crate (hal-<mcu>/)           │
  │  UART, I2C, GPIO, clock, OLED driver,       │
  │  MMIO primitives (only crate with unsafe)   │
  ├─────────────────────────────────────────────┤
  │  Layer 3:  boot.rs (numcore-<mcu>/src/)     │
  │  Vector table, Reset handler, .bss/.data    │
  └─────────────────────────────────────────────┘
```

## Layer details

### Layer 3 — Boot (`numcore-<mcu>/src/boot.rs`)

The lowest software layer. Executes before any Rust code can safely run. One `boot.rs` file per MCU, lives in the per-MCU binary crate.

**Responsibilities:**
- Place the Cortex-M vector table at Flash address `0x0000_0000`
- Define the `Reset` handler (true entry point after power-on)
- Zero-initialise the `.bss` section (all uninitialised statics)
- Copy the `.data` section from Flash LMA to RAM VMA
- Jump to `crate::start()` (thin wrapper in `main.rs`) — never returns

**Rules:**
- `unsafe` is permitted here **only** for raw-pointer memory initialisation
- No application logic belongs here
- No HAL calls — the HAL is initialised by the runtime later
- Must compile correctly even if every other module changes

**Exception vectors:**
- Slot 0: Initial stack pointer (top of 8 KB SRAM, minus 2 KB stack)
- Slot 1: Reset vector → `Reset()` function
- Slots 2–15: All route to `DefaultHandler` (spin loop) — upgrade individually as needed (SysTick, SVCall, fault handlers)

### Layer 4 — Hardware Abstraction Layer (separate crate: `hal-<mcu>/`)

The **only** crate permitted to touch hardware registers directly. All `unsafe` for MMIO access is confined to `mmio.rs`.

Must implement `numcore::hal::Uart` and `numcore::hal::Display`.

**Modules:**

| Module     | Contents                                                    |
|------------|-------------------------------------------------------------|
| `mmio.rs`  | `read_register`, `write_register`, `set_register_bits`      |
| `uart.rs`  | UART0 initialisation, `transmit_byte`, `transmit_bytes`, `receive_byte_blocking`, `poll_byte` |
| `i2c.rs`   | I2C0 initialisation, `send_byte`, `send_bytes`              |
| `gpio.rs`  | GPIO port base addresses, alternate function config, open-drain config |
| `clock.rs` | System clock frequency, RCGC1/RCGC2 clock gating, spin-loop delay |
| `oled.rs`  | SSD0303 OLED driver: initialisation, `clear_display`, `render_screen`, `set_pixel` |

**Hardware configuration:**

- **UART0** at `0x4000_C000`: 115200-8-N-1, PA0=RX, PA1=TX. Baud-rate divisors derived from 12 MHz system clock: IBRD=6, FBRD=33.
- **I2C0** at `0x4002_0000`: 100 kHz standard mode, PB2=SCL, PB3=SDA. TPR=5 for 12 MHz.
- **GPIOA** at `0x4000_4000`: pins PA0/PA1 configured as alternate-function digital for UART0.
- **GPIOB** at `0x4000_5000`: pins PB2/PB3 configured as alternate-function open-drain for I2C0.
- **SSD0303 OLED** at I2C address `0x3D`: 96×16 monochrome, 2 pages × 96 columns, command framing uses `0x80/0x40` control bytes.

**Rules:**
- `unsafe` is permitted **only** inside `hal-*/` crate implementation files
- All public HAL functions **must** have safe signatures — callers never see `unsafe`
- No HAL module may import from `runtime/`, `math/`, `ui/`, or `modes/`
- HAL modules may import from each other (e.g. `uart` imports `mmio`, `gpio`, `clock`)
- Must implement `numcore::hal::Uart` and `numcore::hal::Display`

### Layer 5 — Runtime (`numcore/src/runtime/`)

The control centre of the firmware. Generic over `<U: Uart, D: Display>`. Sits between the HAL crate and the application layers. Contains zero `unsafe` code.

**Modules:**

| Module    | Contents                                                  |
|-----------|-----------------------------------------------------------|
| `mod.rs`  | `start::<U, D>()`, hardware init sequence, event loop, event handlers, OLED rendering glue |
| `state.rs`| `CalcState` — owns input buffer, variable store, scratch buffers, active mode |
| `event.rs`| `CalcEvent` enum, `translate_input_byte_to_event()` — ASCII byte → typed event |

**Startup sequence:**
1. `boot::Reset()` → `.bss`/`.data` init → `crate::start()` → `numcore::runtime::start::<U, D>()`
2. `U::init()` → `D::init()` (which initiliases I2C + OLED)
3. `D::render(&D::new_buffer())` — clear display
4. Print welcome banner via `U::transmit_bytes()`
5. `run_event_loop()` — block on `U::poll_byte()`, dispatch events

**Event handling:**
- Printable ASCII (`0x20`–`0x7E`) → insert at cursor position in input buffer and echo via `U::transmit_byte()`
- CR/LF (`0x0D`/`0x0A`) → submit expression for evaluation
- BS/DEL (`0x08`/`0x7F`) → delete character before cursor
- Escape (`0x1B`) → toggle between Standard and Advanced math modes
- ANSI escape sequences (`Esc[D` / `Esc[C`) → cursor left/right navigation within input buffer (detected by a 3-byte state machine in the event loop)
- Scrolling: long results display left/right arrow sentinel glyphs when the formatted result exceeds 13 characters; arrow keys scroll the viewport

**Memory management:**
- `CalcState` is a single `static mut` allocated once in `.bss`
- The lexer scratch buffer (`LexResult`, ~256 bytes) and parser scratch buffer (`ParseTree`, ~1 KB) live inside `CalcState` — never on the stack
- This avoids stack overflow on the 8 KB SRAM (2 KB reserved for stack)

**Rules:**
- Contains zero `unsafe` — every hardware interaction goes through `U::*` / `D::*` trait calls
- Owns and updates `CalcState` (including the variable store)
- Routes input events to handlers, triggers UI re-renders via `formula::render_screen::<D>()`

### Layer 6 — Math Engine (`numcore/src/math/`)

Completely hardware-independent. Can be compiled and tested on any platform. Zero `unsafe` code, zero HAL imports, zero heap allocation.

The math engine is tested via the `test-suite/` workspace member, which includes every `numcore/src/math/` source file via `#[path]` attributes and compiles them for the host. 250 automated tests cover fixed-point arithmetic, lexer, parser, evaluator, variables, distributions, complex numbers, and the full expression pipeline. Run with `cargo test -p numcore_math --tests` or `make test`.

**Pipeline:**

```
expression bytes
    → lexer::tokenise_expression()   → Token stream
    → parser::parse_token_stream()   → AST (flat arena)
    → evaluator::evaluate_tree()     → Q31.32 result
```

**Modules:**

| Module           | Contents                                                     |
|------------------|--------------------------------------------------------------|
| `fixed_point.rs` | Q31.32 arithmetic: multiply, divide, sqrt, trig (CORDIC), exp (Taylor), log (Taylor), rounding, formatting |
| `lexer.rs`       | Expression string → typed `Token` stream (32-token budget)  |
| `parser.rs`      | Token stream → `AstNode` arena (64-node budget), recursive-descent with precedence climbing |
| `evaluator.rs`   | AST → Q31.32 result, operator/function dispatch, `sto()` register write, loop aggregate evaluation |
| `engine.rs`      | Public API: `evaluate_expression()`, `format_result()`       |
| `vars.rs`        | `VariableStore`: Ans + 26 registers (uppercase A–Z), `Copy` for loop-variable shadowing |
| `distributions.rs`| `ln_gamma`, `ln_factorial`, `binomial_probability`, `poisson_probability`, `chi_squared_cdf` |

**Fixed-point format (Q31.32):**
- Stored as `i64`: 32 integer bits (signed), 32 fractional bits
- Scale factor: `2³² = 4_294_967_296`
- Precision: `1/2³² ≈ 2.33 × 10⁻¹⁰` (~9 correct decimal digits)
- Intermediate multiplication uses `i128` to hold the full Q31.64 product
- All arithmetic in CPU registers — no extra RAM cost beyond the `i64` storage

**CORDIC implementation:**
- 24 iterations for sin/cos with full Q31.32 precision
- Angle reduction to [−π, π] with quadrant folding
- Exact-table lookup for multiples of 30° and 45°
- `atan` uses CORDIC vectoring mode

**Lexer rules:**
- Identifiers are **case-sensitive**. Function names and constants (`sin`, `pi`, `e`, `ans`, `sto`) are all lowercase. Single uppercase letters A–Z are variable registers. Single lowercase letters are unrecognised.
- `e` → Euler's constant; `E` → variable register E. Previously identifiers were lowercased, making `e` and `E` indistinguishable.

**Parser grammar:**
```
expression  =  term   ( ( '+' | '−' ) term )*
term        =  power  ( ( '*' | '/' | '%' | implicit_mult ) power )*
power       =  unary  ( '^' power )*          ← right-associative
unary       =  '−' unary  |  primary
primary     =  NUMBER | CONSTANT | VARIABLE
            |  FUNCTION '(' expression ')'
            |  sto '(' expression ',' VARIABLE ')'
            |  '(' expression ')'
```

Implicit multiplication fires when a primary expression is immediately followed by the start of another primary with no explicit operator. The `is_primary_start()` helper in `parser.rs` classifies tokens (Number, VarRegister, ConstPi, ConstE, LeftParen, all function tokens) to detect adjacency. This makes `3(5)`, `(a)b`, `(x)(y)`, `2sin(x)` all parse as multiplication without requiring `*`.

**Evaluator mutability:**
The evaluator takes `&mut VariableStore` rather than `&VariableStore` because `sto()` writes into a register during evaluation. Loop aggregates (`sum`, `int`) still clone the store to scope loop-variable writes.

**AST node types:**

| Node | Purpose |
|------|---------|
| `Literal(i64)` | Numeric constant |
| `Constant(MathConstant)` | `pi`, `e`, or `i` (imaginary unit) |
| `Variable(VariableRef)` | `Ans` or register A–Z |
| `UnaryNegation` | Prefix `−` |
| `BinaryOperation` | `+` `−` `*` `/` `%` `^` |
| `FunctionCall` | Single-argument functions |
| `TwoArgFunction` | Two-argument functions (`nthroot`, `poissonp`, `chicdf`) |
| `ThreeArgFunction` | Three-argument functions (`binomp`) |
| `Store` | `sto(value, register)` — stores value, returns it |
| `LoopAggregate` | `sum()` and `int()` with bound loop variable |

**Rules:**
- Zero `unsafe` code anywhere in this module
- Zero imports from the HAL crate, `runtime/`, `ui/`, or `modes/`
- All memory is stack-allocated — no heap required

### Layer 7 — UI (`numcore/src/ui/`)

OLED display rendering. Generic over `<D: Display>`. Composes the framebuffer from expression text and results. Contains zero `unsafe` code.

**Modules:**

| Module     | Contents                                                    |
|------------|-------------------------------------------------------------|
| `font.rs`  | 5×7 bitmap font (95 printable ASCII glyphs), `render_text::<D>()`, `clear_page::<D>()` |
| `formula.rs`| `render_screen::<D>()`, aggregate Σ/∫ display, pretty-print (π glyph, ×÷− symbols) |

**Display layout:**
- Page 0 (rows 0–7): expression line, max `D::WIDTH / 6` characters
- Page 1 (rows 8–15): result line, max `D::WIDTH / 6` characters
- Aggregate expressions (sum/int) span both pages with tall ∫/Σ glyphs

### Layer 8 — Modes (`modes/`)

[Roadmap] Standard, Scientific, and Graphing calculator modes. Not yet implemented.

## Data flow

```
UART RX (hardware)
    ↓ byte(s)
U::poll_byte()                    ← trait method call (monomorphized to concrete HAL)
    ↓ byte(s)
runtime::event::translate_input_byte_to_event()
    ↓ CalcEvent
runtime::handle_event::<U, D>()
    ↓
├── DigitOrOperator → insert at cursor in CalcState.input_buffer, echo via U::transmit_byte()
├── Submit →
│     lexer::tokenise_expression()
│         ↓ LexResult
│     parser::parse_token_stream()
│         ↓ ParseTree
│     evaluator::evaluate_tree()   ← reads & writes VariableStore (Complex)
│         ↓ Complex
│     runtime records Ans (+ sto register writes persist)
│     engine::format_result()      ← checks MathMode for a+bi vs real-only
│         ↓ byte slice
│     U::transmit_bytes()            ← trait method call
│     D::render()                    ← trait method call
├── Backspace → delete char before cursor from buffer, U::transmit_bytes(b"\x08 \x08")
├── CursorLeft / CursorRight → move cursor in buffer, re-render OLED
└── ToggleMode → flip MathMode in CalcState, re-render OLED with mode banner
```

Every arrow into the hardware layer goes through a trait method call. No `unsafe` escapes the HAL boundary.

The evaluator now works entirely with `Complex` values internally; `MathMode` only affects lexer (`i` token acceptance) and formatter (real-only vs `a+bi` display).

## Memory layout

```
Flash (0x0000_0000, 64 KB):
  [0x0000]  Vector table (initial SP + Reset vector + exceptions)
  [0x0040]  .text (code + rodata)  — 50 407 bytes used (77%)

RAM (0x2000_0000, 8 KB):
  [0x2000_0000]  .data (initialised statics, 0 bytes)
  [0x2000_0000]  .bss  (zero-initialised statics — CalcState, ~2 128 bytes)
  [0x2000_0850]  (gap, not used)
  [0x2000_1400]  .stack (3 KB, grows downward from 0x2000_2000)
  [0x2000_2000]  Top of SRAM (initial SP)
```

The largest single allocation is `CalcState` (~1.5 KB), dominated by the 64-node `ParseTree` arena (~1 KB). Peak stack usage (measured via canary watermark under the full test workload) is 1 056 bytes out of the 3 KB budget, with 2 016 bytes headroom. The stack was increased from 2 KB to 3 KB (`hal-lm3s811/link.x`) after the complex number and cursor-editing features pushed utilization to 99% of the original budget.

## Key design decisions

1. **Q31.32 over Q20.12**: 32 fractional bits give ~9 decimal digits of precision, matching common calculator expectations. The Cortex-M3's 64-bit multiply instructions (`SMULL`/`UMULL`) make `i64` arithmetic free in registers; `i128` intermediates are only needed during multiplies and are synthesised by the compiler.

2. **Static scratch buffers over stack allocation**: With only 8 KB SRAM, allocating a 1 KB parse tree on the stack inside the evaluation call chain would overflow. All scratch memory lives in `CalcState` (static `.bss`).

3. **CORDIC over lookup tables**: CORDIC uses ~200 bytes of constant data (atan table) versus kilobytes for a full sin/cos LUT. With 24 iterations it achieves full Q31.32 precision.

4. **Log-space probability**: Binomial and Poisson probabilities are computed in log space then exponentiated once. This avoids overflow for large `n` (e.g. `n=1000`) that would occur with direct factorial computation.

5. **No heap**: The entire firmware uses precisely zero dynamic allocation. All data structures are fixed-size arrays sized at compile time with safety checks (bounds-checked appends returning `Option`).

6. **Trait-based HAL abstraction**: The shared code in `numcore/` depends on `Uart` and `Display` traits rather than a concrete HAL crate. Each per-MCU binary crate monomorphizes the generic runtime with concrete types. Porting means creating a new HAL crate implementing the trait + a new binary crate — zero changes to `numcore/`.
