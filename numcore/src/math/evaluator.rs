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
            None
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

const SIMPSONS_INTERVALS: i64 = 100;
const INTEGRATION_SNAP_THRESHOLD: i64 = 4295;

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
            if start.im != 0 || end.im != 0 {
                return None;
            }

            let n = SIMPSONS_INTERVALS;

            let range = end.re.checked_sub(start.re)?;
            let h = fp::divide(range, fp::from_integer(n))?;

            vars.write_register(variable, start);
            let f_start = evaluate_node(tree, body_index, vars, angle_mode)?;

            vars.write_register(variable, end);
            let f_end = evaluate_node(tree, body_index, vars, angle_mode)?;

            let mut sum = f_start.add(f_end);

            for i in 1..n {
                let i_fp = fp::from_integer(i);
                let x = start.re.checked_add(fp::multiply(i_fp, h)?)?;

                vars.write_register(variable, Complex::from_real(x));
                let f_x = evaluate_node(tree, body_index, vars, angle_mode)?;

                let coeff = if i % 2 == 1 { 4 } else { 2 };

                let term = Complex::from_real(fp::from_integer(coeff)).mul(f_x)?;
                sum = sum.add(term);
            }

            let h_times_sum = Complex::from_real(h).mul(sum)?;
            let result = h_times_sum.div(Complex::from_real(fp::from_integer(3)))?;

            let nearest = fp::round(result.re);

            if (result.re - nearest).abs() < INTEGRATION_SNAP_THRESHOLD {
                Some(Complex::new(nearest, result.im))
            } else {
                Some(result)
            }
        }
    })();

    if let Some(val) = saved {
        vars.write_register(variable, val);
    }
    result
}
