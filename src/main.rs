//! # NumCore
//!
//! Bare-metal Rust calculator firmware for the LM3S811 (ARM Cortex-M3).
//!
//! ## Layer map
//!
//!   main.rs          — crate root; wires modules together, owns panic handler
//!   boot.rs          — Layer 3: vector table, Reset handler, memory init
//!   hal/             — Layer 4: UART, I2C, GPIO, clock (hardware-only layer)
//!   runtime/         — Layer 5: event loop, state machine, mode switching
//!   math/            — Layer 6: lexer, parser, evaluator (hardware-independent)
//!   ui/              — Layer 7: display rendering, cursor, layout  [ROADMAP]
//!   modes/           — Layer 8: standard, scientific, graphing      [ROADMAP]
//!
//! ## Build
//!   cargo build --release
//!
//! ## Run
//!   qemu-system-arm -M lm3s811evb -nographic -serial mon:stdio \
//!     -kernel target/thumbv7m-none-eabi/release/NumCore

#![no_std]
#![no_main]

// ─── Module declarations ──────────────────────────────────────────────────────
//
// Each `mod` statement here corresponds to a directory or file under `src/`.
// The compiler resolves `mod hal` to `src/hal/mod.rs`, and so on.

/// Layer 3 — Boot: vector table, Reset, memory initialisation.
mod boot;

/// Layer 4 — Hardware Abstraction: UART, I2C, GPIO, clock.
mod hal;

/// Layer 5 — Core Runtime: event loop, state, mode routing.
mod runtime;

/// Layer 6 — Math Engine: lexer, parser, evaluator.
mod math;

// Layer 7 (UI) and Layer 8 (Modes) declared here when implemented:
// mod ui;
// mod modes;

// ─── Panic handler ────────────────────────────────────────────────────────────
//
// Required by the Rust compiler in a no_std context. Every possible panic
// (bounds check failure, unwrap on None, explicit panic!()) ends up here.
//
// We attempt a best-effort UART message then spin. On real hardware with a
// debugger attached, halting here and reading the backtrace is straightforward.
// A future improvement: print the PanicInfo location to the UART.

use core::panic::PanicInfo;

#[panic_handler]
fn handle_panic(_panic_info: &PanicInfo) -> ! {
    // Best-effort: UART may not be initialised if the panic occurs very early.
    // If transmit_bytes itself panics we would recurse — acceptable since the
    // spin loop below is the terminal state either way.
    crate::hal::uart::transmit_bytes(b"\r\n*** PANIC - system halted ***\r\n");
    loop {}
}
