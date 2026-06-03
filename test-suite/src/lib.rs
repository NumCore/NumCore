//! # NumCore Math — Host-side Test Harness
//!
//! This library enables host-side unit testing of the `math/` module
//! from the NumCore firmware.  It includes each source file from
//! `src/math/` via `#[path]` attributes, reconstructing the same
//! module tree so internal `super::` and `crate::` references resolve.
//!
//! ## `#![no_std]`
//!
//! The firmware targets `thumbv7m-none-eabi` (ARM Cortex-M3, no std).
//! This library keeps `#![no_std]` so the math source files compile
//! under identical conditions.  When compiled as a test binary on the
//! host, `std` is linked automatically by Cargo and provides the test
//! harness and panic handler.
//!
//! ## Building & testing
//!
//! ```bash
//! # Run host-side unit tests (overrides the embedded target from .cargo/config.toml)
//! cargo test -p numcore_math --target "$(rustc -vV | grep host | awk '{print $2}')"
//!
//! # Build firmware (embedded target) — unchanged
//! cargo build --release --target thumbv7m-none-eabi
//! ```
//!
//! ## Module layout
//!
//! Each math file is loaded at the crate root via `#[path]`, so
//! `super::fixed_point` inside `lexer.rs` resolves to the sibling
//! `crate::fixed_point`.  A re-export module `math::` matches the
//! firmware's namespace for integration tests.

#![no_std]

// ─── MathMode / AngleMode ─────────────────────────────────────────────────────
// Defined at crate root so that `super::MathMode` and `super::AngleMode`
// resolve correctly in the #[path]-included evaluator.rs and engine.rs.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MathMode {
    Standard,
    Advanced,
    Matrix,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AngleMode {
    Radians,
    Degrees,
}

// ─── HAL stub ────────────────────────────────────────────────────────────────
// Satisfies `use crate::hal::uart;` in fixed_point.rs (line 39), a dead
// import only used in commented-out debug prints.

#[allow(dead_code)]
pub mod hal {
    pub mod uart {
        pub fn transmit_bytes(_bytes: &[u8]) {}
    }
}

// ─── Math modules (included from src/math/ via #[path]) ─────────────────────
//
// Each file sits at crate root level so that `super::fixed_point` etc.
// inside the math files resolves to the sibling module created here.

#[path = "../../numcore/src/math/fixed_point.rs"]
pub mod fixed_point;

#[path = "../../numcore/src/math/lexer.rs"]
pub mod lexer;

#[path = "../../numcore/src/math/parser.rs"]
pub mod parser;

#[path = "../../numcore/src/math/evaluator.rs"]
pub mod evaluator;

#[path = "../../numcore/src/math/vars.rs"]
pub mod vars;

#[path = "../../numcore/src/math/engine.rs"]
pub mod engine;

#[path = "../../numcore/src/math/distributions.rs"]
pub mod distributions;

#[path = "../../numcore/src/math/complex.rs"]
pub mod complex;

#[path = "../../numcore/src/math/matrix.rs"]
pub mod matrix;

// ─── Re-export under math:: for API compatibility ───────────────────────────

pub mod math {
    pub use crate::complex;
    pub use crate::distributions;
    pub use crate::engine;
    pub use crate::evaluator;
    pub use crate::fixed_point;
    pub use crate::lexer;
    pub use crate::matrix;
    pub use crate::matrix::{Matrix, MatrixKind};
    pub use crate::parser;
    pub use crate::vars;
    pub use crate::AngleMode;
    pub use crate::MathMode;
}
