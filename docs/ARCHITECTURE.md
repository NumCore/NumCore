# Architecture

NumCore is organised as a strict **layered architecture**. Each layer has well-defined responsibilities and import rules enforced by convention and documented in the source code.

## Safety contract

NumCore's safety model is explicit and auditable:

1. **HAL crate** (`hal-lm3s811/`) is the **only** crate permitted to perform memory-mapped I/O. All `unsafe` for hardware register access is confined to `mmio.rs` (two functions: `read_register` and `write_register`). Every other HAL module calls through these primitives — no module outside `mmio.rs` issues raw pointer reads or writes.

2. **`firmware/src/boot.rs`** uses `unsafe` for a single, narrow purpose: zeroing `.bss` and copying `.data` from Flash to RAM before the HAL or any Rust code can run. This is unavoidable on bare metal — there is no OS loader to do it.

3. **`runtime/`, `math/`, and `ui/` contain zero `unsafe` code.** They interact with hardware exclusively through the safe public API exposed by the HAL crate. This is verified by inspection and enforced by convention.

4. **Every `unsafe` block** in the codebase has an adjacent `// SAFETY:` comment explaining why the invariants hold.

Porting to a new MCU means auditing and rewriting only the HAL crate and `boot_*.rs`. Everything above the HAL is architecture-agnostic.

## Cargo workspace structure

The project is a Cargo workspace with three members:

| Member | Path | Target | Purpose |
|--------|------|--------|---------|
| `numcore` | `firmware/` | `thumbv7m-none-eabi` | Firmware binary for LM3S811 |
| `hal-lm3s811` | `hal-lm3s811/` | `thumbv7m-none-eabi` | HAL implementation for LM3S811 |
| `numcore_math` | `test-suite/` | Host (e.g. `x86_64`) | Host-side unit tests for the math engine |

The firmware crate (`firmware/`) depends on a single HAL crate:

```toml
[dependencies]
hal-lm3s811 = { path = "../hal-lm3s811" }
```

The workspace root `.cargo/config.toml` does **not** set a default build target. All firmware commands require `--target thumbv7m-none-eabi`. The test-suite compiles for the host by default. Use `make build` / `make test` for convenience.

## Portability

The firmware is currently developed and tested on the **Luminary Micro Stellaris LM3S811** (ARM Cortex-M3, 64 KB Flash, 8 KB SRAM). The layered design is explicitly engineered to support future ports to other architectures and microprocessors:

- **`math/`** — zero HAL imports, zero `unsafe`, zero platform dependencies. Compiles on any target Rust supports. The `test-suite/` workspace member includes `firmware/src/math/` sources via `#[path]` and runs 143 automated tests on the host.
- **`runtime/`** — touches hardware only through the safe HAL API. No register names or memory addresses leak in.
- **`ui/`** — renders to an abstract framebuffer byte array. Only `hal::oled::render_screen()` is platform-specific.
- **HAL crate** (`hal-lm3s811/`) — the only crate that needs rewriting per target. Peripheral register maps, clock trees, and pin muxing are encapsulated here.
- **`boot.rs`** + **`link.x`** — the only files that depend on the MCU's memory map and vector table layout.

A port to a new architecture therefore involves: writing a new HAL crate, creating `boot.rs` and `link.x` for the new MCU, and adding the target triple. No application logic changes.

## Layer map

```
  ┌─────────────────────────────────────────────┐
  │  Layer 8:  modes/              [ROADMAP]    │
  │  Standard, Scientific, Graphing modes       │
  ├─────────────────────────────────────────────┤
  │  Layer 7:  ui/                              │
  │  OLED rendering, font, formula pretty-print │
  ├─────────────────────────────────────────────┤
  │  Layer 6:  math/                            │
  │  Fixed-point, lexer, parser, evaluator,     │
  │  variables, distributions                   │
  ├─────────────────────────────────────────────┤
  │  Layer 5:  runtime/                         │
  │  Event loop, state machine, CalcState,      │
  │  event dispatch                             │
  ├─────────────────────────────────────────────┤
  │  Layer 4:  HAL crate (hal-lm3s811/)          │
  │  UART, I2C, GPIO, clock, OLED driver,       │
  │  MMIO primitives (only crate with unsafe)   │
  ├─────────────────────────────────────────────┤
  │  Layer 3:  boot.rs (firmware/src/)           │
  │  Vector table, Reset handler, .bss/.data    │
  └─────────────────────────────────────────────┘
```

## Layer details

### Layer 3 — Boot (`firmware/src/boot_lm3s811.rs`)

The lowest software layer. Executes before any Rust code can safely run. One `boot_*.rs` file per MCU, selected by feature gate.

**Responsibilities:**
- Place the Cortex-M vector table at Flash address `0x0000_0000`
- Define the `Reset` handler (true entry point after power-on)
- Zero-initialise the `.bss` section (all uninitialised statics)
- Copy the `.data` section from Flash LMA to RAM VMA
- Jump to `runtime::start()` — never returns

**Rules:**
- `unsafe` is permitted here **only** for raw-pointer memory initialisation
- No application logic belongs here
- No HAL calls — the HAL is initialised by the runtime later
- Must compile correctly even if every other module changes

**Exception vectors:**
- Slot 0: Initial stack pointer (top of 8 KB SRAM, minus 2 KB stack)
- Slot 1: Reset vector → `Reset()` function
- Slots 2–15: All route to `DefaultHandler` (spin loop) — upgrade individually as needed (SysTick, SVCall, fault handlers)

### Layer 4 — Hardware Abstraction Layer (separate crate: `hal-lm3s811/`)

The **only** crate permitted to touch hardware registers directly. All `unsafe` for MMIO access is confined to `mmio.rs`.

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

### Layer 5 — Runtime (`firmware/src/runtime/`)

The control centre of the firmware. Sits between the HAL crate and the application layers. Contains zero `unsafe` code.

**Modules:**

| Module    | Contents                                                  |
|-----------|-----------------------------------------------------------|
| `mod.rs`  | `start()`, hardware init sequence, event loop, event handlers, OLED rendering glue |
| `state.rs`| `CalcState` — owns input buffer, variable store, scratch buffers, active mode |
| `event.rs`| `CalcEvent` enum, `translate_input_byte_to_event()` — ASCII byte → typed event |

**Startup sequence:**
1. `boot::Reset()` → `.bss`/`.data` init → `runtime::start()`
2. `initialise_all_hardware()` — UART → I2C → OLED (in that order)
3. Print welcome banner with all available functions
4. `run_event_loop()` — block on UART input, dispatch events

**Event handling:**
- Printable ASCII (`0x20`–`0x7E`) → append to input buffer and echo
- CR/LF (`0x0D`/`0x0A`) → submit expression for evaluation
- BS/DEL (`0x08`/`0x7F`) → backspace
- All other bytes → ignored

**Memory management:**
- `CalcState` is a single `static mut` allocated once in `.bss`
- The lexer scratch buffer (`LexResult`, ~256 bytes) and parser scratch buffer (`ParseTree`, ~1 KB) live inside `CalcState` — never on the stack
- This avoids stack overflow on the 8 KB SRAM (2 KB reserved for stack)

**Rules:**
- Contains zero `unsafe` — every hardware interaction goes through the HAL crate's safe API
- Owns and updates `CalcState` (including the variable store)
- Routes input events to handlers, triggers UI re-renders

### Layer 6 — Math Engine (`firmware/src/math/`)

Completely hardware-independent. Can be compiled and tested on any platform. Zero `unsafe` code, zero HAL imports, zero heap allocation.

The math engine is tested via the `test-suite/` workspace member, which includes every `firmware/src/math/` source file via `#[path]` attributes and compiles them for the host. 143 automated tests cover fixed-point arithmetic, lexer, parser, evaluator, variables, distributions, and the full expression pipeline. Run with `cargo test -p numcore_math --tests` or `make test`.

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
| `Constant(MathConstant)` | `pi` or `e` |
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

### Layer 7 — UI (`firmware/src/ui/`)

OLED display rendering. Composes the 96×16 framebuffer from expression text and results. Contains zero `unsafe` code.

**Modules:**

| Module     | Contents                                                    |
|------------|-------------------------------------------------------------|
| `font.rs`  | 5×7 bitmap font (95 printable ASCII glyphs), `render_text()`, `clear_page()` |
| `formula.rs`| `render_screen()`, aggregate Σ/∫ display, pretty-print (π glyph, ×÷− symbols) |

**Display layout:**
- Page 0 (rows 0–7): expression line, max 16 characters
- Page 1 (rows 8–15): result line, max 16 characters
- Aggregate expressions (sum/int) span both pages with tall ∫/Σ glyphs

### Layer 8 — Modes (`modes/`)

[Roadmap] Standard, Scientific, and Graphing calculator modes. Not yet implemented.

## Data flow

```
UART RX (hardware)
    ↓ byte
hal::uart::poll_byte()          ← safe HAL call
    ↓ byte
runtime::event::translate_input_byte_to_event()
    ↓ CalcEvent
runtime::handle_event()
    ↓
├── DigitOrOperator → append to CalcState.input_buffer
├── Submit →
│     lexer::tokenise_expression()
│         ↓ LexResult
│     parser::parse_token_stream()
│         ↓ ParseTree
│     evaluator::evaluate_tree()   ← reads & writes VariableStore
│         ↓ Q31.32
│     runtime records Ans (+ sto register writes persist)
│     engine::format_result()
│         ↓ byte slice
│     hal::uart::transmit_bytes()   ← safe HAL call
│     hal::oled::render_screen()    ← safe HAL call
└── Backspace → remove last char from buffer
```

Every arrow into the hardware layer goes through a safe HAL function. No `unsafe` escapes the HAL boundary.

## Memory layout

```
Flash (0x0000_0000, 64 KB):
  [0x0000]  Vector table (initial SP + Reset vector + exceptions)
  [0x0040]  .text (code + rodata)

RAM (0x2000_0000, 8 KB):
  [0x2000_0000]  .data (initialised statics, copied from Flash)
  [0x2000_0100]  .bss  (zero-initialised statics — CalcState lives here)
  [0x2000_1700]  .stack (2 KB, grows downward from 0x2000_2000)
  [0x2000_2000]  Top of SRAM (initial SP)
```

The largest single allocation is `CalcState` (~1.5 KB), dominated by the 64-node `ParseTree` arena (~1 KB). The 2 KB stack budget is safe — the deepest call chain (integration via Simpson's rule) uses under 200 bytes.

## Key design decisions

1. **Q31.32 over Q20.12**: 32 fractional bits give ~9 decimal digits of precision, matching common calculator expectations. The Cortex-M3's 64-bit multiply instructions (`SMULL`/`UMULL`) make `i64` arithmetic free in registers; `i128` intermediates are only needed during multiplies and are synthesised by the compiler.

2. **Static scratch buffers over stack allocation**: With only 8 KB SRAM, allocating a 1 KB parse tree on the stack inside the evaluation call chain would overflow. All scratch memory lives in `CalcState` (static `.bss`).

3. **CORDIC over lookup tables**: CORDIC uses ~200 bytes of constant data (atan table) versus kilobytes for a full sin/cos LUT. With 24 iterations it achieves full Q31.32 precision.

4. **Log-space probability**: Binomial and Poisson probabilities are computed in log space then exponentiated once. This avoids overflow for large `n` (e.g. `n=1000`) that would occur with direct factorial computation.

5. **No heap**: The entire firmware uses precisely zero dynamic allocation. All data structures are fixed-size arrays sized at compile time with safety checks (bounds-checked appends returning `Option`).

6. **Separate HAL crate**: The HAL is an independent Cargo crate in the workspace, not a module in the firmware crate. This enforces a compile-time boundary — the shared code never imports from a specific HAL; it imports from `hal` (the alias). Porting means creating a new crate implementing the same public API surface.
