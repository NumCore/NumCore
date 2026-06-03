use super::complex::Complex as Cplx;
use super::distributions;
use super::fixed_point as fp;
use super::matrix::{normalize_scientific, Matrix, MatrixKind};
use super::parser::{
    BinaryOperator, MathFunction, MatrixFunction, ThreeArgMathFunction, TwoArgMathFunction,
};
use super::AngleMode;

#[derive(Clone, Copy, Debug)]
pub enum EvalResult {
    Matrix(Matrix),
    Overflow {
        mantissa: i64,
        exponent: i32,
        negative: bool,
    },
    DomainError,
}

static mut LAST_OVERFLOW_INFO: Option<(i64, bool)> = None;

pub fn set_overflow_info(log10_est: i64, negative: bool) {
    unsafe {
        LAST_OVERFLOW_INFO = Some((log10_est, negative));
    }
}

pub fn take_overflow_info() -> Option<(i64, bool)> {
    unsafe {
        let val = LAST_OVERFLOW_INFO;
        LAST_OVERFLOW_INFO = None;
        val
    }
}

fn approx_log10(val: i64) -> Option<i64> {
    if val == 0 {
        return None;
    }
    let abs = val.unsigned_abs();
    let bits = 62 - abs.leading_zeros();
    let int_bits = bits.saturating_sub(32) as i64;
    if int_bits >= 0 {
        let mut int_part = abs >> 32;
        if int_part == 0 {
            int_part = 1;
        }
        let mut log = 0i64;
        while int_part >= 10 {
            int_part /= 10;
            log += 1;
        }
        Some(log * fp::SCALE)
    } else {
        let frac = abs & 0xFFFF_FFFF;
        let lz = frac.leading_zeros();
        if lz >= 32 {
            Some(-9 * fp::SCALE)
        } else {
            let est = -(lz as i64 + 1);
            Some(est * fp::SCALE * 3 / 10)
        }
    }
}

pub fn compute_overflow(log10_est: i64, negative: bool) -> EvalResult {
    let exp_int = log10_est >> 32;
    let frac_q31 = log10_est - (exp_int << 32);
    let frac_times_ln10 = fp::multiply(frac_q31, fp::FIXED_LN10);
    let mantissa = match frac_times_ln10 {
        Some(v) => fp::natural_exp(v).unwrap_or(fp::FIXED_ONE),
        None => fp::FIXED_ONE,
    };
    let exponent = if exp_int > i32::MAX as i64 {
        i32::MAX
    } else if exp_int < i32::MIN as i64 {
        i32::MIN
    } else {
        exp_int as i32
    };
    EvalResult::Overflow {
        mantissa,
        exponent,
        negative,
    }
}

const LOG10_2: i64 = 1_292_913_986;

pub fn overflow_complex_hyp(result: Option<Cplx>, component: i64, negative: bool) -> Option<Cplx> {
    match result {
        Some(v) => Some(v),
        None => {
            if component.abs() > fp::FIXED_ONE * 21 {
                let log_est =
                    fp::divide(component.abs(), fp::FIXED_LN10).unwrap_or(fp::FIXED_ONE * 10);
                set_overflow_info(log_est.wrapping_sub(LOG10_2), negative);
            }
            None
        }
    }
}

// ─── Binary operator dispatch ────────────────────────────────────────

pub fn apply_binary_operator(
    operator: BinaryOperator,
    left: Matrix,
    right: Matrix,
) -> Option<Matrix> {
    use MatrixKind as MK;
    match (left.kind, right.kind) {
        (MK::Scalar, MK::Scalar) => {
            let l = Cplx::from_real(left.data[0]);
            let r = Cplx::from_real(right.data[0]);
            apply_binary_op_complex(operator, l, r).map(Matrix::from_complex)
        }
        (MK::Complex, MK::Scalar) => {
            let l = Cplx::new(left.data[0], left.data[1]);
            let r = Cplx::from_real(right.data[0]);
            apply_binary_op_complex(operator, l, r).map(|c| {
                if c.im == 0 {
                    Matrix::scalar(c.re)
                } else {
                    Matrix::complex(c.re, c.im)
                }
            })
        }
        (MK::Scalar, MK::Complex) => {
            let l = Cplx::from_real(left.data[0]);
            let r = Cplx::new(right.data[0], right.data[1]);
            apply_binary_op_complex(operator, l, r).map(|c| {
                if c.im == 0 {
                    Matrix::scalar(c.re)
                } else {
                    Matrix::complex(c.re, c.im)
                }
            })
        }
        (MK::Complex, MK::Complex) => {
            let l = Cplx::new(left.data[0], left.data[1]);
            let r = Cplx::new(right.data[0], right.data[1]);
            apply_binary_op_complex(operator, l, r).map(|c| {
                if c.im == 0 {
                    Matrix::scalar(c.re)
                } else {
                    Matrix::complex(c.re, c.im)
                }
            })
        }
        (MK::Mat, MK::Mat) => match operator {
            BinaryOperator::Add => left.elementwise_add(&right),
            BinaryOperator::Subtract => left.elementwise_sub(&right),
            BinaryOperator::Multiply => left.matmul(&right),
            _ => None,
        },
        (MK::Mat, MK::Scalar) => {
            let k = right.data[0];
            match operator {
                BinaryOperator::Add => left.scalar_add(k),
                BinaryOperator::Subtract => left.scalar_sub(k),
                BinaryOperator::Multiply => left.scalar_mul(k),
                _ => None,
            }
        }
        (MK::Scalar, MK::Mat) => {
            let k = left.data[0];
            match operator {
                BinaryOperator::Add => right.scalar_add(k),
                BinaryOperator::Subtract => right.scalar_sub(k).map(|m| m.negate()),
                BinaryOperator::Multiply => right.scalar_mul(k),
                _ => None,
            }
        }
        (MK::Complex, MK::Mat) | (MK::Mat, MK::Complex) => None,

        // ── Scientific notation arms ──
        (MK::Scientific, _) | (_, MK::Scientific) => {
            if operator == BinaryOperator::Modulo {
                return None;
            }
            apply_sci_binary(operator, left, right)
        }
    }
}

#[inline(never)]
fn apply_sci_binary(operator: BinaryOperator, left: Matrix, right: Matrix) -> Option<Matrix> {
    use MatrixKind as MK;
    match (left.kind, right.kind) {
        (MK::Scientific, MK::Scientific) => apply_sci_sci(operator, left, right),
        (MK::Scientific, MK::Scalar) => {
            let r_sci = Matrix::to_scientific_value(&right)?;
            apply_sci_sci(operator, left, Matrix::scientific(r_sci.0, r_sci.1)?)
        }
        (MK::Scalar, MK::Scientific) => {
            let l_sci = Matrix::to_scientific_value(&left)?;
            apply_sci_sci(operator, Matrix::scientific(l_sci.0, l_sci.1)?, right)
        }
        _ => None,
    }
}

pub(crate) fn sci_to_scalar(mantissa: i64, exponent: i64) -> Option<i64> {
    if mantissa & (fp::SCALE - 1) != 0 {
        return None;
    }
    let int_mant = mantissa >> 32;
    if exponent == 0 {
        return Some(int_mant << 32);
    }
    if exponent > 0 {
        let pow10 = fp::integer_power(fp::from_integer(10), exponent)?;
        fp::multiply(int_mant << 32, pow10)
    } else {
        let mut val = int_mant << 32;
        for _ in 0..(-exponent) {
            val = fp::divide(val, fp::from_integer(10))?;
        }
        Some(val)
    }
}

fn check_exp_overflow(total_exp: i64, negative: bool) -> Option<()> {
    if total_exp > 99 || total_exp < -99 {
        let log10_est = if total_exp > 0 {
            fp::multiply(fp::from_integer(total_exp), fp::FIXED_ONE)
                .unwrap_or(total_exp * fp::SCALE)
        } else {
            fp::multiply(fp::from_integer(total_exp), fp::FIXED_ONE)
                .unwrap_or(total_exp * fp::SCALE)
        };
        set_overflow_info(log10_est, negative);
        None
    } else {
        Some(())
    }
}

fn apply_sci_sci(op: BinaryOperator, left: Matrix, right: Matrix) -> Option<Matrix> {
    let (m1, e1) = left.to_scientific()?;
    let (m2, e2) = right.to_scientific()?;
    // Helper: try to convert a Scientific result to Scalar if it fits in Q31.32
    fn result_or_scalar(m: Option<Matrix>) -> Option<Matrix> {
        let mat = m?;
        if let Some((mant, exp)) = mat.to_scientific() {
            // Only convert positive/zero exponents where multiply is exact.
            // Negative exponents lose precision from repeated division.
            if exp >= 0 && exp <= 8 {
                if let Some(v) = sci_to_scalar(mant, exp) {
                    return Some(Matrix::scalar(v));
                }
            }
            if exp == 9 {
                if let Some(v) = sci_to_scalar(mant, 9) {
                    return Some(Matrix::scalar(v));
                }
            }
            Some(mat)
        } else {
            Some(mat)
        }
    }

    match op {
        BinaryOperator::Multiply => {
            let m = fp::multiply(m1, m2)?;
            let (m_n, e_adj) = normalize_scientific(m, 0)?;
            let total = e1 + e2 + e_adj;
            check_exp_overflow(total, m_n < 0)?;
            result_or_scalar(Matrix::scientific(m_n, total))
        }
        BinaryOperator::Divide => {
            let m = fp::divide(m1, m2)?;
            let (m_n, e_adj) = normalize_scientific(m, 0)?;
            let total = e1 - e2 + e_adj;
            check_exp_overflow(total, m_n < 0)?;
            result_or_scalar(Matrix::scientific(m_n, total))
        }
        BinaryOperator::Add | BinaryOperator::Subtract => {
            let (m_small, m_large, e_small, e_large) = if e1 >= e2 {
                (m2, m1, e2, e1)
            } else {
                (m1, m2, e1, e2)
            };
            let diff = e_large - e_small;
            if diff > 9 {
                return result_or_scalar(Matrix::scientific(m_large, e_large));
            }
            let mut scaled = m_small;
            for _ in 0..diff {
                scaled = fp::divide(scaled, fp::from_integer(10))?;
            }
            let m = if op == BinaryOperator::Add {
                m_large.checked_add(scaled)?
            } else {
                m_large.checked_sub(scaled)?
            };
            if m == 0 {
                return Some(Matrix::scalar(0));
            }
            let (m_n, e_adj) = normalize_scientific(m, 0)?;
            let total = e_large + e_adj;
            check_exp_overflow(total, m_n < 0)?;
            result_or_scalar(Matrix::scientific(m_n, total))
        }
        BinaryOperator::Power => {
            let m2_val = if e2 == 0 {
                m2
            } else {
                let mut v = m2;
                if e2 > 0 {
                    for _ in 0..e2 {
                        v = fp::multiply(v, fp::from_integer(10))?;
                    }
                } else {
                    for _ in 0..(-e2) {
                        v = fp::divide(v, fp::from_integer(10))?;
                    }
                }
                v
            };
            let m = fp::power(m1, m2_val)?;
            let e = fp::multiply(fp::from_integer(e1), m2_val)?;
            let e_int = fp::to_integer_truncated(e);
            let (m_n, e_adj) = normalize_scientific(m, 0)?;
            let total = e_int + e_adj;
            check_exp_overflow(total, m_n < 0)?;
            result_or_scalar(Matrix::scientific(m_n, total))
        }
        BinaryOperator::Modulo => None,
    }
}

pub fn apply_binary_op_complex(operator: BinaryOperator, left: Cplx, right: Cplx) -> Option<Cplx> {
    match operator {
        BinaryOperator::Add => Some(left.add(right)),
        BinaryOperator::Subtract => Some(left.sub(right)),
        BinaryOperator::Multiply => match left.mul(right) {
            Some(v) => Some(v),
            None => {
                if left.re != 0 && right.re != 0 {
                    if let (Some(l), Some(r)) = (
                        fp::log10(left.re.unsigned_abs() as i64),
                        fp::log10(right.re.unsigned_abs() as i64),
                    ) {
                        set_overflow_info(l.wrapping_add(r), (left.re < 0) != (right.re < 0));
                    }
                }
                None
            }
        },
        BinaryOperator::Divide => match left.div(right) {
            Some(v) => Some(v),
            None => {
                if left.re != 0 && right.re != 0 {
                    if let (Some(l), Some(r)) = (
                        fp::log10(left.re.unsigned_abs() as i64),
                        fp::log10(right.re.unsigned_abs() as i64),
                    ) {
                        set_overflow_info(l.wrapping_sub(r), (left.re < 0) != (right.re < 0));
                    }
                }
                None
            }
        },
        BinaryOperator::Modulo => {
            if left.im != 0 || right.im != 0 || right.re == 0 {
                return None;
            }
            Some(Cplx::from_real(left.re % right.re))
        }
        BinaryOperator::Power => {
            if left.is_real() && right.is_real() {
                return match fp::power(left.re, right.re) {
                    Some(v) => Some(Cplx::from_real(v)),
                    None => {
                        if left.re > 0 {
                            if let Some(log_left) = fp::log10(left.re) {
                                let log_est =
                                    fp::multiply(right.re, log_left).unwrap_or(fp::FIXED_ONE * 40);
                                set_overflow_info(log_est, false);
                            }
                        }
                        None
                    }
                };
            }
            if right.is_real() {
                let exp_int = fp::to_integer_truncated(right.re);
                if right.re == fp::from_integer(exp_int) {
                    return match left.integer_pow(exp_int) {
                        Some(v) => Some(v),
                        None => {
                            if let Some(log) = fp::log10(left.re) {
                                let log_est = fp::multiply(fp::from_integer(exp_int), log)
                                    .unwrap_or(i64::MAX);
                                set_overflow_info(log_est, false);
                            }
                            None
                        }
                    };
                }
                let norm = left.norm_sq()?;
                let r = fp::sqrt(norm)?;
                let theta = left.arg();
                let r_new = fp::power(r, right.re)?;
                let theta_new = fp::multiply(theta, right.re)?;
                return Cplx::from_polar(r_new, theta_new);
            }
            Cplx::exp(right.mul(Cplx::ln(left)?)?)
        }
    }
}

// ─── Function dispatch ──────────────────────────────────────────────

pub fn apply_function(function: MathFunction, arg: Cplx, angle_mode: AngleMode) -> Option<Cplx> {
    if !arg.is_real() {
        return match function {
            MathFunction::Sin => overflow_complex_hyp(Cplx::sin(arg), arg.im, false),
            MathFunction::Cos => overflow_complex_hyp(Cplx::cos(arg), arg.im, false),
            MathFunction::Tan => Cplx::tan(arg),
            MathFunction::Asin => Cplx::asin(arg),
            MathFunction::Acos => Cplx::acos(arg),
            MathFunction::Atan => Cplx::atan(arg),
            MathFunction::SinH => overflow_complex_hyp(Cplx::sinh(arg), arg.re, false),
            MathFunction::CosH => overflow_complex_hyp(Cplx::cosh(arg), arg.re, false),
            MathFunction::TanH => Cplx::tanh(arg),
            MathFunction::ASinH => Cplx::asinh(arg),
            MathFunction::ACosH => Cplx::acosh(arg),
            MathFunction::ATanH => Cplx::atanh(arg),
            MathFunction::Sqrt => Cplx::sqrt(arg),
            MathFunction::Abs => {
                let norm = arg.norm_sq()?;
                Some(Cplx::from_real(fp::sqrt(norm)?))
            }
            MathFunction::Log => Cplx::log10(arg),
            MathFunction::Ln => Cplx::ln(arg),
            MathFunction::Log2 => Cplx::log2(arg),
            MathFunction::Exp => overflow_complex_hyp(Cplx::exp(arg), arg.re, arg.re < 0),
            MathFunction::Floor
            | MathFunction::Ceil
            | MathFunction::Round
            | MathFunction::Deg
            | MathFunction::Rad
            | MathFunction::LnGamma => None,
        };
    }
    let x = arg.re;
    match function {
        MathFunction::Sin => {
            let rad = if angle_mode == AngleMode::Degrees {
                fp::degrees_to_radians(x).or_else(|| {
                    if let Some(log) = approx_log10(x) {
                        set_overflow_info(log.wrapping_add(fp::FIXED_ONE * 3 / 4), x < 0);
                    }
                    None
                })?
            } else {
                x
            };
            Some(Cplx::from_real(fp::sin(rad)))
        }
        MathFunction::Cos => {
            let rad = if angle_mode == AngleMode::Degrees {
                fp::degrees_to_radians(x).or_else(|| {
                    if let Some(log) = approx_log10(x) {
                        set_overflow_info(log.wrapping_add(fp::FIXED_ONE * 3 / 4), x < 0);
                    }
                    None
                })?
            } else {
                x
            };
            Some(Cplx::from_real(fp::cos(rad)))
        }
        MathFunction::Tan => {
            let rad = if angle_mode == AngleMode::Degrees {
                fp::degrees_to_radians(x).or_else(|| {
                    if let Some(log) = approx_log10(x) {
                        set_overflow_info(log.wrapping_add(fp::FIXED_ONE * 3 / 4), x < 0);
                    }
                    None
                })?
            } else {
                x
            };
            fp::tan(rad).map(Cplx::from_real)
        }
        MathFunction::Asin => match fp::asin(x) {
            Some(r) => {
                let deg = if angle_mode == AngleMode::Degrees {
                    fp::radians_to_degrees(r).or_else(|| {
                        if let Some(log) = approx_log10(r) {
                            set_overflow_info(
                                log.wrapping_add(fp::FIXED_ONE + fp::FIXED_ONE * 3 / 4),
                                r < 0,
                            );
                        }
                        None
                    })?
                } else {
                    r
                };
                Some(Cplx::from_real(deg))
            }
            None => None,
        },
        MathFunction::Acos => match fp::acos(x) {
            Some(r) => {
                let deg = if angle_mode == AngleMode::Degrees {
                    fp::radians_to_degrees(r).or_else(|| {
                        if let Some(log) = approx_log10(r) {
                            set_overflow_info(
                                log.wrapping_add(fp::FIXED_ONE + fp::FIXED_ONE * 3 / 4),
                                r < 0,
                            );
                        }
                        None
                    })?
                } else {
                    r
                };
                Some(Cplx::from_real(deg))
            }
            None => None,
        },
        MathFunction::Atan => {
            let r = fp::atan(x);
            let deg = if angle_mode == AngleMode::Degrees {
                fp::radians_to_degrees(r).or_else(|| {
                    if let Some(log) = approx_log10(r) {
                        set_overflow_info(
                            log.wrapping_add(fp::FIXED_ONE + fp::FIXED_ONE * 3 / 4),
                            r < 0,
                        );
                    }
                    None
                })?
            } else {
                r
            };
            Some(Cplx::from_real(deg))
        }
        MathFunction::SinH => match fp::sinh(x) {
            Some(v) => Some(Cplx::from_real(v)),
            None => {
                if x.abs() > fp::FIXED_ONE * 21 {
                    let log_est = fp::divide(x.abs(), fp::FIXED_LN10).unwrap_or(fp::FIXED_ONE * 10);
                    set_overflow_info(log_est.wrapping_sub(LOG10_2), x < 0);
                }
                None
            }
        },
        MathFunction::CosH => match fp::cosh(x) {
            Some(v) => Some(Cplx::from_real(v)),
            None => {
                if x.abs() > fp::FIXED_ONE * 21 {
                    let log_est = fp::divide(x.abs(), fp::FIXED_LN10).unwrap_or(fp::FIXED_ONE * 10);
                    set_overflow_info(log_est.wrapping_sub(LOG10_2), false);
                }
                None
            }
        },
        MathFunction::TanH => fp::tanh(x).map(Cplx::from_real),
        MathFunction::ASinH => fp::asinh(x).map(Cplx::from_real),
        MathFunction::ACosH => fp::acosh(x).map(Cplx::from_real),
        MathFunction::ATanH => fp::atanh(x).map(Cplx::from_real),
        MathFunction::Sqrt => {
            if x >= 0 {
                fp::sqrt(x).map(Cplx::from_real)
            } else {
                Cplx::sqrt(Cplx::new(x, 0))
            }
        }
        MathFunction::Abs => Some(Cplx::from_real(fp::abs(x))),
        MathFunction::Log => fp::log10(x).map(Cplx::from_real),
        MathFunction::Ln => fp::natural_log(x).map(Cplx::from_real),
        MathFunction::Log2 => fp::log2(x).map(Cplx::from_real),
        MathFunction::Exp => match fp::natural_exp(x) {
            Some(v) => Some(Cplx::from_real(v)),
            None => {
                let log_est = fp::divide(x, fp::FIXED_LN10).unwrap_or(0);
                set_overflow_info(log_est, x < 0);
                None
            }
        },
        MathFunction::Floor => Some(Cplx::from_real(fp::floor(x))),
        MathFunction::Ceil => Some(Cplx::from_real(fp::ceil(x))),
        MathFunction::Round => Some(Cplx::from_real(fp::round(x))),
        MathFunction::Deg => fp::degrees_to_radians(x).map(Cplx::from_real).or_else(|| {
            if let Some(log) = approx_log10(x) {
                set_overflow_info(log.wrapping_add(fp::FIXED_ONE * 3 / 4), x < 0);
            }
            None
        }),
        MathFunction::Rad => fp::radians_to_degrees(x).map(Cplx::from_real).or_else(|| {
            if let Some(log) = approx_log10(x) {
                set_overflow_info(
                    log.wrapping_add(fp::FIXED_ONE + fp::FIXED_ONE * 3 / 4),
                    x < 0,
                );
            }
            None
        }),
        MathFunction::LnGamma => distributions::ln_gamma(x).map(Cplx::from_real),
    }
}

pub fn apply_two_arg_function(function: TwoArgMathFunction, a0: Cplx, a1: Cplx) -> Option<Cplx> {
    if !a0.is_real() || !a1.is_real() {
        return None;
    }
    match function {
        TwoArgMathFunction::PoissonProbability => {
            distributions::poisson_probability(a0.re, a1.re).map(Cplx::from_real)
        }
        TwoArgMathFunction::ChiSquaredCDF => {
            distributions::chi_squared_cdf(a0.re, a1.re).map(Cplx::from_real)
        }
        TwoArgMathFunction::NthRoot => match fp::nthroot(a0.re, a1.re) {
            Some(v) => Some(Cplx::from_real(v)),
            None => {
                if a0.re > 0 && a1.re != 0 {
                    let log_est = fp::divide(approx_log10(a0.re).unwrap_or(fp::FIXED_ONE), a1.re)
                        .unwrap_or(0);
                    set_overflow_info(log_est, a0.re < 0);
                }
                None
            }
        },
    }
}

pub fn apply_three_arg_function(
    function: ThreeArgMathFunction,
    a0: Cplx,
    a1: Cplx,
    a2: Cplx,
) -> Option<Cplx> {
    if !a0.is_real() || !a1.is_real() || !a2.is_real() {
        return None;
    }
    match function {
        ThreeArgMathFunction::BinomialProbability => {
            distributions::binomial_probability(a0.re, a1.re, a2.re).map(Cplx::from_real)
        }
    }
}

pub fn apply_matrix_function(function: MatrixFunction, arg: Matrix) -> Option<Matrix> {
    match function {
        MatrixFunction::Det => {
            let d = arg.determinant()?;
            Some(Matrix::scalar(d))
        }
        MatrixFunction::Transpose => arg.transpose(),
        MatrixFunction::Identity => {
            let n = arg.to_complex()?.re;
            let int_n = fp::to_integer_truncated(n);
            if int_n < 1 || int_n as usize > 4 {
                return None;
            }
            Matrix::identity(int_n as u8)
        }
        MatrixFunction::Inv => arg.inverse(),
        MatrixFunction::Cofactor => arg.cofactor(),
        MatrixFunction::Adjugate => arg.adjugate(),
    }
}

// ─── Integration constants (used by vm.rs) ──────────────────────────

pub const ADAPTIVE_TOL: i64 = 43;
pub const ADAPTIVE_MAX_DEPTH: u32 = 20;
pub const ADAPTIVE_MAX_EVALS: u32 = 10_000;
