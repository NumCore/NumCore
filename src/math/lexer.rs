//! # Lexer (Math Engine — Layer 6)
//!
//! Converts an expression byte slice into a flat array of typed `Token` values.
//!
//! ## Full token set
//!   Numbers    — integer and decimal literals  (e.g. 42, 3.14)
//!   Operators  — +  -  *  /  ^  %
//!   Grouping   — (  )
//!   Functions  — sin cos tan asin acos atan sqrt abs log ln log2 exp
//!                floor ceil round deg rad sto
//!   Constants  — pi  e
//!   Variables  — Ans  A B C D E F G H I J K L M N O P Q R S T U V W X Y Z
//!
//! ## Lexing rules
//!   - Whitespace is discarded between tokens
//!   - Identifiers are case-sensitive (sin only, not SIN or Sin)
//!   - A minus sign following an operator, opening paren, or at the
//!     start of the expression is classified as UnaryMinus, not Minus,
//!     so the parser can build a unary-negation node correctly
//!   - Decimal numbers are parsed and immediately converted to Q31.32

use super::fixed_point;

/// Maximum tokens from one expression. A 64-char expression can yield at most
/// ~32 operator/operand pairs; 128 gives generous headroom.
pub const MAX_TOKEN_COUNT: usize = 32;
// Sized for the LM3S811's 8 KB SRAM. A 64-char expression yields at most
// ~20 tokens in practice; 32 gives headroom without blowing the stack.

// ─── Token ────────────────────────────────────────────────────────────────────

/// A single lexical unit in a calculator expression.
///
/// All numeric values are stored as Q31.32 fixed-point (i32).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Token {
    // ── Literals ──
    /// A numeric literal already converted to Q31.32.
    Number(i64),

    // ── Binary operators (in precedence order, low → high) ──
    Plus,    // +
    Minus,   // −  (binary subtraction)
    Star,    // ×
    Slash,   // ÷
    Percent, // %  (modulo)
    Caret,   // ^  (exponentiation, right-associative)
    Comma,   // ,  (argument separator in multi-arg functions)

    // ── Unary operator ──
    UnaryMinus, // −  (negation, prefix)

    // ── Grouping ──
    LeftParen,  // (
    RightParen, // )

    // ── Named functions (all take one argument in parens) ──
    FuncSinH,
    FuncCosH,
    FuncTanH,
    FuncASinH,
    FuncACosH,
    FuncATanH,
    FuncSin,
    FuncCos,
    FuncTan,
    FuncAsin,
    FuncAcos,
    FuncAtan,
    FuncSqrt,
    FuncAbs,
    FuncLog, // log10
    FuncLn,  // natural log
    FuncLog2,
    FuncExp, // e^x
    FuncFloor,
    FuncCeil,
    FuncRound,
    FuncDeg, // radians → degrees
    FuncRad, // degrees → radians

    // ── Multi-argument functions ──
    // These take the form: name(expr, var, start, end) or name(n, k, p)
    FuncSum,      // sum(expr, var, start, end) — Σ
    FuncInt,      // int(expr, var, a, b)       — ∫ via Simpson's rule
    FuncBinomP,   // binomP(n, k, p)
    FuncPoissonP, // poissonP(lambda, k)
    FuncChiCDF,   // chiCDF(x, k)
    FuncNthRoot,  // nthRoot(x, n)
    FuncSto,      // sto(value, var) — store value into a register
    FuncLnGamma,  // lnGamma(x)

    // ── Named constants ──
    ConstPi, // π
    ConstE,  // e (Euler's number)

    // ── Variables ──
    VarAns,          // Ans
    VarRegister(u8), // A–Z (stores the letter byte)
}

// ─── LexResult ────────────────────────────────────────────────────────────────

/// Output of a successful lex pass: a fixed-size token array plus a count.
pub struct LexResult {
    pub tokens: [Token; MAX_TOKEN_COUNT],
    pub token_count: usize,
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Tokenise an expression byte slice, writing tokens into the provided scratch buffer.
///
/// Resets `result` before use so stale tokens from prior calls are never visible.
/// Returns `None` if any unrecognised character or malformed number is found.
/// Whitespace is discarded. Identifiers are case-sensitive.
pub fn tokenise_expression(expression: &[u8], mut result: &mut LexResult) -> Option<()> {
    // Reset scratch buffer — must clear token_count so old tokens are invisible.
    result.token_count = 0;

    let mut cursor = 0usize;

    while cursor < expression.len() {
        // Skip whitespace.
        if expression[cursor] == b' ' {
            cursor += 1;
            continue;
        }

        // ── Number literal ────────────────────────────────────────────────────
        if expression[cursor].is_ascii_digit()
            || (expression[cursor] == b'.'
                && cursor + 1 < expression.len()
                && expression[cursor + 1].is_ascii_digit())
        {
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
            b',' => Some(Token::Comma),
            _ => None,
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

    Some(())
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Return true if the current position expects a unary (prefix) operator.
///
/// A minus is unary when it appears:
///   - At the very start of the expression (no prior tokens)
///   - After another operator (+, −, *, /, ^, %, unary−)
///   - After an opening parenthesis or comma (argument separator)
fn is_unary_position(result: &LexResult) -> bool {
    if result.token_count == 0 {
        return true;
    }
    matches!(
        result.tokens[result.token_count - 1],
        Token::Plus
            | Token::Minus
            | Token::Star
            | Token::Slash
            | Token::Percent
            | Token::Caret
            | Token::UnaryMinus
            | Token::LeftParen
            | Token::Comma
    )
}

/// Parse a decimal or integer literal from the start of `slice`.
///
/// Converts the parsed value directly to Q31.32 fixed-point.
/// Returns (fixed_point_value, bytes_consumed) or None on parse failure.
fn parse_number_literal(slice: &[u8]) -> Option<(i64, usize)> {
    let mut cursor = 0usize;
    let mut integer_part: i128 = 0;
    let mut frac_part: i128 = 0;
    let mut frac_divisor: i128 = 1;
    let mut has_frac = false;

    // Integer digits before the decimal point.
    while cursor < slice.len() && slice[cursor].is_ascii_digit() {
        integer_part = integer_part
            .checked_mul(10)?
            .checked_add((slice[cursor] - b'0') as i128)?;
        cursor += 1;
    }

    // Optional decimal point and fractional digits.
    if cursor < slice.len() && slice[cursor] == b'.' {
        cursor += 1;
        has_frac = true;
        while cursor < slice.len() && slice[cursor].is_ascii_digit() {
            frac_part = frac_part
                .checked_mul(10)?
                .checked_add((slice[cursor] - b'0') as i128)?;
            frac_divisor *= 10;
            cursor += 1;
        }
    }

    // Convert to Q31.32.
    // integer_part × SCALE + frac_part × SCALE / frac_divisor
    let scaled_integer = integer_part * (fixed_point::SCALE as i128);
    let scaled_frac = if has_frac {
        (frac_part * (fixed_point::SCALE as i128)) / frac_divisor
    } else {
        0
    };

    let total = scaled_integer.checked_add(scaled_frac)?;
    // Ensure it fits in i64.
    if total > i64::MAX as i128 || total < i64::MIN as i128 {
        return None;
    }

    Some((total as i64, cursor))
}

/// Parse an identifier (function name, constant, or variable) from `slice`.
///
/// Identifiers are case-sensitive. Function names and constants (`sin`, `pi`,
/// `e`, `ans`, `sto`) are always lowercase. Single uppercase letters A–Z
/// are variable registers. Single lowercase letters are unrecognised.
/// Returns (Token, bytes_consumed) or None if unrecognised.
fn parse_identifier(slice: &[u8]) -> Option<(Token, usize)> {
    // Identifiers start with a letter and may continue with letters or digits.
    // This allows names like "log2", "log10" to lex correctly as a single token.
    let mut len = 0usize;
    if len < slice.len() && slice[len].is_ascii_alphabetic() {
        len += 1;
        while len < slice.len() && slice[len].is_ascii_alphanumeric() {
            len += 1;
        }
    }
    if len == 0 {
        return None;
    }

    if len > 8 {
        return None;
    } // Longest identifier: "poissonp" = 8 chars
    let ident = &slice[..len];

    let token = match ident {
        b"log2" => Token::FuncLog2, // Must be before "log" to avoid prefix match
        b"sinh" => Token::FuncSinH,
        b"cosh" => Token::FuncCosH,
        b"tanh" => Token::FuncTanH,
        b"asinh" => Token::FuncASinH,
        b"acosh" => Token::FuncACosH,
        b"atanh" => Token::FuncATanH,
        b"sin" => Token::FuncSin,
        b"cos" => Token::FuncCos,
        b"tan" => Token::FuncTan,
        b"asin" => Token::FuncAsin,
        b"acos" => Token::FuncAcos,
        b"atan" => Token::FuncAtan,
        b"sqrt" => Token::FuncSqrt,
        b"nthroot" => Token::FuncNthRoot,
        b"sto" => Token::FuncSto,
        b"abs" => Token::FuncAbs,
        b"log" => Token::FuncLog,
        b"ln" => Token::FuncLn,
        b"exp" => Token::FuncExp,
        b"floor" => Token::FuncFloor,
        b"ceil" => Token::FuncCeil,
        b"round" => Token::FuncRound,
        b"deg" => Token::FuncDeg,
        b"rad" => Token::FuncRad,
        b"sum" => Token::FuncSum,
        b"int" => Token::FuncInt,
        b"binomp" => Token::FuncBinomP,
        b"poissonp" => Token::FuncPoissonP,
        b"chicdf" => Token::FuncChiCDF,
        b"lngamma" => Token::FuncLnGamma,
        b"pi" => Token::ConstPi,
        b"ans" => Token::VarAns,
        b"e" => Token::ConstE,
        // Single uppercase letter = variable register A–Z.
        _ if len == 1 && ident[0] >= b'A' && ident[0] <= b'Z' => {
            Token::VarRegister(ident[0])
        }
        _ => return None, // Unrecognised identifier
    };

    Some((token, len))
}

/// Append a token to the result, returning None if capacity is exceeded.
fn append_token(result: &mut LexResult, token: Token) -> Option<()> {
    if result.token_count >= MAX_TOKEN_COUNT {
        return None;
    }
    result.tokens[result.token_count] = token;
    result.token_count += 1;
    Some(())
}
