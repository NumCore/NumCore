//! # HAL — LM3S811 (Stellaris)
//!
//! Hardware Abstraction Layer for the LM3S811 ARM Cortex-M3 MCU.
//! Implements UART, I2C, GPIO, clock control, MMIO primitives, and
//! the SSD0303 OLED display driver for the LM3S811EVB.
//!
//! Every function in this crate is safe to call. `unsafe` is contained
//! within MMIO register read/write operations in `mmio.rs` and I2C
//! wait-loop spin code.

#![no_std]

pub mod clock;
pub mod gpio;
pub mod i2c;
pub mod mmio;
pub mod oled;
pub mod uart;
