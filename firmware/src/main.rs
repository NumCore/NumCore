//! # NumCore
//!
//! Bare-metal Rust calculator firmware for the LM3S811 (ARM Cortex-M3).
//!
//! ## Layer map
//!
//!   main.rs          — crate root; wires modules together, owns panic handler
//!   boot_lm3s811.rs  — Layer 3: vector table, Reset handler, memory init
//!   hal/             — Layer 4: UART, I2C, GPIO, clock (hardware-only layer)
//!   runtime/         — Layer 5: event loop, state machine, mode switching
//!   math/            — Layer 6: lexer, parser, evaluator (hardware-independent)
//!   ui/              — Layer 7: display rendering, cursor, layout  [ROADMAP]
//!   modes/           — Layer 8: standard, scientific, graphing      [ROADMAP]
//!
//! ## Build
//!   cargo build -p numcore --release --target thumbv7m-none-eabi
//!
//! ## Run
//!   qemu-system-arm -M lm3s811evb -nographic -serial mon:stdio \
//!     -kernel target/thumbv7m-none-eabi/release/NumCore

#![no_std]
#![no_main]

// ─── HAL crate ───────────────────────────────────────────────────────────────
//
// The hal-lm3s811 crate implements all hardware-specific drivers (UART, I2C,
// GPIO, clock, OLED). It is renamed to `hal` so that shared layers (runtime,
// UI) import it by that name.  Porting to a new MCU means replacing this
// crate — the shared code never changes.

extern crate hal_lm3s811 as hal;

// ─── Boot loader ─────────────────────────────────────────────────────────────

#[path = "boot_lm3s811.rs"]
mod boot;

// ─── Shared layers (MCU-independent) ─────────────────────────────────────────

/// Layer 5 — Core Runtime: event loop, state machine, mode routing.
mod runtime;

/// Layer 6 — Math Engine: lexer, parser, evaluator.
mod math;

/// Layer 7 — UI rendering helpers.
mod ui {
    pub(crate) use hal::oled;

    pub mod font;
    pub mod formula;
}

// Layer 8 (Modes) declared here when implemented:
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
    hal::uart::transmit_bytes(b"\r\n*** PANIC - system halted ***\r\n");
    loop {}
}
