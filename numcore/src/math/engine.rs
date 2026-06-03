use super::fixed_point;
use super::lexer::LexResult;
use super::matrix::{Matrix, MatrixKind};
use super::parser::ParseTree;
use super::vars::VariableStore;
use super::{evaluator, lexer, parser};
use super::{AngleMode, MathMode};
pub use evaluator::EvalResult;

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
    evaluator::evaluate_tree(parse_scratch, variables, angle_mode, mode)
}

pub fn format_result<'a>(mat: &Matrix, mode: MathMode, buffer: &'a mut [u8; 48]) -> &'a [u8] {
    match (mat.kind, mode) {
        (MatrixKind::Complex, MathMode::Standard) => {
            fixed_point::format_fixed_point(mat.data[0], buffer)
        }
        (MatrixKind::Complex, _) => format_complex(mat.data[0], mat.data[1], buffer),
        (MatrixKind::Scalar, _) => fixed_point::format_fixed_point(mat.data[0], buffer),
        (MatrixKind::Mat, _) => {
            let s = match mat.rows {
                1 => b"1x1" as &[u8],
                _ => b"Mat" as &[u8],
            };
            let len = s.len().min(48);
            buffer[..len].copy_from_slice(s);
            &buffer[..len]
        }
    }
}

pub fn format_result_uart<'a>(mat: &Matrix, buffer: &'a mut [u8; 192]) -> &'a [u8] {
    match mat.kind {
        MatrixKind::Scalar => {
            let mut tmp = [0u8; 48];
            let s = fixed_point::format_fixed_point(mat.data[0], &mut tmp);
            let len = s.len().min(192);
            buffer[..len].copy_from_slice(s);
            &buffer[..len]
        }
        MatrixKind::Complex => {
            let mut tmp = [0u8; 48];
            let s = format_complex(mat.data[0], mat.data[1], &mut tmp);
            let len = s.len().min(192);
            buffer[..len].copy_from_slice(s);
            &buffer[..len]
        }
        MatrixKind::Mat => {
            let mut pos = 0usize;
            for r in 0..mat.rows as usize {
                if r == 0 {
                    if pos < 192 {
                        let s = b"[ ";
                        for &b in s {
                            buffer[pos] = b;
                            pos += 1;
                        }
                    }
                } else {
                    if pos < 192 {
                        let s = b"\r\n  [ ";
                        for &b in s {
                            buffer[pos] = b;
                            pos += 1;
                        }
                    }
                }
                for c in 0..mat.cols as usize {
                    if c > 0 {
                        if pos < 192 {
                            buffer[pos] = b' ';
                            pos += 1;
                        }
                    }
                    let val = mat.data[r * mat.cols as usize + c];
                    let mut tmp = [0u8; 24];
                    let s = fixed_point::format_fixed_point(val, &mut tmp);
                    for &b in s {
                        if pos < 192 {
                            buffer[pos] = b;
                            pos += 1;
                        }
                    }
                }
                if pos < 192 {
                    buffer[pos] = b' ';
                    pos += 1;
                }
                if pos < 192 {
                    buffer[pos] = b']';
                    pos += 1;
                }
            }
            &buffer[..pos]
        }
    }
}

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

fn format_complex(re: i64, im: i64, buffer: &mut [u8; 48]) -> &[u8] {
    if im == 0 {
        return fixed_point::format_fixed_point(re, buffer);
    }

    let mut pos = 0usize;

    if re != 0 {
        let real_str = fixed_point::format_fixed_point(re, buffer);
        let real_len = real_str.len();
        pos = real_len;
    }

    let mut tmp = [0u8; 24];
    let abs_im = if im < 0 { -im } else { im };
    let im_str = fixed_point::format_fixed_point(abs_im, &mut tmp);

    if pos > 0 {
        if im > 0 {
            buffer[pos] = b'+';
        } else {
            buffer[pos] = b'-';
        }
        pos += 1;
    } else if im < 0 {
        buffer[pos] = b'-';
        pos += 1;
    }

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
