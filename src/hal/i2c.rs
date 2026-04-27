//! # I2C0 Driver (Layer 4 — HAL)
//!
//! Driver stub for the LM3S811's I2C0 peripheral. This module is the future
//! home of all I2C communication — currently used as the transport layer for
//! the OSRAM Pictiva 96×16 OLED display via its SSD0303 controller.
//!
//! ## Hardware wiring on LM3S811EVB
//!   PB2 = I2C0 SCL (clock)
//!   PB3 = I2C0 SDA (data)
//!   OLED I2C address: 0x3C (SA0 pin tied low) or 0x3D (SA0 tied high)
//!
//! ## I2C clock speed
//!   Standard mode: 100 kHz
//!   TPR (Timer Period Register) = (system_clock / (20 × i2c_speed)) − 1
//!                               = (12_000_000 / (20 × 100_000)) − 1
//!                               = 5
//!
//! ## Status
//!   STUB — functions are defined with correct signatures but do nothing yet.
//!   Implement in order:
//!     1. initialise_i2c()
//!     2. send_byte()
//!     3. send_bytes()
//!     4. check_bus_busy()
//!   Then build the SSD0303 driver on top in ui/oled.rs (a HAL consumer —
//!   it calls these functions, never touches I2C registers directly).

use super::{clock, gpio, mmio};

// ─── I2C0 register map ────────────────────────────────────────────────────────

/// I2C0 Master base address.
const I2C0_MASTER_BASE: u32 = 0x4002_0000;

/// Master Slave Address register — holds target device address + R/W bit.
const I2C_MSA_OFFSET: u32 = 0x000;

/// Master Control/Status register — initiate transfers, read status.
const I2C_MCS_OFFSET: u32 = 0x004;

/// Master Data register — byte to transmit or received byte.
const I2C_MDR_OFFSET: u32 = 0x008;

/// Master Timer Period register — sets SCL frequency.
const I2C_MTPR_OFFSET: u32 = 0x00C;

// ─── MCS bit masks ────────────────────────────────────────────────────────────

/// MCS write: initiate a burst transmit START condition.
const I2C_MCS_START: u32 = 1 << 1;

/// MCS write: initiate a STOP condition after this byte.
const I2C_MCS_STOP: u32 = 1 << 2;

/// MCS write: run (trigger the transfer).
const I2C_MCS_RUN: u32 = 1 << 0;

/// MCS read: bus busy flag — another master (or a prior transaction) is active.
const I2C_MCS_BUS_BUSY: u32 = 1 << 6;

/// MCS read: master busy flag — a transfer is in progress.
const I2C_MCS_MASTER_BUSY: u32 = 1 << 0;

// ─── GPIO pins for I2C0 ───────────────────────────────────────────────────────

/// PB2 (SCL) and PB3 (SDA) pin mask.
const I2C0_GPIO_PIN_MASK: u8 = 0b0000_1100;

// ─── Timing ───────────────────────────────────────────────────────────────────

/// Timer Period Register value for 100 kHz I2C at 12 MHz system clock.
/// Formula: TPR = (system_clock / (20 × i2c_speed)) − 1 = 5
const I2C_TIMER_PERIOD_100KHZ: u32 = 5;

// ─── Public API ───────────────────────────────────────────────────────────────

/// Initialise the I2C0 peripheral for 100 kHz standard-mode operation.
///
/// Configures PB2/PB3 as open-drain alternate-function pins, sets the SCL
/// clock speed, and enables the I2C master block.
///
/// # Status: STUB — not yet implemented.
pub fn initialise_i2c() {
    // Enable I2C0 and GPIOB clocks.
    clock::enable_rcgc1_peripherals(clock::CLOCK_GATE_I2C0);
    clock::enable_rcgc2_peripherals(clock::CLOCK_GATE_GPIOB);
    clock::spin_for_cycles(10);

    // Configure PB2/PB3 as alternate-function open-drain pins.
    gpio::configure_pins_as_alternate_function(gpio::GPIOB_BASE, I2C0_GPIO_PIN_MASK);
    gpio::configure_pins_as_open_drain(gpio::GPIOB_BASE, I2C0_GPIO_PIN_MASK);

    // Set SCL frequency.
    mmio::write_register(I2C0_MASTER_BASE, I2C_MTPR_OFFSET, I2C_TIMER_PERIOD_100KHZ);

    // TODO: set MCS MASTER_ENABLE bit to bring the master block online.
    // Reference: LM3S811 datasheet section 15.4 "Initialization and Configuration"
}

/// Transmit a single byte to the device at `device_address`.
///
/// Generates: START → address+W → data byte → STOP
///
/// # Arguments
/// * `device_address` — 7-bit I2C address of the target device
/// * `byte`           — the byte to send
///
/// # Status: STUB — not yet implemented.
pub fn send_byte(device_address: u8, byte: u8) {
    // Step 1: Set target address with write bit (bit 0 = 0).
    mmio::write_register(I2C0_MASTER_BASE, I2C_MSA_OFFSET, (device_address as u32) << 1);

    // Step 2: Load the data byte.
    mmio::write_register(I2C0_MASTER_BASE, I2C_MDR_OFFSET, byte as u32);

    // Step 3: Issue START + RUN + STOP (single-byte transfer).
    mmio::write_register(
        I2C0_MASTER_BASE,
        I2C_MCS_OFFSET,
        I2C_MCS_START | I2C_MCS_RUN | I2C_MCS_STOP,
    );

    // Step 4: Wait for the transfer to complete.
    wait_until_master_idle();
}

/// Transmit a slice of bytes to `device_address` as a single I2C transaction.
///
/// Generates: START → address+W → byte[0] → byte[1] → … → STOP
///
/// # Status: STUB — not yet implemented.
pub fn send_bytes(device_address: u8, bytes: &[u8]) {
    if bytes.is_empty() { return; }

    // Set target address (write mode).
    mmio::write_register(I2C0_MASTER_BASE, I2C_MSA_OFFSET, (device_address as u32) << 1);

    let last_index = bytes.len() - 1;
    for (index, &byte) in bytes.iter().enumerate() {
        mmio::write_register(I2C0_MASTER_BASE, I2C_MDR_OFFSET, byte as u32);

        let control_bits = if index == 0 && index == last_index {
            // Only byte — START + RUN + STOP
            I2C_MCS_START | I2C_MCS_RUN | I2C_MCS_STOP
        } else if index == 0 {
            // First of many — START + RUN (no STOP yet)
            I2C_MCS_START | I2C_MCS_RUN
        } else if index == last_index {
            // Last byte — RUN + STOP
            I2C_MCS_RUN | I2C_MCS_STOP
        } else {
            // Middle byte — RUN only
            I2C_MCS_RUN
        };

        mmio::write_register(I2C0_MASTER_BASE, I2C_MCS_OFFSET, control_bits);
        wait_until_master_idle();
    }
}

/// Block until the I2C master is no longer busy with a transfer.
fn wait_until_master_idle() {
    while mmio::read_register(I2C0_MASTER_BASE, I2C_MCS_OFFSET) & I2C_MCS_MASTER_BUSY != 0 {}
}
