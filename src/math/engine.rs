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

use super::{evaluator, lexer, parser};
use super::fixed_point;
use super::lexer::LexResult;
use super::parser::ParseTree;
use super::vars::VariableStore;

// ─── Public API ───────────────────────────────────────────────────────────────

/// Evaluate a mathematical expression byte slice.
///
/// `lex_scratch` and `parse_scratch` are reusable buffers owned by `CalcState`.
/// They are reset and overwritten on each call — the caller must not rely on
/// their contents after this function returns.
///
/// Returns `Some(q20_12_result)` on success, `None` on any error.
pub fn evaluate_expression(
    expression:    &[u8],
    variables:     &VariableStore,
    lex_scratch:   &mut LexResult,
    parse_scratch: &mut ParseTree,
) -> Option<i64> {
    // tokenise_expression and parse_token_stream write into the scratch
    // buffers and return a reference into them. We call evaluate_tree
    // directly on the scratch buffers after writing is complete.
    lexer::tokenise_expression(expression, lex_scratch)?;
    parser::parse_token_stream(lex_scratch, parse_scratch)?;
    evaluator::evaluate_tree(parse_scratch, variables)
}

/// Format a Q20.12 fixed-point result into a human-readable byte slice.
///
/// Writes into `buffer` (must be ≥ 20 bytes) and returns the filled slice.
/// Trailing fractional zeros are stripped. Integer results have no decimal point.
pub fn format_result(value: i64, buffer: &mut [u8; 24]) -> &[u8] {
    fixed_point::format_fixed_point(value, buffer)
}