//! # Hardware Abstraction Layer (Layer 4)
//!
//! The HAL is the ONLY layer permitted to touch hardware registers directly.
//! Every other layer (runtime, math, UI, modes) must call through this API.
//!
//! ## Module layout
//!
//!   hal/
//!     mod.rs      — this file; re-exports the public HAL surface
//!     mmio.rs     — raw MMIO read/write primitives (unsafe contained here)
//!     uart.rs     — UART0 driver: init, transmit, receive
//!     i2c.rs      — I2C0 driver stub: init, send, recv (for OLED later)
//!     gpio.rs     — GPIO helpers: pin direction, alternate function, digital enable
//!     clock.rs    — System clock configuration
//!
//! ## The HAL contract
//!   - `unsafe` is permitted inside HAL implementation files
//!   - All public HAL functions MUST have safe signatures
//!   - No HAL module may import from runtime, math, ui, or modes
//!   - HAL modules may import from each other (e.g. uart imports mmio)

pub mod clock;
pub mod gpio;
pub mod i2c;
pub mod mmio;
pub mod uart;
