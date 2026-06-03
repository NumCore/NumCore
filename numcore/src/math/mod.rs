pub mod complex;
pub mod distributions;
pub mod engine;
pub mod evaluator;
pub mod fixed_point;
pub mod lexer;
pub mod matrix;
pub mod parser;
pub mod vars;

pub use complex::Complex;
pub use matrix::Matrix;

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
