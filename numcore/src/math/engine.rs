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
use super::MathMode;
use super::{evaluator, lexer, parser};

/// Evaluate a mathematical expression byte slice.
///
/// `lex_scratch` and `parse_scratch` are reusable buffers owned by `CalcState`.
/// They are reset and overwritten on each call — the caller must not rely on
/// their contents after this function returns.
///
/// `mode` controls whether imaginary-unit tokens are accepted (Advanced) or
/// rejected (Standard).
///
/// Returns `Some(result)` on success, `None` on any error.
pub fn evaluate_expression(
    expression: &[u8],
    variables: &mut VariableStore,
    lex_scratch: &mut LexResult,
    parse_scratch: &mut ParseTree,
    mode: MathMode,
) -> Option<Complex> {
    lexer::tokenise_expression(expression, lex_scratch, mode)?;
    parser::parse_token_stream(lex_scratch, parse_scratch)?;
    let result = evaluator::evaluate_tree(parse_scratch, variables)?;
    if mode == MathMode::Standard && result.im != 0 {
        return None;
    }
    Some(result)
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
