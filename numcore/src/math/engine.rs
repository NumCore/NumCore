//! # Math Engine — Public API (Layer 6)
//!
//! The single entry point for all mathematical computation. Orchestrates the
//! full three-stage pipeline using caller-provided scratch buffers so that
//! no large structures are ever stack-allocated:
//!
//!   expression bytes
//!       → lexer::tokenise_expression(expr, &mut lex_scratch)
//!       → parser::parse_token_stream(&lex_result, &mut parse_scratch)
//!       → evaluator::evaluate_tree(&parse_tree, &variables)
//!
//! The runtime calls only `evaluate_expression()` and `format_result()`.
//! All other math modules are internal implementation details.

use super::complex::Complex;
use super::fixed_point;
use super::lexer::LexResult;
use super::parser::ParseTree;
use super::vars::VariableStore;
use super::{evaluator, lexer, parser};
use super::{AngleMode, MathMode};
pub use evaluator::EvalResult;

/// Evaluate a mathematical expression byte slice.
///
/// `lex_scratch` and `parse_scratch` are reusable buffers owned by `CalcState`.
/// They are reset and overwritten on each call — the caller must not rely on
/// their contents after this function returns.
///
/// `mode` controls whether imaginary-unit tokens are accepted (Advanced) or
/// rejected (Standard).
/// `angle_mode` controls whether trig functions interpret values in radians
/// or degrees.
///
/// Returns the result of evaluation.
pub fn evaluate_expression(
    expression: &[u8],
    variables: &mut VariableStore,
    lex_scratch: &mut LexResult,
    parse_scratch: &mut ParseTree,
    mode: MathMode,
    angle_mode: AngleMode,
) -> EvalResult {
    if lexer::tokenise_expression(expression, lex_scratch, mode).is_none() {
        return EvalResult::DomainError;
    }
    if parser::parse_token_stream(lex_scratch, parse_scratch).is_none() {
        return EvalResult::DomainError;
    }
    let result = evaluator::evaluate_tree(parse_scratch, variables, angle_mode);
    if let EvalResult::Value(complex) = result {
        if mode == MathMode::Standard && complex.im != 0 {
            return EvalResult::DomainError;
        }
    }
    result
}

/// Format a numeric result into a human-readable byte slice.
///
/// In Standard mode, only the real part is shown.
/// In Advanced mode, complex values are formatted as `a+bi`.
///
/// Writes into `buffer` (must be ≥ 48 bytes) and returns the filled slice.
pub fn format_result(value: Complex, mode: MathMode, buffer: &mut [u8; 48]) -> &[u8] {
    match mode {
        MathMode::Standard => fixed_point::format_fixed_point(value.re, buffer),
        MathMode::Advanced => format_complex(value, buffer),
    }
}

/// Format an overflow result as scientific notation (e.g. `1.23456E+99`).
///
/// Writes into `buffer` (must be ≥ 48 bytes) and returns the filled slice.
pub fn format_overflow(
    mantissa: i64,
    exponent: i32,
    negative: bool,
    buffer: &mut [u8; 48],
) -> Option<&[u8]> {
    if exponent > 99 || exponent < -99 {
        return None;
    }

    let mut pos = 0usize;

    if negative {
        buffer[pos] = b'-';
        pos += 1;
    }

    let mut scratch = [0u8; 24];
    let mantissa_str = fixed_point::format_fixed_point(mantissa, &mut scratch);
    let mantissa_len = mantissa_str.len();
    buffer[pos..pos + mantissa_len].copy_from_slice(mantissa_str);
    pos += mantissa_len;

    buffer[pos] = b'E';
    pos += 1;

    if exponent < 0 {
        buffer[pos] = b'-';
        pos += 1;
    }
    let exp_abs = exponent.unsigned_abs();
    let mut exp_buf = [0u8; 12];
    let mut exp_pos = 0usize;
    if exp_abs == 0 {
        exp_buf[0] = b'0';
        exp_pos = 1;
    } else {
        let mut n = exp_abs;
        while n > 0 {
            exp_buf[exp_pos] = b'0' + (n % 10) as u8;
            exp_pos += 1;
            n /= 10;
        }
    }
    for i in (0..exp_pos).rev() {
        buffer[pos] = exp_buf[i];
        pos += 1;
    }

    Some(&buffer[..pos])
}

fn format_complex(value: Complex, buffer: &mut [u8; 48]) -> &[u8] {
    if value.im == 0 {
        return fixed_point::format_fixed_point(value.re, buffer);
    }

    let mut pos = 0usize;

    // Format real part
    if value.re != 0 {
        let real_str = fixed_point::format_fixed_point(value.re, buffer);
        let real_len = real_str.len();
        pos = real_len;
    }

    // Format imaginary part
    let mut tmp = [0u8; 24];
    let abs_im = if value.im < 0 { -value.im } else { value.im };
    let im_str = fixed_point::format_fixed_point(abs_im, &mut tmp);

    if pos > 0 {
        if value.im > 0 {
            buffer[pos] = b'+';
        } else {
            buffer[pos] = b'-';
        }
        pos += 1;
    } else if value.im < 0 {
        buffer[pos] = b'-';
        pos += 1;
    }

    // Write magnitude of imaginary part (omit '1' before 'i')
    let im_is_one = abs_im == fixed_point::FIXED_ONE;
    if !im_is_one {
        for &b in im_str {
            buffer[pos] = b;
            pos += 1;
        }
    }
    buffer[pos] = b'i';
    pos += 1;

    &buffer[..pos]
}
