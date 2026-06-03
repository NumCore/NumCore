use super::fixed_point;
use super::MathMode;

pub const MAX_TOKEN_COUNT: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Token {
    Number(i64),
    Exponent(i64),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    Comma,
    UnaryMinus,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
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
    FuncLog,
    FuncLn,
    FuncLog2,
    FuncExp,
    FuncFloor,
    FuncCeil,
    FuncRound,
    FuncDeg,
    FuncRad,
    FuncSum,
    FuncInt,
    FuncDet,
    FuncTranspose,
    FuncIdentity,
    FuncInv,
    FuncCofactor,
    FuncAdjugate,
    FuncBinomP,
    FuncPoissonP,
    FuncChiCDF,
    FuncNthRoot,
    FuncSto,
    FuncLnGamma,
    ConstPi,
    ConstE,
    ConstI,
    VarAns,
    VarRegister(u8),
    MatRegister(u8),
}

pub struct LexResult {
    pub tokens: [Token; MAX_TOKEN_COUNT],
    pub token_count: usize,
}

pub fn tokenise_expression(
    expression: &[u8],
    mut result: &mut LexResult,
    mode: MathMode,
) -> Option<()> {
    result.token_count = 0;

    let mut cursor = 0usize;

    while cursor < expression.len() {
        if expression[cursor] == b' ' {
            cursor += 1;
            continue;
        }

        if expression[cursor].is_ascii_digit()
            || (expression[cursor] == b'.'
                && cursor + 1 < expression.len()
                && expression[cursor + 1].is_ascii_digit())
        {
            let (fp_value, consumed) = parse_number_literal(&expression[cursor..])?;
            append_token(&mut result, Token::Number(fp_value))?;
            cursor += consumed;

            // In Scientific mode, check for 'E' exponential suffix
            if mode == MathMode::Scientific
                && cursor < expression.len()
                && expression[cursor] == b'E'
            {
                let mut e_cursor = cursor + 1;
                let sign = if e_cursor < expression.len() && expression[e_cursor] == b'+' {
                    e_cursor += 1;
                    1i64
                } else if e_cursor < expression.len() && expression[e_cursor] == b'-' {
                    e_cursor += 1;
                    -1i64
                } else {
                    1i64
                };
                let exp_start = e_cursor;
                while e_cursor < expression.len() && expression[e_cursor].is_ascii_digit() {
                    e_cursor += 1;
                }
                if e_cursor > exp_start {
                    // Reject fractional or multi-digit exponents immediately
                    // followed by a letter (e.g. "2E1.2" or "2E10A").
                    let bad_follow = expression
                        .get(e_cursor)
                        .map_or(false, |&b| b == b'.' || b.is_ascii_alphanumeric());
                    if !bad_follow {
                        let mut exp_val: i64 = 0;
                        for &b in &expression[exp_start..e_cursor] {
                            exp_val = exp_val.checked_mul(10)?;
                            exp_val = exp_val.checked_add((b - b'0') as i64)?;
                        }
                        append_token(&mut result, Token::Exponent(sign * exp_val))?;
                        cursor = e_cursor;
                    }
                }
            }
            continue;
        }

        if mode == MathMode::Advanced
            && cursor < expression.len()
            && expression[cursor] == b'i'
            && (cursor + 1 >= expression.len() || !expression[cursor + 1].is_ascii_alphanumeric())
        {
            append_token(&mut result, Token::ConstI)?;
            cursor += 1;
            continue;
        }

        let single = match expression[cursor] {
            b'(' => Some(Token::LeftParen),
            b')' => Some(Token::RightParen),
            b'+' => Some(Token::Plus),
            b'*' => Some(Token::Star),
            b'/' => Some(Token::Slash),
            b'%' => Some(Token::Percent),
            b'^' => Some(Token::Caret),
            b',' => Some(Token::Comma),
            b'[' if mode == MathMode::Matrix => Some(Token::LeftBracket),
            b']' if mode == MathMode::Matrix => Some(Token::RightBracket),
            _ => None,
        };
        if let Some(token) = single {
            append_token(&mut result, token)?;
            cursor += 1;
            continue;
        }

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

        if expression[cursor].is_ascii_alphabetic() {
            let Some((token, consumed)) = parse_identifier(&expression[cursor..], mode) else {
                return None;
            };
            append_token(&mut result, token)?;
            cursor += consumed;
            continue;
        }

        return None;
    }

    Some(())
}

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
            | Token::LeftBracket
            | Token::Comma
    )
}

fn parse_number_literal(slice: &[u8]) -> Option<(i64, usize)> {
    let mut cursor = 0usize;
    let mut integer_part: i128 = 0;
    let mut frac_part: i128 = 0;
    let mut frac_divisor: i128 = 1;
    let mut has_frac = false;

    while cursor < slice.len() && slice[cursor].is_ascii_digit() {
        integer_part = integer_part
            .checked_mul(10)?
            .checked_add((slice[cursor] - b'0') as i128)?;
        cursor += 1;
    }

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

    let scaled_integer = integer_part * (fixed_point::SCALE as i128);
    let scaled_frac = if has_frac {
        (frac_part * (fixed_point::SCALE as i128)) / frac_divisor
    } else {
        0
    };

    let total = scaled_integer.checked_add(scaled_frac)?;
    if total > i64::MAX as i128 || total < i64::MIN as i128 {
        return None;
    }

    Some((total as i64, cursor))
}

fn parse_identifier(slice: &[u8], mode: MathMode) -> Option<(Token, usize)> {
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

    if len > 12 {
        return None;
    }
    let ident = &slice[..len];

    let token: Token;
    // Match multi-char identifiers first, then single-char.
    match ident {
        b"log2" => token = Token::FuncLog2,
        b"sinh" => token = Token::FuncSinH,
        b"cosh" => token = Token::FuncCosH,
        b"tanh" => token = Token::FuncTanH,
        b"asinh" => token = Token::FuncASinH,
        b"acosh" => token = Token::FuncACosH,
        b"atanh" => token = Token::FuncATanH,
        b"sin" => token = Token::FuncSin,
        b"cos" => token = Token::FuncCos,
        b"tan" => token = Token::FuncTan,
        b"asin" => token = Token::FuncAsin,
        b"acos" => token = Token::FuncAcos,
        b"atan" => token = Token::FuncAtan,
        b"sqrt" => token = Token::FuncSqrt,
        b"nthroot" => token = Token::FuncNthRoot,
        b"sto" => token = Token::FuncSto,
        b"abs" => token = Token::FuncAbs,
        b"log" => token = Token::FuncLog,
        b"ln" => token = Token::FuncLn,
        b"exp" => token = Token::FuncExp,
        b"floor" => token = Token::FuncFloor,
        b"ceil" => token = Token::FuncCeil,
        b"round" => token = Token::FuncRound,
        b"deg" => token = Token::FuncDeg,
        b"rad" => token = Token::FuncRad,
        b"sum" => token = Token::FuncSum,
        b"int" => token = Token::FuncInt,
        b"binomp" => token = Token::FuncBinomP,
        b"poissonp" => token = Token::FuncPoissonP,
        b"chicdf" => token = Token::FuncChiCDF,
        b"lngamma" => token = Token::FuncLnGamma,
        b"pi" => token = Token::ConstPi,
        b"ans" | b"Ans" => token = Token::VarAns,
        b"e" => token = Token::ConstE,
        _ => {
            if ident.len() == 4 && ident[0..3].eq_ignore_ascii_case(b"mat") {
                let letter = ident[3].to_ascii_uppercase();
                if letter >= b'A' && letter <= b'C' {
                    token = Token::MatRegister(letter);
                } else {
                    return None;
                }
            } else if ident.len() == 1 && ident[0] >= b'A' && ident[0] <= b'Z' {
                token = Token::VarRegister(ident[0]);
            } else if mode == MathMode::Matrix {
                // Matrix-mode-specific function identifiers
                if ident.eq_ignore_ascii_case(b"det") {
                    token = Token::FuncDet;
                } else if ident.eq_ignore_ascii_case(b"transpose") {
                    token = Token::FuncTranspose;
                } else if ident.eq_ignore_ascii_case(b"identity") {
                    token = Token::FuncIdentity;
                } else if ident.eq_ignore_ascii_case(b"inv")
                    || ident.eq_ignore_ascii_case(b"inverse")
                {
                    token = Token::FuncInv;
                } else if ident.eq_ignore_ascii_case(b"cofactor") {
                    token = Token::FuncCofactor;
                } else if ident.eq_ignore_ascii_case(b"adj")
                    || ident.eq_ignore_ascii_case(b"adjugate")
                {
                    token = Token::FuncAdjugate;
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
    };

    Some((token, len))
}

fn append_token(result: &mut LexResult, token: Token) -> Option<()> {
    if result.token_count >= MAX_TOKEN_COUNT {
        return None;
    }
    result.tokens[result.token_count] = token;
    result.token_count += 1;
    Some(())
}
