# Roadmap

## Recent milestones (v0.2 → v0.4)

- [x] Math engine: Q31.32 fixed-point, lexer, parser, evaluator, distributions
- [x] Boot layer: vector table, `.bss`/`.data` init, Reset handler
- [x] HAL: UART, I2C, GPIO, clock configuration, OLED driver
- [x] Runtime: event loop, state machine, input handling
- [x] UI: 5×7 bitmap font, formula renderer, Σ/∫ glyphs
- [x] `nthroot(x,n)` with proper domain checking (odd/even/non-integer n)
- [x] `natural_log` range reduction fix for sub-ppm accuracy on all inputs
- [x] Trailing-zero stripping in formatted output
- [x] Full documentation: README, ARCHITECTURE, CONTRIBUTING, HACKING, ROADMAP
- [x] Case-sensitive identifiers — `e` is Euler's constant, `E` is register E
- [x] `sto(value, var)` — store a value into a register A–Z, returns the value
- [x] Implicit multiplication — `3(5)`, `(a)b`, `(x)(y)`, `2sin(x)` all work without `*`
- [x] Complex numbers — full arithmetic, transcendental functions, `a+bi` display via mode switching
- [x] Mode switching — Escape key toggles Standard (real-only) / Advanced (complex) modes
- [x] Scrollable results — left/right arrow keys scroll long results on 96×16 OLED
- [x] Cursor-based input editing — insert/delete at arbitrary buffer positions

## Short-term

### Testing infrastructure
- [x] Host-side unit test suite for `math/` — compile and test the pure-math layer natively with `cargo test`
- [x] CI pipeline: run host-side tests on every PR (`cargo test -p numcore_math --tests`)
- [ ] Test-vector generator script to verify Q31.32 results against Python's `Decimal`

### Math engine hardening
- [ ] Fractional display mode — show `1/3` instead of `0.333333` where exact
- [ ] Overflow detection in more paths (currently some operations silently saturate)
- [ ] Configurable output precision (6 digits is hardcoded)

### Developer experience
- [ ] `.vscode/` configuration for debug + task runners
- [ ] `cargo run` wrapper that starts QEMU automatically (via `.cargo/config.toml` runner)

## Medium-term

### Port to modern hardware

The layered architecture makes this the natural next step. The math engine (`math/`) and runtime (`runtime/`) are already hardware-independent.

**Candidate targets:**
- **RP2040 (Raspberry Pi Pico)** — \$4, dual-core Cortex-M0+, ample Flash/RAM, large community
- **STM32F4 series** — Cortex-M4F with FPU (useful even though we use fixed-point), \$5–15
- **ESP32** — dual-core LX6, Wi-Fi/BT for connectivity experiments
- **ATSAMD51** — Cortex-M4F used in Adafruit's PyGamer/PyBadge, has display support

For each port: rewrite `hal/`, update `boot.rs` + `link.x`, adjust `.cargo/config.toml`. Zero application logic changes.

### Reference hardware design
- [ ] Open-source KiCad PCB with:
  - Mechanical key switches (40–60 keys, matrix-scanned via GPIO)
  - High-resolution OLED or LCD (128×64 or better)
  - USB-C for power and UART
  - Dedicated battery management
- [ ] 3D-printable enclosure design
- [ ] Bill of materials targeting \$20–30 BOM cost

### Graphing mode
- [ ] Function plotting: `x^2` rendered as a pixel graph on the display
- [ ] Pan/zoom with register-based viewport control
- [ ] Multiple simultaneous function plots
- [ ] Table of values mode

### Matrix operations
- [ ] Matrix type as a new `Variable` variant (stored in a fixed-size buffer)
- [ ] Matrix addition, multiplication, transposition, determinant, inverse
- [ ] Solve systems of linear equations

### Standalone math crate
- [ ] Extract `math/` into a separate `numcore-math` crate on crates.io
- [ ] Platform-agnostic: compiles on any Rust target (including `std` environments)
- [ ] Makes the Q31.32 engine, parser, and evaluator available to other embedded calculator projects

## Long-term

### Advanced calculator modes
- [ ] **Scientific mode**: unit conversions, physical constants, engineering notation
- [ ] **Programming mode**: integer bases (HEX/DEC/OCT/BIN), bitwise operations, flags
- [ ] **Statistics mode**: data entry, mean/median/stddev, linear regression
- [ ] **CAS basics**: symbolic differentiation, symbolic integration, expression simplification

### Ecosystem
- [ ] **Curriculum alignment**: target exam-board specifications (IB, A-level, AP, SAT)
- [ ] **Flash-based storage**: save expressions, constants, and user programs across reboots
- [ ] **USB connectivity**: virtual serial port for computer connectivity, firmware updates
- [ ] **Open source textbook integration**: companion textbook that reference-codes into NumCore functions

### Community
- [ ] Dedicated documentation site (gh-pages)
- [ ] Community port catalogue — community-maintained `hal/` implementations for various MCUs
- [ ] Web-based simulator (compile math engine to WASM)
- [ ] Internationalisation: i18n for prompt strings, error messages, function names
- [ ] emsdk/SPI flash image builder: flash NumCore to a physical LM3S811 or compatible board

### Distributions and adoption
- [ ] Production-ready reference design with CE/FCC certification
- [ ] Kickstarter or group-buy for first batch of open hardware calculators
- [ ] School pilot programme: donate units to 2–3 schools, gather feedback
- [ ] Exam-board approval process (longest lead-time item — typically 1–3 years)

## Non-goals

The following are explicitly **not** on the roadmap:

- **Touchscreen UI** — hurts battery life and cost; physical keys are more reliable for exams
- **Full CAS like Mathematica** — the scope is calculator-grade CAS (differentiation, integration, simplification), not a computer algebra system
- **Proprietary lock-in** — all hardware designs will be open-source (CERN-OHL-S or similar)
- **Mobile app** — the project is about dedicated calculator hardware; phone apps are a different problem space
- **Wireless exam cheating vectors** — deliberately no Wi-Fi/Bluetooth during exam modes
