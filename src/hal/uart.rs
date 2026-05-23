//! # UART0 Driver
//!
//! Drives the LM3S811's first UART peripheral (UART0) at 115 200-8-N-1.
//! In QEMU, UART0 is wired to the host stdio, making it the natural console.
//!
//! ## Baud-rate calculation
//!
//! The LM3S811 UART uses a fractional baud-rate divisor:
//!
//!   BRD = system_clock / (16 × baud_rate)
//!       = 12_000_000  / (16 × 115_200)
//!       ≈ 6.510
//!
//!   Integer part (IBRD)   = 6
//!   Fractional part (FBRD) = round(0.510 × 64) = 33
//!
//! ## Pin assignment
//!   PA0 = UART0 RX (receive)
//!   PA1 = UART0 TX (transmit)

use super::{clock, gpio, mmio};

// ─── UART0 register map ───────────────────────────────────────────────────────

/// UART0 peripheral base address.
const UART0_BASE: u32 = 0x4000_C000;

/// Data Register — write to transmit, read to receive.
const UART_DR_OFFSET: u32 = 0x000;

/// Flag Register — contains TX/RX FIFO status bits.
const UART_FR_OFFSET: u32 = 0x018;

/// Integer Baud-Rate Divisor register.
const UART_IBRD_OFFSET: u32 = 0x024;

/// Fractional Baud-Rate Divisor register.
const UART_FBRD_OFFSET: u32 = 0x028;

/// Line Control register — word length, parity, stop bits, FIFO enable.
const UART_LCRH_OFFSET: u32 = 0x02C;

/// Control register — UART enable, TX enable, RX enable.
const UART_CTL_OFFSET: u32 = 0x030;

// ─── Flag register bit masks ──────────────────────────────────────────────────

/// TX FIFO Full flag. When set, writing to DR would be lost — must wait.
const UART_FLAG_TX_FIFO_FULL: u32 = 1 << 5;

/// RX FIFO Empty flag. When set, no data is available to read.
const UART_FLAG_RX_FIFO_EMPTY: u32 = 1 << 4;

// ─── Control and line-control bit masks ───────────────────────────────────────

/// UART Enable bit in CTL.
const UART_CTL_UART_ENABLE: u32 = 1 << 0;

/// Transmit Enable bit in CTL.
const UART_CTL_TX_ENABLE: u32 = 1 << 8;

/// Receive Enable bit in CTL.
const UART_CTL_RX_ENABLE: u32 = 1 << 9;

/// 8-bit word length field in LCRH (WLEN = 0b11).
const UART_LCRH_WORD_LENGTH_8BIT: u32 = 0b11 << 5;

/// FIFO Enable bit in LCRH.
const UART_LCRH_FIFO_ENABLE: u32 = 1 << 4;

// ─── Baud-rate divisors for 115200 @ 12 MHz ──────────────────────────────────

/// Integer baud-rate divisor. Derived from SYSTEM_CLOCK_HZ above.
const BAUD_RATE_INTEGER_DIVISOR: u32 = 6;

/// Fractional baud-rate divisor.
const BAUD_RATE_FRACTIONAL_DIVISOR: u32 = 33;

// ─── GPIO pin masks for UART0 on Port A ──────────────────────────────────────

/// PA0 (RX) and PA1 (TX) — both configured as alternate-function pins.
const UART0_GPIO_PIN_MASK: u8 = 0b0000_0011;

// ─── Public API ───────────────────────────────────────────────────────────────

/// Initialise UART0 for 115 200-8-N-1 communication.
///
/// Must be called once before any other UART function. Safe to call from the
/// runtime init sequence.
///
/// Steps performed:
///   1. Enable UART0 and GPIOA clocks via SysCtl
///   2. Configure PA0/PA1 for alternate (UART) function
///   3. Set baud rate, word format, and FIFO mode
///   4. Enable the UART
pub fn initialise_uart() {
    // Step 1: Enable peripheral clocks.
    clock::enable_rcgc1_peripherals(clock::CLOCK_GATE_UART0);
    clock::enable_rcgc2_peripherals(clock::CLOCK_GATE_GPIOA);
    // Allow clocks to stabilise before accessing peripheral registers.
    clock::spin_for_cycles(10);

    // Step 2: Configure GPIOA pins PA0/PA1 for UART alternate function.
    gpio::configure_pins_as_alternate_function(gpio::GPIOA_BASE, UART0_GPIO_PIN_MASK);

    // Step 3: Disable UART before changing configuration (required by datasheet).
    mmio::write_register(UART0_BASE, UART_CTL_OFFSET, 0);

    // Step 4: Configure baud rate.
    mmio::write_register(UART0_BASE, UART_IBRD_OFFSET, BAUD_RATE_INTEGER_DIVISOR);
    mmio::write_register(UART0_BASE, UART_FBRD_OFFSET, BAUD_RATE_FRACTIONAL_DIVISOR);

    // Step 5: 8-bit words, no parity, 1 stop bit, FIFOs enabled.
    mmio::write_register(
        UART0_BASE,
        UART_LCRH_OFFSET,
        UART_LCRH_WORD_LENGTH_8BIT | UART_LCRH_FIFO_ENABLE,
    );

    // Step 6: Enable UART with both TX and RX active.
    mmio::write_register(
        UART0_BASE,
        UART_CTL_OFFSET,
        UART_CTL_UART_ENABLE | UART_CTL_TX_ENABLE | UART_CTL_RX_ENABLE,
    );
}

/// Transmit a single byte over UART0.
///
/// Blocks until the TX FIFO has space. On a real system under heavy load this
/// could stall for many microseconds — add interrupt-driven TX if that matters.
pub fn transmit_byte(byte: u8) {
    // Spin until the transmit FIFO has room for another byte.
    while mmio::read_register(UART0_BASE, UART_FR_OFFSET) & UART_FLAG_TX_FIFO_FULL != 0 {}
    mmio::write_register(UART0_BASE, UART_DR_OFFSET, byte as u32);
}

/// Transmit a byte slice over UART0.
///
/// Convenience wrapper around `transmit_byte` for sending strings.
pub fn transmit_bytes(bytes: &[u8]) {
    for &byte in bytes {
        transmit_byte(byte);
    }
}

/// Receive a single byte from UART0, blocking until one is available.
///
/// Spins on the RX FIFO Empty flag. On a real system, this should become
/// interrupt-driven to avoid wasting CPU cycles while waiting for input.
pub fn receive_byte_blocking() -> u8 {
    // Spin until at least one byte is in the receive FIFO.
    while mmio::read_register(UART0_BASE, UART_FR_OFFSET) & UART_FLAG_RX_FIFO_EMPTY != 0 {}
    // Read the byte from the data register. Bits 8-11 are error flags — mask them.
    (mmio::read_register(UART0_BASE, UART_DR_OFFSET) & 0xFF) as u8
}

/// Receive a single byte from UART0 if one is already available.
///
/// Returns immediately with `None` when the RX FIFO is empty.
pub fn poll_byte() -> Option<u8> {
    if mmio::read_register(UART0_BASE, UART_FR_OFFSET) & UART_FLAG_RX_FIFO_EMPTY != 0 {
        return None;
    }

    Some((mmio::read_register(UART0_BASE, UART_DR_OFFSET) & 0xFF) as u8)
}
