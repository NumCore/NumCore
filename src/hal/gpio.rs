//! # GPIO Driver
//!
//! General-Purpose I/O configuration for the LM3S811.
//!
//! This module exposes named, intention-revealing functions for the two GPIO
//! operations needed so far:
//!   - Enabling alternate function mode (hand a pin to a peripheral like UART)
//!   - Enabling digital mode (required for any digital I/O, including alt-fn)
//!
//! Extend this module when button input or additional GPIO peripherals are added.

use super::mmio;

// ─── GPIO port base addresses ─────────────────────────────────────────────────

/// GPIO Port A base address. Hosts UART0: PA0 = RX, PA1 = TX.
pub const GPIOA_BASE: u32 = 0x4000_4000;

/// GPIO Port B base address. Hosts I2C0: PB2 = SCL, PB3 = SDA.
pub const GPIOB_BASE: u32 = 0x4000_5000;

// ─── GPIO register offsets ────────────────────────────────────────────────────

/// Alternate Function Select register offset.
/// Setting a bit hands the corresponding pin to the peripheral mux.
pub const GPIO_AFSEL_OFFSET: u32 = 0x420;

/// Digital Enable register offset.
/// Must be set for any pin used as a digital input or output (including alt-fn).
pub const GPIO_DEN_OFFSET: u32 = 0x51C;

/// Open-drain select register offset.
/// Required for I2C SDA and SCL pins per the I2C specification.
pub const GPIO_ODR_OFFSET: u32 = 0x50C;

// ─── Public API ───────────────────────────────────────────────────────────────

/// Configure a set of pins on a GPIO port for peripheral (alternate) function.
///
/// # Arguments
/// * `port_base` — base address of the GPIO port (use `GPIOA_BASE` etc.)
/// * `pin_mask`  — bitmask of pins to configure (bit 0 = pin 0, bit 1 = pin 1…)
pub fn configure_pins_as_alternate_function(port_base: u32, pin_mask: u8) {
    // Enable alternate function selection for the specified pins.
    mmio::set_register_bits(port_base, GPIO_AFSEL_OFFSET, pin_mask as u32);
    // Activate digital mode — mandatory for any digital operation.
    mmio::set_register_bits(port_base, GPIO_DEN_OFFSET, pin_mask as u32);
}

/// Configure a set of pins on a GPIO port as open-drain outputs.
///
/// Required for I2C SCL and SDA lines, which use open-drain signalling so
/// that multiple devices can pull the line low without fighting each other.
pub fn configure_pins_as_open_drain(port_base: u32, pin_mask: u8) {
    mmio::set_register_bits(port_base, GPIO_ODR_OFFSET, pin_mask as u32);
}
