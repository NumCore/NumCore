//! # Math Engine (Layer 6)
//!
//! Completely hardware-independent. Can be compiled and tested on any platform.
//!
//! ## Module layout
//!   fixed_point.rs  — Q20.12 arithmetic: multiply, divide, sqrt, trig, log, exp
//!   vars.rs         — Variable store: Ans, A–F registers
//!   lexer.rs        — Expression string → typed Token stream
//!   parser.rs       — Token stream → Abstract Syntax Tree (with precedence)
//!   evaluator.rs    — AST → Q20.12 numeric result (reads VariableStore)
//!   engine.rs       — Public API: evaluate_expression(), format_result()
//!
//! ## The math contract
//!   - Zero `unsafe` code anywhere in this module
//!   - Zero imports from hal, runtime, ui, or modes
//!   - All functions are pure (no side effects, no global mutable state)
//!   - All memory is stack-allocated — no heap required

pub mod engine;
pub mod evaluator;
pub mod fixed_point;
pub mod lexer;
pub mod parser;
pub mod vars;
pub mod distributions;
pub mod state;
