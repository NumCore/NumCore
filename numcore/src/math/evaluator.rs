use super::complex::Complex;
use super::distributions;
use super::fixed_point as fp;
use super::parser::{
    AstNode, BinaryOperator, LoopOperation, MathConstant, MathFunction, ParseTree,
    ThreeArgMathFunction, TwoArgMathFunction, VariableRef,
};
use super::vars::VariableStore;
use super::AngleMode;

#[derive(Clone, Copy, Debug)]
pub enum EvalResult {
    Value(Complex),
    Overflow {
        mantissa: i64,
        exponent: i32,
        negative: bool,
    },
    DomainError,
}

static mut LAST_OVERFLOW_INFO: Option<(i64, bool)> = None;

fn set_overflow_info(log10_est: i64, negative: bool) {
    unsafe { LAST_OVERFLOW_INFO = Some((log10_est, negative)); }
}

fn take_overflow_info() -> Option<(i64, bool)> {
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

fn compute_overflow(log10_est: i64, negative: bool) -> EvalResult {
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

/// log10(2) ≈ 0.30102999566 in Q31.32 = round(log10(2) × 2^32)
const LOG10_2: i64 = 1_292_913_986;

fn overflow_complex_hyp(result: Option<Complex>, component: i64, negative: bool) -> Option<Complex> {
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

pub fn evaluate_tree(
    tree: &ParseTree,
    variables: &mut VariableStore,
    angle_mode: AngleMode,
) -> EvalResult {
    take_overflow_info();
    match evaluate_node(tree, tree.root_index, variables, angle_mode) {
        Some(v) => EvalResult::Value(v),
        None => match take_overflow_info() {
            Some((log10_est, negative)) => compute_overflow(log10_est, negative),
            None => EvalResult::DomainError,
        },
    }
}

fn evaluate_node(
    tree: &ParseTree,
    node_index: usize,
    vars: &mut VariableStore,
    angle_mode: AngleMode,
) -> Option<Complex> {
    match tree.nodes[node_index] {
        AstNode::Literal(value) => Some(Complex::from_real(value)),

        AstNode::Constant(constant) => Some(match constant {
            MathConstant::Pi => Complex::from_real(fp::FIXED_PI),
            MathConstant::E => Complex::from_real(fp::FIXED_E),
            MathConstant::ImaginaryUnit => Complex::new(0, fp::FIXED_ONE),
        }),

        AstNode::Variable(var_ref) => match var_ref {
            VariableRef::Ans => vars.read_ans(),
            VariableRef::Register(ch) => vars.read_register(ch),
        },

        AstNode::UnaryNegation { operand_index } => {
            match evaluate_node(tree, operand_index, vars, angle_mode) {
                Some(v) => Some(v.neg()),
                None => {
                    if let Some((log10_est, negative)) = take_overflow_info() {
                        set_overflow_info(log10_est, !negative);
                    }
                    None
                }
            }
        }

        AstNode::BinaryOperation {
            operator,
            left_child_index,
            right_child_index,
        } => {
            let left = evaluate_node(tree, left_child_index, vars, angle_mode);
            let left_info = if left.is_none() { take_overflow_info() } else { None };
            let right = evaluate_node(tree, right_child_index, vars, angle_mode);
            let right_info = if right.is_none() { take_overflow_info() } else { None };

            match (left, right) {
                (Some(l), Some(r)) => apply_binary_operator(operator, l, r),
                (left_val, right_val) => {
                    // Propagate overflow info, adjusting for the operation when possible
                    let info = match (left_info, right_info) {
                        (Some(l), Some(r)) => Some(if l.0 >= r.0 { l } else { r }),
                        (a, b) => a.or(b),
                    };
                    if let Some((log10_est, negative)) = info {
                        match (operator, left_val, right_val) {
                            (BinaryOperator::Divide, _, Some(r)) if r.re != 0 => {
                                if let Some(d) = fp::log10(r.re.unsigned_abs() as i64) {
                                    set_overflow_info(log10_est.wrapping_sub(d), negative != (r.re < 0));
                                } else {
                                    set_overflow_info(log10_est, negative);
                                }
                            }
                            (BinaryOperator::Divide, Some(_), None) => {
                                // constant / overflow → result is ~0, not overflow
                            }
                            (BinaryOperator::Multiply, None, Some(r)) => {
                                if let Some(m) = fp::log10(r.re.unsigned_abs() as i64) {
                                    set_overflow_info(log10_est.wrapping_add(m), negative != (r.re < 0));
                                } else {
                                    set_overflow_info(log10_est, negative);
                                }
                            }
                            (BinaryOperator::Multiply, Some(l), None) => {
                                if let Some(m) = fp::log10(l.re.unsigned_abs() as i64) {
                                    set_overflow_info(log10_est.wrapping_add(m), negative != (l.re < 0));
                                } else {
                                    set_overflow_info(log10_est, negative);
                                }
                            }
                            (BinaryOperator::Power, None, Some(r)) if r.im == 0 && r.re > 0 => {
                                if let Some(p) = fp::multiply(log10_est, r.re) {
                                    set_overflow_info(p, negative);
                                }
                            }
                            (BinaryOperator::Power, Some(_), None) => {
                                // constant ^ overflow → uncomputable from log10 alone
                            }
                            (BinaryOperator::Power, None, Some(_)) => {
                                // overflow ^ negative/complex exponent → keep original
                                set_overflow_info(log10_est, negative);
                            }
                            (BinaryOperator::Add | BinaryOperator::Subtract, None, Some(r)) => {
                                if r.re < 0 {
                                    set_overflow_info(log10_est, !negative);
                                } else {
                                    set_overflow_info(log10_est, negative);
                                }
                            }
                            (BinaryOperator::Add | BinaryOperator::Subtract, Some(_), None) => {
                                set_overflow_info(log10_est, negative);
                            }
                            _ => {
                                set_overflow_info(log10_est, negative);
                            }
                        }
                    }
                    None
                }
            }
        }

        AstNode::FunctionCall {
            function,
            argument_index,
        } => {
            let arg = evaluate_node(tree, argument_index, vars, angle_mode)?;
            apply_function(function, arg, angle_mode)
        }

        AstNode::ThreeArgFunction {
            function,
            arg_indices,
        } => {
            let a0 = evaluate_node(tree, arg_indices[0], vars, angle_mode)?;
            let a1 = evaluate_node(tree, arg_indices[1], vars, angle_mode)?;
            let a2 = evaluate_node(tree, arg_indices[2], vars, angle_mode)?;
            apply_three_arg_function(function, a0, a1, a2)
        }

        AstNode::TwoArgFunction {
            function,
            arg_indices,
        } => {
            let a0 = evaluate_node(tree, arg_indices[0], vars, angle_mode)?;
            let a1 = evaluate_node(tree, arg_indices[1], vars, angle_mode)?;
            apply_two_arg_function(function, a0, a1)
        }

        AstNode::Store {
            value_index,
            register,
        } => {
            let value = evaluate_node(tree, value_index, vars, angle_mode)?;
            vars.write_register(register, value);
            Some(value)
        }

        AstNode::LoopAggregate {
            operation,
            variable,
            start_index,
            end_index,
            body_index,
        } => {
            let start = evaluate_node(tree, start_index, vars, angle_mode)?;
            let end = evaluate_node(tree, end_index, vars, angle_mode)?;
            evaluate_loop_aggregate(
                operation, variable, start, end, body_index, tree, vars, angle_mode,
            )
        }
    }
}

fn apply_binary_operator(
    operator: BinaryOperator,
    left: Complex,
    right: Complex,
) -> Option<Complex> {
    match operator {
        BinaryOperator::Add => Some(left.add(right)),
        BinaryOperator::Subtract => Some(left.sub(right)),
        BinaryOperator::Multiply => match left.mul(right) {
            Some(v) => Some(v),
            None => {
                if left.re != 0 && right.re != 0 {
                    if let (Some(l), Some(r)) = (fp::log10(left.re.unsigned_abs() as i64), fp::log10(right.re.unsigned_abs() as i64)) {
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
                    if let (Some(l), Some(r)) = (fp::log10(left.re.unsigned_abs() as i64), fp::log10(right.re.unsigned_abs() as i64)) {
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
            Some(Complex::from_real(left.re % right.re))
        }
        BinaryOperator::Power => {
            if left.is_real() && right.is_real() {
                return match fp::power(left.re, right.re) {
                    Some(v) => Some(Complex::from_real(v)),
                    None => {
                        if left.re > 0 {
                            if let Some(log_left) = fp::log10(left.re) {
                                let log_est = fp::multiply(right.re, log_left).unwrap_or(fp::FIXED_ONE * 40);
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
                                let log_est = fp::multiply(fp::from_integer(exp_int), log).unwrap_or(i64::MAX);
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
                return Complex::from_polar(r_new, theta_new);
            }
            // General complex exponentiation: a^b = exp(b * ln(a))
            Complex::exp(right.mul(Complex::ln(left)?)?)
        }
    }
}

fn apply_function(function: MathFunction, arg: Complex, angle_mode: AngleMode) -> Option<Complex> {
    if !arg.is_real() {
        return match function {
            MathFunction::Sin => {
                let r = Complex::sin(arg);
                overflow_complex_hyp(r, arg.im, false)
            }
            MathFunction::Cos => {
                let r = Complex::cos(arg);
                overflow_complex_hyp(r, arg.im, false)
            }
            MathFunction::Tan => Complex::tan(arg),
            MathFunction::Asin => Complex::asin(arg),
            MathFunction::Acos => Complex::acos(arg),
            MathFunction::Atan => Complex::atan(arg),
            MathFunction::SinH => {
                let r = Complex::sinh(arg);
                overflow_complex_hyp(r, arg.re, false)
            }
            MathFunction::CosH => {
                let r = Complex::cosh(arg);
                overflow_complex_hyp(r, arg.re, false)
            }
            MathFunction::TanH => Complex::tanh(arg),
            MathFunction::ASinH => Complex::asinh(arg),
            MathFunction::ACosH => Complex::acosh(arg),
            MathFunction::ATanH => Complex::atanh(arg),
            MathFunction::Sqrt => Complex::sqrt(arg),
            MathFunction::Abs => {
                let norm = arg.norm_sq()?;
                Some(Complex::from_real(fp::sqrt(norm)?))
            }
            MathFunction::Log => Complex::log10(arg),
            MathFunction::Ln => Complex::ln(arg),
            MathFunction::Log2 => Complex::log2(arg),
            MathFunction::Exp => {
                let r = Complex::exp(arg);
                overflow_complex_hyp(r, arg.re, arg.re < 0)
            }
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
                match fp::degrees_to_radians(x) {
                    Some(v) => v,
                    None => {
                        if let Some(log) = approx_log10(x) {
                            set_overflow_info(log.wrapping_add(fp::FIXED_ONE * 3 / 4), x < 0);
                        }
                        return None;
                    }
                }
            } else {
                x
            };
            Some(Complex::from_real(fp::sin(rad)))
        }
        MathFunction::Cos => {
            let rad = if angle_mode == AngleMode::Degrees {
                match fp::degrees_to_radians(x) {
                    Some(v) => v,
                    None => {
                        if let Some(log) = approx_log10(x) {
                            set_overflow_info(log.wrapping_add(fp::FIXED_ONE * 3 / 4), x < 0);
                        }
                        return None;
                    }
                }
            } else {
                x
            };
            Some(Complex::from_real(fp::cos(rad)))
        }
        MathFunction::Tan => {
            let rad = if angle_mode == AngleMode::Degrees {
                match fp::degrees_to_radians(x) {
                    Some(v) => v,
                    None => {
                        if let Some(log) = approx_log10(x) {
                            set_overflow_info(log.wrapping_add(fp::FIXED_ONE * 3 / 4), x < 0);
                        }
                        return None;
                    }
                }
            } else {
                x
            };
            fp::tan(rad).map(Complex::from_real)
        }
        MathFunction::Asin => {
            match fp::asin(x) {
                Some(r) => {
                    let deg = if angle_mode == AngleMode::Degrees {
                        match fp::radians_to_degrees(r) {
                            Some(v) => v,
                            None => {
                                if let Some(log) = approx_log10(r) {
                                    set_overflow_info(log.wrapping_add(fp::FIXED_ONE + fp::FIXED_ONE * 3 / 4), r < 0);
                                }
                                return None;
                            }
                        }
                    } else {
                        r
                    };
                    Some(Complex::from_real(deg))
                }
                None => return None,
            }
        }
        MathFunction::Acos => {
            match fp::acos(x) {
                Some(r) => {
                    let deg = if angle_mode == AngleMode::Degrees {
                        match fp::radians_to_degrees(r) {
                            Some(v) => v,
                            None => {
                                if let Some(log) = approx_log10(r) {
                                    set_overflow_info(log.wrapping_add(fp::FIXED_ONE + fp::FIXED_ONE * 3 / 4), r < 0);
                                }
                                return None;
                            }
                        }
                    } else {
                        r
                    };
                    Some(Complex::from_real(deg))
                }
                None => return None,
            }
        }
        MathFunction::Atan => {
            let r = fp::atan(x);
            let deg = if angle_mode == AngleMode::Degrees {
                match fp::radians_to_degrees(r) {
                    Some(v) => v,
                    None => {
                        if let Some(log) = approx_log10(r) {
                            set_overflow_info(log.wrapping_add(fp::FIXED_ONE + fp::FIXED_ONE * 3 / 4), r < 0);
                        }
                        return None;
                    }
                }
            } else {
                r
            };
            Some(Complex::from_real(deg))
        }
        MathFunction::SinH => match fp::sinh(x) {
            Some(v) => Some(Complex::from_real(v)),
            None => {
                if x.abs() > fp::FIXED_ONE * 21 {
                    let log_est = fp::divide(x.abs(), fp::FIXED_LN10).unwrap_or(fp::FIXED_ONE * 10);
                    set_overflow_info(log_est.wrapping_sub(LOG10_2), x < 0);
                }
                None
            }
        },
        MathFunction::CosH => match fp::cosh(x) {
            Some(v) => Some(Complex::from_real(v)),
            None => {
                if x.abs() > fp::FIXED_ONE * 21 {
                    let log_est = fp::divide(x.abs(), fp::FIXED_LN10).unwrap_or(fp::FIXED_ONE * 10);
                    set_overflow_info(log_est.wrapping_sub(LOG10_2), false);
                }
                None
            }
        },
        MathFunction::TanH => fp::tanh(x).map(Complex::from_real),
        MathFunction::ASinH => {
            // asinh overflow is from intermediate multiply(x,x), not from the
            // result being too large (result ≈ ln(2x) which fits Q31.32 for
            // any x ≤ 2^31). Treat as domain error.
            fp::asinh(x).map(Complex::from_real)
        }
        MathFunction::ACosH => {
            // Same as asinh: intermediate overflow, not result overflow.
            fp::acosh(x).map(Complex::from_real)
        }
        MathFunction::ATanH => fp::atanh(x).map(Complex::from_real),
        MathFunction::Sqrt => {
            if x >= 0 {
                fp::sqrt(x).map(Complex::from_real)
            } else {
                Complex::sqrt(Complex::new(x, 0))
            }
        }
        MathFunction::Abs => Some(Complex::from_real(fp::abs(x))),
        MathFunction::Log => fp::log10(x).map(Complex::from_real),
        MathFunction::Ln => fp::natural_log(x).map(Complex::from_real),
        MathFunction::Log2 => fp::log2(x).map(Complex::from_real),
        MathFunction::Exp => match fp::natural_exp(x) {
            Some(v) => Some(Complex::from_real(v)),
            None => {
                let log_est = fp::divide(x, fp::FIXED_LN10).unwrap_or(0);
                set_overflow_info(log_est, x < 0);
                None
            }
        },
        MathFunction::Floor => Some(Complex::from_real(fp::floor(x))),
        MathFunction::Ceil => Some(Complex::from_real(fp::ceil(x))),
        MathFunction::Round => Some(Complex::from_real(fp::round(x))),
        MathFunction::Deg => match fp::degrees_to_radians(x) {
            Some(v) => Some(Complex::from_real(v)),
            None => {
                if let Some(log) = approx_log10(x) {
                    set_overflow_info(log.wrapping_add(fp::FIXED_ONE * 3 / 4), x < 0);
                }
                None
            }
        },
        MathFunction::Rad => match fp::radians_to_degrees(x) {
            Some(v) => Some(Complex::from_real(v)),
            None => {
                if let Some(log) = approx_log10(x) {
                    set_overflow_info(log.wrapping_add(fp::FIXED_ONE + fp::FIXED_ONE * 3 / 4), x < 0);
                }
                None
            }
        },
        MathFunction::LnGamma => distributions::ln_gamma(x).map(Complex::from_real),
    }
}

fn apply_two_arg_function(
    function: TwoArgMathFunction,
    a0: Complex,
    a1: Complex,
) -> Option<Complex> {
    if !a0.is_real() || !a1.is_real() {
        return None;
    }
    match function {
        TwoArgMathFunction::PoissonProbability => {
            distributions::poisson_probability(a0.re, a1.re).map(Complex::from_real)
        }
        TwoArgMathFunction::ChiSquaredCDF => {
            distributions::chi_squared_cdf(a0.re, a1.re).map(Complex::from_real)
        }
        TwoArgMathFunction::NthRoot => match fp::nthroot(a0.re, a1.re) {
            Some(v) => Some(Complex::from_real(v)),
            None => {
                if a0.re > 0 && a1.re != 0 {
                    let log_est = fp::divide(approx_log10(a0.re).unwrap_or(fp::FIXED_ONE), a1.re).unwrap_or(0);
                    set_overflow_info(log_est, a0.re < 0);
                }
                None
            }
        },
    }
}

fn apply_three_arg_function(
    function: ThreeArgMathFunction,
    a0: Complex,
    a1: Complex,
    a2: Complex,
) -> Option<Complex> {
    if !a0.is_real() || !a1.is_real() || !a2.is_real() {
        return None;
    }
    match function {
        ThreeArgMathFunction::BinomialProbability => {
            distributions::binomial_probability(a0.re, a1.re, a2.re).map(Complex::from_real)
        }
    }
}

const INTEGRATION_SNAP_THRESHOLD: i64 = 4295;

// ─── Adaptive Simpson integration ──────────────────────────────────────────

const ADAPTIVE_TOL: i64 = 43; // τ ≈ 1e-8 in Q31.32 (43 ULP)
const ADAPTIVE_MAX_DEPTH: u32 = 20;
const ADAPTIVE_MAX_EVALS: u32 = 10_000;
const ADAPTIVE_MAX_STACK: usize = 24;

#[derive(Clone, Copy)]
struct AdSimpTask {
    a: i64,
    b: i64,
    fa_re: i64,
    fb_re: i64,
    tol: i64,
    depth: u32,
}

fn simpson_step(h: i64, fa: i64, fm: i64, fb: i64) -> Option<i64> {
    let ws = (fa as i128) + 4 * (fm as i128) + (fb as i128);
    let prod = (h as i128) * ws;
    let result = prod / (6 * fp::SCALE as i128);
    if result > i64::MAX as i128 || result < i64::MIN as i128 {
        return None;
    }
    Some(result as i64)
}

fn adaptive_simpson_integrate(
    variable: u8,
    start: Complex,
    end: Complex,
    body_index: usize,
    tree: &ParseTree,
    vars: &mut VariableStore,
    angle_mode: AngleMode,
) -> Option<Complex> {
    if start.im != 0 || end.im != 0 {
        return None;
    }

    let (a, b, negate) = if start.re <= end.re {
        (start.re, end.re, false)
    } else {
        (end.re, start.re, true)
    };

    vars.write_register(variable, Complex::from_real(a));
    let fa = evaluate_node(tree, body_index, vars, angle_mode)?;
    if !fa.is_real() {
        return None;
    }

    vars.write_register(variable, Complex::from_real(b));
    let fb = evaluate_node(tree, body_index, vars, angle_mode)?;
    if !fb.is_real() {
        return None;
    }

    if a == b {
        return Some(Complex::zero());
    }

    let mut stack: [AdSimpTask; ADAPTIVE_MAX_STACK] = [AdSimpTask {
        a: 0,
        b: 0,
        fa_re: 0,
        fb_re: 0,
        tol: 0,
        depth: 0,
    }; ADAPTIVE_MAX_STACK];
    let mut stack_len: u32 = 1;
    let mut result: i64 = 0;
    let mut total_evals: u32 = 2;

    stack[0] = AdSimpTask {
        a,
        b,
        fa_re: fa.re,
        fb_re: fb.re,
        tol: ADAPTIVE_TOL,
        depth: 0,
    };

    while stack_len > 0 {
        stack_len -= 1;
        let task = stack[stack_len as usize];

        if task.depth >= ADAPTIVE_MAX_DEPTH || total_evals >= ADAPTIVE_MAX_EVALS {
            let h = task.b.wrapping_sub(task.a);
            let m = task.a + (h >> 1);
            vars.write_register(variable, Complex::from_real(m));
            let fm = evaluate_node(tree, body_index, vars, angle_mode)?;
            if !fm.is_real() {
                return None;
            }
            total_evals += 1;
            let s = simpson_step(h, task.fa_re, fm.re, task.fb_re)?;
            result = result.saturating_add(s);
            continue;
        }

        let h = task.b.checked_sub(task.a)?;
        let m = task.a + (h >> 1);

        vars.write_register(variable, Complex::from_real(m));
        let fm = evaluate_node(tree, body_index, vars, angle_mode)?;
        if !fm.is_real() {
            return None;
        }
        total_evals += 1;

        let s_ab = simpson_step(h, task.fa_re, fm.re, task.fb_re)?;

        let am_mid = task.a + ((m - task.a) >> 1);
        vars.write_register(variable, Complex::from_real(am_mid));
        let fl = evaluate_node(tree, body_index, vars, angle_mode)?;
        if !fl.is_real() {
            return None;
        }
        total_evals += 1;

        let mb_mid = m + ((task.b - m) >> 1);
        vars.write_register(variable, Complex::from_real(mb_mid));
        let fr = evaluate_node(tree, body_index, vars, angle_mode)?;
        if !fr.is_real() {
            return None;
        }
        total_evals += 1;

        let s_am = simpson_step(m - task.a, task.fa_re, fl.re, fm.re)?;
        let s_mb = simpson_step(task.b - m, fm.re, fr.re, task.fb_re)?;

        let error = (s_am as i128)
            .wrapping_add(s_mb as i128)
            .wrapping_sub(s_ab as i128);
        let error_abs = if error < 0 {
            error.wrapping_neg() as u128
        } else {
            error as u128
        };
        let threshold = 15u128 * (task.tol as u128);

        if error_abs < threshold {
            result = result.saturating_add(s_am.saturating_add(s_mb));
        } else {
            let child_tol = (task.tol >> 1).max(1);
            let new_len = stack_len + 2;
            if (new_len as usize) < stack.len() {
                let idx = stack_len as usize;
                stack[idx] = AdSimpTask {
                    a: m,
                    b: task.b,
                    fa_re: fm.re,
                    fb_re: task.fb_re,
                    tol: child_tol,
                    depth: task.depth + 1,
                };
                stack[idx + 1] = AdSimpTask {
                    a: task.a,
                    b: m,
                    fa_re: task.fa_re,
                    fb_re: fm.re,
                    tol: child_tol,
                    depth: task.depth + 1,
                };
                stack_len = new_len;
            } else {
                result = result.saturating_add(s_am.saturating_add(s_mb));
            }
        }
    }

    let final_result = if negate {
        result.wrapping_neg()
    } else {
        result
    };

    let nearest = fp::round(final_result);
    if (final_result - nearest).abs() < INTEGRATION_SNAP_THRESHOLD {
        Some(Complex::from_real(nearest))
    } else {
        Some(Complex::from_real(final_result))
    }
}

fn evaluate_loop_aggregate(
    operation: LoopOperation,
    variable: u8,
    start: Complex,
    end: Complex,
    body_index: usize,
    tree: &ParseTree,
    vars: &mut VariableStore,
    angle_mode: AngleMode,
) -> Option<Complex> {
    let saved = vars.read_register(variable);

    let result = (|| match operation {
        LoopOperation::Summation => {
            if start.im != 0 || end.im != 0 {
                return None;
            }
            if start.re & (fp::SCALE - 1) != 0 {
                return None;
            }
            if end.re & (fp::SCALE - 1) != 0 {
                return None;
            }

            let start_int = fp::to_integer_truncated(start.re);
            let end_int = fp::to_integer_truncated(end.re);

            if end_int < start_int {
                return Some(Complex::zero());
            }
            if end_int - start_int > 10_000 {
                return None;
            }

            let mut accumulator = Complex::zero();
            let mut k = start_int;
            while k <= end_int {
                vars.write_register(variable, Complex::from_real(fp::from_integer(k)));
                let term = evaluate_node(tree, body_index, vars, angle_mode)?;
                accumulator = accumulator.add(term);
                k += 1;
            }
            Some(accumulator)
        }

        LoopOperation::Integration => {
            adaptive_simpson_integrate(variable, start, end, body_index, tree, vars, angle_mode)
        }
    })();

    if let Some(val) = saved {
        vars.write_register(variable, val);
    }
    result
}
