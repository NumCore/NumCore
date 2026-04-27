//! # Lexer (Math Engine — Layer 6)
//!
//! Converts an expression byte slice into a flat array of typed `Token` values.
//!
//! ## Full token set
//!   Numbers    — integer and decimal literals  (e.g. 42, 3.14)
//!   Operators  — +  -  *  /  ^  %
//!   Grouping   — (  )
//!   Functions  — sin cos tan asin acos atan sqrt abs log ln log2 exp
//!                floor ceil round deg rad
//!   Constants  — pi  e
//!   Variables  — Ans  A B C D E F
//!
//! ## Lexing rules
//!   - Whitespace is discarded between tokens
//!   - Identifiers are case-insensitive (SIN = sin = Sin)
//!   - A minus sign following an operator, opening paren, or at the
//!     start of the expression is classified as UnaryMinus, not Minus,
//!     so the parser can build a unary-negation node correctly
//!   - Decimal numbers are parsed and immediately converted to Q20.12

use super::fixed_point;

/// Maximum tokens from one expression. A 64-char expression can yield at most
/// ~32 operator/operand pairs; 128 gives generous headroom.
pub const MAX_TOKEN_COUNT: usize = 32;
// Sized for the LM3S811's 8 KB SRAM. A 64-char expression yields at most
// ~20 tokens in practice; 32 gives headroom without blowing the stack.

// ─── Token ────────────────────────────────────────────────────────────────────

/// A single lexical unit in a calculator expression.
///
/// All numeric values are stored as Q20.12 fixed-point (i32).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Token {
    // ── Literals ──
    /// A numeric literal already converted to Q20.12.
    Number(i32),

    // ── Binary operators (in precedence order, low → high) ──
    Plus,           // +
    Minus,          // −  (binary subtraction)
    Star,           // ×
    Slash,          // ÷
    Percent,        // %  (modulo)
    Caret,          // ^  (exponentiation, right-associative)

    // ── Unary operator ──
    UnaryMinus,     // −  (negation, prefix)

    // ── Grouping ──
    LeftParen,      // (
    RightParen,     // )

    // ── Named functions (all take one argument in parens) ──
    FuncSin,
    FuncCos,
    FuncTan,
    FuncAsin,
    FuncAcos,
    FuncAtan,
    FuncSqrt,
    FuncAbs,
    FuncLog,        // log10
    FuncLn,         // natural log
    FuncLog2,
    FuncExp,        // e^x
    FuncFloor,
    FuncCeil,
    FuncRound,
    FuncDeg,        // radians → degrees
    FuncRad,        // degrees → radians

    // ── Named constants ──
    ConstPi,        // π
    ConstE,         // e (Euler's number)

    // ── Variables ──
    VarAns,         // Ans
    VarRegister(u8),// A–F (stores the letter byte)
}

// ─── LexResult ────────────────────────────────────────────────────────────────

/// Output of a successful lex pass: a fixed-size token array plus a count.
pub struct LexResult {
    pub tokens: [Token; MAX_TOKEN_COUNT],
    pub token_count: usize,
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Tokenise an expression byte slice.
///
/// Returns `None` if any unrecognised character or malformed number is found.
/// Whitespace is discarded. Identifiers are matched case-insensitively.
pub fn tokenise_expression(expression: &[u8]) -> Option<LexResult> {
    let mut result = LexResult {
        tokens: [Token::Number(0); MAX_TOKEN_COUNT],
        token_count: 0,
    };

    let mut cursor = 0usize;

    while cursor < expression.len() {
        // Skip whitespace.
        if expression[cursor] == b' ' { cursor += 1; continue; }

        // ── Number literal ────────────────────────────────────────────────────
        if expression[cursor].is_ascii_digit() || (
            expression[cursor] == b'.' &&
                cursor + 1 < expression.len() &&
                expression[cursor + 1].is_ascii_digit()
        ) {
            let (fp_value, consumed) = parse_number_literal(&expression[cursor..])?;
            append_token(&mut result, Token::Number(fp_value))?;
            cursor += consumed;
            continue;
        }

        // ── Single-character tokens ───────────────────────────────────────────
        let single = match expression[cursor] {
            b'(' => Some(Token::LeftParen),
            b')' => Some(Token::RightParen),
            b'+' => Some(Token::Plus),
            b'*' => Some(Token::Star),
            b'/' => Some(Token::Slash),
            b'%' => Some(Token::Percent),
            b'^' => Some(Token::Caret),
            _    => None,
        };
        if let Some(token) = single {
            append_token(&mut result, token)?;
            cursor += 1;
            continue;
        }

        // ── Minus: binary subtraction or unary negation ───────────────────────
        if expression[cursor] == b'-' {
            let token = if is_unary_position(&result) {
                Token::UnaryMinus
            } else {
                Token::Minus
            };
            append_token(&mut result, token)?;
            cursor += 1;
            continue;
        }

        // ── Identifier: function name, constant, or variable ─────────────────
        if expression[cursor].is_ascii_alphabetic() {
            let (token, consumed) = parse_identifier(&expression[cursor..])?;
            append_token(&mut result, token)?;
            cursor += consumed;
            continue;
        }

        // Unrecognised character — reject the expression.
        return None;
    }

    Some(result)
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Return true if the current position expects a unary (prefix) operator.
///
/// A minus is unary when it appears:
///   - At the very start of the expression (no prior tokens)
///   - After another operator (+, −, *, /, ^, %, unary−)
///   - After an opening parenthesis
fn is_unary_position(result: &LexResult) -> bool {
    if result.token_count == 0 { return true; }
    matches!(
        result.tokens[result.token_count - 1],
        Token::Plus | Token::Minus | Token::Star | Token::Slash |
        Token::Percent | Token::Caret | Token::UnaryMinus | Token::LeftParen
    )
}

/// Parse a decimal or integer literal from the start of `slice`.
///
/// Converts the parsed value directly to Q20.12 fixed-point.
/// Returns (fixed_point_value, bytes_consumed) or None on parse failure.
fn parse_number_literal(slice: &[u8]) -> Option<(i32, usize)> {
    let mut cursor = 0usize;
    let mut integer_part: i64 = 0;
    let mut frac_part: i64 = 0;
    let mut frac_divisor: i64 = 1;
    let mut has_frac = false;

    // Integer digits before the decimal point.
    while cursor < slice.len() && slice[cursor].is_ascii_digit() {
        integer_part = integer_part
            .checked_mul(10)?
            .checked_add((slice[cursor] - b'0') as i64)?;
        cursor += 1;
    }

    // Optional decimal point and fractional digits.
    if cursor < slice.len() && slice[cursor] == b'.' {
        cursor += 1;
        has_frac = true;
        while cursor < slice.len() && slice[cursor].is_ascii_digit() {
            frac_part = frac_part
                .checked_mul(10)?
                .checked_add((slice[cursor] - b'0') as i64)?;
            frac_divisor *= 10;
            cursor += 1;
        }
    }

    // Convert to Q20.12.
    // integer_part × SCALE + frac_part × SCALE / frac_divisor
    let scaled_integer = integer_part * (fixed_point::SCALE as i64);
    let scaled_frac = if has_frac {
        (frac_part * (fixed_point::SCALE as i64)) / frac_divisor
    } else {
        0
    };

    let total = scaled_integer.checked_add(scaled_frac)?;
    // Ensure it fits in i32.
    if total > i32::MAX as i64 || total < i32::MIN as i64 { return None; }

    Some((total as i32, cursor))
}

/// Parse an identifier (function name, constant, or variable) from `slice`.
///
/// Identifiers are ASCII alphabetic sequences, matched case-insensitively.
/// Returns (Token, bytes_consumed) or None if unrecognised.
fn parse_identifier(slice: &[u8]) -> Option<(Token, usize)> {
    // Find the end of the identifier.
    let mut len = 0usize;
    while len < slice.len() && slice[len].is_ascii_alphabetic() {
        len += 1;
    }
    if len == 0 { return None; }

    // Copy to a small stack buffer and lowercase for case-insensitive matching.
    // Maximum identifier length we support is 5 characters (e.g. "floor", "round").
    if len > 5 { return None; } // No known identifier is longer than 5 chars
    let mut lower = [0u8; 5];
    for i in 0..len {
        lower[i] = slice[i].to_ascii_lowercase();
    }
    let ident = &lower[..len];

    let token = match ident {
        b"sin"   => Token::FuncSin,
        b"cos"   => Token::FuncCos,
        b"tan"   => Token::FuncTan,
        b"asin"  => Token::FuncAsin,
        b"acos"  => Token::FuncAcos,
        b"atan"  => Token::FuncAtan,
        b"sqrt"  => Token::FuncSqrt,
        b"abs"   => Token::FuncAbs,
        b"log"   => Token::FuncLog,
        b"ln"    => Token::FuncLn,
        b"log2"  => Token::FuncLog2,
        b"exp"   => Token::FuncExp,
        b"floor" => Token::FuncFloor,
        b"ceil"  => Token::FuncCeil,
        b"round" => Token::FuncRound,
        b"deg"   => Token::FuncDeg,
        b"rad"   => Token::FuncRad,
        b"pi"    => Token::ConstPi,
        b"ans"   => Token::VarAns,
        b"e"     => Token::ConstE,
        // Single-letter variable registers A–F.
        _ if len == 1 && lower[0] >= b'a' && lower[0] <= b'f' => {
            Token::VarRegister(lower[0].to_ascii_uppercase())
        }
        _ => return None, // Unrecognised identifier
    };

    Some((token, len))
}

/// Append a token to the result, returning None if capacity is exceeded.
fn append_token(result: &mut LexResult, token: Token) -> Option<()> {
    if result.token_count >= MAX_TOKEN_COUNT { return None; }
    result.tokens[result.token_count] = token;
    result.token_count += 1;
    Some(())
}