use super::complex::Complex;
use super::distributions;
use super::fixed_point as fp;
use super::parser::{
    AstNode, BinaryOperator, LoopOperation, MathConstant, MathFunction, ParseTree,
    ThreeArgMathFunction, TwoArgMathFunction, VariableRef,
};
use super::vars::VariableStore;
use super::AngleMode;

pub fn evaluate_tree(
    tree: &ParseTree,
    variables: &mut VariableStore,
    angle_mode: AngleMode,
) -> Option<Complex> {
    evaluate_node(tree, tree.root_index, variables, angle_mode)
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
            let value = evaluate_node(tree, operand_index, vars, angle_mode)?;
            Some(value.neg())
        }

        AstNode::BinaryOperation {
            operator,
            left_child_index,
            right_child_index,
        } => {
            let left = evaluate_node(tree, left_child_index, vars, angle_mode)?;
            let right = evaluate_node(tree, right_child_index, vars, angle_mode)?;
            apply_binary_operator(operator, left, right)
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
        BinaryOperator::Multiply => left.mul(right),
        BinaryOperator::Divide => left.div(right),
        BinaryOperator::Modulo => {
            if left.im != 0 || right.im != 0 || right.re == 0 {
                return None;
            }
            Some(Complex::from_real(left.re % right.re))
        }
        BinaryOperator::Power => {
            if left.is_real() && right.is_real() {
                return fp::power(left.re, right.re).map(Complex::from_real);
            }
            if right.is_real() {
                let exp_int = fp::to_integer_truncated(right.re);
                if right.re == fp::from_integer(exp_int) {
                    return left.integer_pow(exp_int);
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
            MathFunction::Sin => Complex::sin(arg),
            MathFunction::Cos => Complex::cos(arg),
            MathFunction::Tan => Complex::tan(arg),
            MathFunction::Asin => Complex::asin(arg),
            MathFunction::Acos => Complex::acos(arg),
            MathFunction::Atan => Complex::atan(arg),
            MathFunction::SinH => Complex::sinh(arg),
            MathFunction::CosH => Complex::cosh(arg),
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
            MathFunction::Exp => Complex::exp(arg),
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
                fp::degrees_to_radians(x)?
            } else {
                x
            };
            Some(Complex::from_real(fp::sin(rad)))
        }
        MathFunction::Cos => {
            let rad = if angle_mode == AngleMode::Degrees {
                fp::degrees_to_radians(x)?
            } else {
                x
            };
            Some(Complex::from_real(fp::cos(rad)))
        }
        MathFunction::Tan => {
            let rad = if angle_mode == AngleMode::Degrees {
                fp::degrees_to_radians(x)?
            } else {
                x
            };
            fp::tan(rad).map(Complex::from_real)
        }
        MathFunction::Asin => {
            let r = fp::asin(x)?;
            let deg = if angle_mode == AngleMode::Degrees {
                fp::radians_to_degrees(r)?
            } else {
                r
            };
            Some(Complex::from_real(deg))
        }
        MathFunction::Acos => {
            let r = fp::acos(x)?;
            let deg = if angle_mode == AngleMode::Degrees {
                fp::radians_to_degrees(r)?
            } else {
                r
            };
            Some(Complex::from_real(deg))
        }
        MathFunction::Atan => {
            let r = fp::atan(x);
            let deg = if angle_mode == AngleMode::Degrees {
                fp::radians_to_degrees(r)?
            } else {
                r
            };
            Some(Complex::from_real(deg))
        }
        MathFunction::SinH => fp::sinh(x).map(Complex::from_real),
        MathFunction::CosH => fp::cosh(x).map(Complex::from_real),
        MathFunction::TanH => fp::tanh(x).map(Complex::from_real),
        MathFunction::ASinH => fp::asinh(x).map(Complex::from_real),
        MathFunction::ACosH => fp::acosh(x).map(Complex::from_real),
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
        MathFunction::Exp => fp::natural_exp(x).map(Complex::from_real),
        MathFunction::Floor => Some(Complex::from_real(fp::floor(x))),
        MathFunction::Ceil => Some(Complex::from_real(fp::ceil(x))),
        MathFunction::Round => Some(Complex::from_real(fp::round(x))),
        MathFunction::Deg => fp::degrees_to_radians(x).map(Complex::from_real),
        MathFunction::Rad => fp::radians_to_degrees(x).map(Complex::from_real),
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
        TwoArgMathFunction::NthRoot => fp::nthroot(a0.re, a1.re).map(Complex::from_real),
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

const ADAPTIVE_TOL: i64 = 43;       // τ ≈ 1e-8 in Q31.32 (43 ULP)
const ADAPTIVE_MAX_DEPTH: u32 = 20;
const ADAPTIVE_MAX_EVALS: u32 = 2000;
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
        a: 0, b: 0, fa_re: 0, fb_re: 0, tol: 0, depth: 0,
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

        let error = (s_am as i128).wrapping_add(s_mb as i128).wrapping_sub(s_ab as i128);
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

        LoopOperation::Integration => adaptive_simpson_integrate(
            variable,
            start,
            end,
            body_index,
            tree,
            vars,
            angle_mode,
        ),
    })();

    if let Some(val) = saved {
        vars.write_register(variable, val);
    }
    result
}
