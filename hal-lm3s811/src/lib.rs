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

pub struct Lm3s811Uart;

impl numcore::hal::Uart for Lm3s811Uart {
    fn init() {
        uart::initialise_uart();
    }

    fn transmit_bytes(bytes: &[u8]) {
        uart::transmit_bytes(bytes);
    }

    fn transmit_byte(byte: u8) {
        uart::transmit_byte(byte);
    }

    fn poll_byte() -> Option<u8> {
        uart::poll_byte()
    }
}

pub struct Ssd0303;

impl numcore::hal::Display for Ssd0303 {
    type Buffer = [u8; 192];
    const WIDTH: usize = 96;
    const HEIGHT: usize = 16;

    fn new_buffer() -> Self::Buffer {
        [0u8; 192]
    }

    fn init() {
        i2c::initialise_i2c();
        oled::initialise_oled();
    }

    fn render(fb: &Self::Buffer) {
        oled::render_screen(fb);
    }

    fn set_pixel(fb: &mut Self::Buffer, col: usize, row: usize, on: bool) {
        oled::set_pixel(fb, col, row, on);
    }
}
