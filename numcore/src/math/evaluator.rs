use super::complex::Complex as Cplx;
use super::distributions;
use super::fixed_point as fp;
use super::matrix::{Matrix, MatrixKind};
use super::parser::{
    AstNode, BinaryOperator, LoopOperation, MathConstant, MathFunction, MatrixFunction, ParseTree,
    StoreTarget, ThreeArgMathFunction, TwoArgMathFunction, VariableRef,
};
use super::vars::VariableStore;
use super::{AngleMode, MathMode};

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

fn set_overflow_info(log10_est: i64, negative: bool) {
    unsafe {
        LAST_OVERFLOW_INFO = Some((log10_est, negative));
    }
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

const LOG10_2: i64 = 1_292_913_986;

fn overflow_complex_hyp(result: Option<Cplx>, component: i64, negative: bool) -> Option<Cplx> {
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
    mode: MathMode,
) -> EvalResult {
    take_overflow_info();
    match evaluate_node(tree, tree.root_index, variables, angle_mode, mode) {
        Some(mat) => match (mat.kind, mode) {
            (MatrixKind::Complex, MathMode::Standard) => EvalResult::DomainError,
            (MatrixKind::Mat, MathMode::Standard) => EvalResult::DomainError,
            (MatrixKind::Mat, MathMode::Advanced) => EvalResult::DomainError,
            _ => EvalResult::Matrix(mat),
        },
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
    mode: MathMode,
) -> Option<Matrix> {
    match tree.nodes[node_index] {
        AstNode::Literal(value) => Some(Matrix::scalar(value)),

        AstNode::Constant(constant) => Some(match constant {
            MathConstant::Pi => Matrix::scalar(fp::FIXED_PI),
            MathConstant::E => Matrix::scalar(fp::FIXED_E),
            MathConstant::ImaginaryUnit => Matrix::complex(0, fp::FIXED_ONE),
        }),

        AstNode::Variable(var_ref) => match var_ref {
            VariableRef::Ans => {
                // Always try matrix Ans first, then fall back to scalar Ans.
                // This ensures Ans returns the most recent result of any kind.
                vars.read_matrix_ans().or_else(|| {
                    vars.read_ans().map(|c| {
                        if c.im == 0 {
                            Matrix::scalar(c.re)
                        } else {
                            Matrix::complex(c.re, c.im)
                        }
                    })
                })
            }
            VariableRef::Register(ch) => vars.read_register(ch).map(|c| {
                if c.im == 0 {
                    Matrix::scalar(c.re)
                } else {
                    Matrix::complex(c.re, c.im)
                }
            }),
        },

        AstNode::MatrixRegister(ch) => vars.read_matrix_reg(ch),

        AstNode::UnaryNegation { operand_index } => {
            match evaluate_node(tree, operand_index, vars, angle_mode, mode) {
                Some(v) => Some(v.negate()),
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
            // For commutative Multiply: if left is Scalar, extract value
            // and drop the Matrix before evaluating right (ARM codegen workaround).

            let left = evaluate_node(tree, left_child_index, vars, angle_mode, mode);
            let left_info = if left.is_none() {
                take_overflow_info()
            } else {
                None
            };
            let right = evaluate_node(tree, right_child_index, vars, angle_mode, mode);
            let right_info = if right.is_none() {
                take_overflow_info()
            } else {
                None
            };

            match (left, right) {
                (Some(l), Some(r)) => apply_binary_operator(operator, l, r),
                (left_val, right_val) => {
                    let info = match (left_info, right_info) {
                        (Some(l), Some(r)) => Some(if l.0 >= r.0 { l } else { r }),
                        (a, b) => a.or(b),
                    };
                    if let Some((log10_est, negative)) = info {
                        // Extract scalar values for overflow adjustment
                        let l_scalar = left_val.as_ref().and_then(|m| m.to_complex());
                        let r_scalar = right_val.as_ref().and_then(|m| m.to_complex());
                        match (operator, l_scalar, r_scalar) {
                            (BinaryOperator::Divide, _, Some(r)) if r.re != 0 => {
                                if let Some(d) = fp::log10(r.re.unsigned_abs() as i64) {
                                    set_overflow_info(
                                        log10_est.wrapping_sub(d),
                                        negative != (r.re < 0),
                                    );
                                } else {
                                    set_overflow_info(log10_est, negative);
                                }
                            }
                            (BinaryOperator::Divide, Some(_), None) => {}
                            (BinaryOperator::Multiply, None, Some(r)) => {
                                if let Some(m) = fp::log10(r.re.unsigned_abs() as i64) {
                                    set_overflow_info(
                                        log10_est.wrapping_add(m),
                                        negative != (r.re < 0),
                                    );
                                } else {
                                    set_overflow_info(log10_est, negative);
                                }
                            }
                            (BinaryOperator::Multiply, Some(l), None) => {
                                if let Some(m) = fp::log10(l.re.unsigned_abs() as i64) {
                                    set_overflow_info(
                                        log10_est.wrapping_add(m),
                                        negative != (l.re < 0),
                                    );
                                } else {
                                    set_overflow_info(log10_est, negative);
                                }
                            }
                            (BinaryOperator::Power, None, Some(r)) if r.im == 0 && r.re > 0 => {
                                if let Some(p) = fp::multiply(log10_est, r.re) {
                                    set_overflow_info(p, negative);
                                }
                            }
                            (BinaryOperator::Power, Some(_), None) => {}
                            (BinaryOperator::Power, None, Some(_)) => {
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
            let arg = evaluate_node(tree, argument_index, vars, angle_mode, mode)?;
            let c = arg.to_complex()?;
            apply_function(function, c, angle_mode).map(Matrix::from_complex)
        }

        AstNode::ThreeArgFunction {
            function,
            arg_indices,
        } => {
            let a0 = evaluate_node(tree, arg_indices[0], vars, angle_mode, mode)?;
            let a1 = evaluate_node(tree, arg_indices[1], vars, angle_mode, mode)?;
            let a2 = evaluate_node(tree, arg_indices[2], vars, angle_mode, mode)?;
            let c0 = a0.to_complex()?;
            let c1 = a1.to_complex()?;
            let c2 = a2.to_complex()?;
            apply_three_arg_function(function, c0, c1, c2).map(Matrix::from_complex)
        }

        AstNode::TwoArgFunction {
            function,
            arg_indices,
        } => {
            let a0 = evaluate_node(tree, arg_indices[0], vars, angle_mode, mode)?;
            let a1 = evaluate_node(tree, arg_indices[1], vars, angle_mode, mode)?;
            let c0 = a0.to_complex()?;
            let c1 = a1.to_complex()?;
            apply_two_arg_function(function, c0, c1).map(Matrix::from_complex)
        }

        AstNode::Store {
            value_index,
            register,
            target,
        } => {
            let value = evaluate_node(tree, value_index, vars, angle_mode, mode)?;
            match target {
                StoreTarget::Scalar => {
                    if let Some(c) = value.to_complex() {
                        vars.write_register(register, c);
                    } else {
                        return None;
                    }
                }
                StoreTarget::Matrix => {
                    if value.kind == MatrixKind::Mat || value.kind == MatrixKind::Scalar {
                        vars.write_matrix_reg(register, value);
                    } else {
                        return None;
                    }
                }
            }
            Some(value)
        }

        AstNode::LoopAggregate {
            operation,
            variable,
            start_index,
            end_index,
            body_index,
        } => {
            let start = evaluate_node(tree, start_index, vars, angle_mode, mode)?;
            let end = evaluate_node(tree, end_index, vars, angle_mode, mode)?;
            let start_c = start.to_complex()?;
            let end_c = end.to_complex()?;
            evaluate_loop_aggregate(
                operation, variable, start_c, end_c, body_index, tree, vars, angle_mode,
            )
            .map(Matrix::from_complex)
        }

        AstNode::MatrixLiteral { cache_index } => {
            tree.mat_cache.get(cache_index).copied().flatten()
        }

        AstNode::MatrixFunctionCall {
            function,
            argument_index,
        } => {
            let arg = evaluate_node(tree, argument_index, vars, angle_mode, mode)?;
            apply_matrix_function(function, arg)
        }
    }
}

fn apply_binary_operator(operator: BinaryOperator, left: Matrix, right: Matrix) -> Option<Matrix> {
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
    }
}

fn apply_binary_op_complex(operator: BinaryOperator, left: Cplx, right: Cplx) -> Option<Cplx> {
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

fn apply_function(function: MathFunction, arg: Cplx, angle_mode: AngleMode) -> Option<Cplx> {
    if !arg.is_real() {
        return match function {
            MathFunction::Sin => {
                let r = Cplx::sin(arg);
                overflow_complex_hyp(r, arg.im, false)
            }
            MathFunction::Cos => {
                let r = Cplx::cos(arg);
                overflow_complex_hyp(r, arg.im, false)
            }
            MathFunction::Tan => Cplx::tan(arg),
            MathFunction::Asin => Cplx::asin(arg),
            MathFunction::Acos => Cplx::acos(arg),
            MathFunction::Atan => Cplx::atan(arg),
            MathFunction::SinH => {
                let r = Cplx::sinh(arg);
                overflow_complex_hyp(r, arg.re, false)
            }
            MathFunction::CosH => {
                let r = Cplx::cosh(arg);
                overflow_complex_hyp(r, arg.re, false)
            }
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
            MathFunction::Exp => {
                let r = Cplx::exp(arg);
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
            Some(Cplx::from_real(fp::sin(rad)))
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
            Some(Cplx::from_real(fp::cos(rad)))
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
            fp::tan(rad).map(Cplx::from_real)
        }
        MathFunction::Asin => match fp::asin(x) {
            Some(r) => {
                let deg = if angle_mode == AngleMode::Degrees {
                    match fp::radians_to_degrees(r) {
                        Some(v) => v,
                        None => {
                            if let Some(log) = approx_log10(r) {
                                set_overflow_info(
                                    log.wrapping_add(fp::FIXED_ONE + fp::FIXED_ONE * 3 / 4),
                                    r < 0,
                                );
                            }
                            return None;
                        }
                    }
                } else {
                    r
                };
                Some(Cplx::from_real(deg))
            }
            None => return None,
        },
        MathFunction::Acos => match fp::acos(x) {
            Some(r) => {
                let deg = if angle_mode == AngleMode::Degrees {
                    match fp::radians_to_degrees(r) {
                        Some(v) => v,
                        None => {
                            if let Some(log) = approx_log10(r) {
                                set_overflow_info(
                                    log.wrapping_add(fp::FIXED_ONE + fp::FIXED_ONE * 3 / 4),
                                    r < 0,
                                );
                            }
                            return None;
                        }
                    }
                } else {
                    r
                };
                Some(Cplx::from_real(deg))
            }
            None => return None,
        },
        MathFunction::Atan => {
            let r = fp::atan(x);
            let deg = if angle_mode == AngleMode::Degrees {
                match fp::radians_to_degrees(r) {
                    Some(v) => v,
                    None => {
                        if let Some(log) = approx_log10(r) {
                            set_overflow_info(
                                log.wrapping_add(fp::FIXED_ONE + fp::FIXED_ONE * 3 / 4),
                                r < 0,
                            );
                        }
                        return None;
                    }
                }
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
        MathFunction::Deg => match fp::degrees_to_radians(x) {
            Some(v) => Some(Cplx::from_real(v)),
            None => {
                if let Some(log) = approx_log10(x) {
                    set_overflow_info(log.wrapping_add(fp::FIXED_ONE * 3 / 4), x < 0);
                }
                None
            }
        },
        MathFunction::Rad => match fp::radians_to_degrees(x) {
            Some(v) => Some(Cplx::from_real(v)),
            None => {
                if let Some(log) = approx_log10(x) {
                    set_overflow_info(
                        log.wrapping_add(fp::FIXED_ONE + fp::FIXED_ONE * 3 / 4),
                        x < 0,
                    );
                }
                None
            }
        },
        MathFunction::LnGamma => distributions::ln_gamma(x).map(Cplx::from_real),
    }
}

fn apply_two_arg_function(function: TwoArgMathFunction, a0: Cplx, a1: Cplx) -> Option<Cplx> {
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

fn apply_three_arg_function(
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

fn apply_matrix_function(function: MatrixFunction, arg: Matrix) -> Option<Matrix> {
    match function {
        MatrixFunction::Det => {
            let d = arg.determinant()?;
            Some(Matrix::scalar(d))
        }
        MatrixFunction::Transpose => arg.transpose(),
        MatrixFunction::Identity => {
            let n = arg.to_complex()?.re;
            let int_n = fp::to_integer_truncated(n);
            if int_n < 1 || int_n as usize > super::matrix::MAX_MATRIX_DIM {
                return None;
            }
            Matrix::identity(int_n as u8)
        }
        MatrixFunction::Inv => arg.inverse(),
        MatrixFunction::Cofactor => arg.cofactor(),
        MatrixFunction::Adjugate => arg.adjugate(),
    }
}

const INTEGRATION_SNAP_THRESHOLD: i64 = 4295;

const ADAPTIVE_TOL: i64 = 43;
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
    start: Cplx,
    end: Cplx,
    body_index: usize,
    tree: &ParseTree,
    vars: &mut VariableStore,
    angle_mode: AngleMode,
) -> Option<Cplx> {
    if start.im != 0 || end.im != 0 {
        return None;
    }

    let (a, b, negate) = if start.re <= end.re {
        (start.re, end.re, false)
    } else {
        (end.re, start.re, true)
    };

    vars.write_register(variable, Cplx::from_real(a));
    let fa = evaluate_node(tree, body_index, vars, angle_mode, MathMode::Advanced)?;
    let fa_c = fa.to_complex()?;
    if !fa_c.is_real() {
        return None;
    }

    vars.write_register(variable, Cplx::from_real(b));
    let fb = evaluate_node(tree, body_index, vars, angle_mode, MathMode::Advanced)?;
    let fb_c = fb.to_complex()?;
    if !fb_c.is_real() {
        return None;
    }

    if a == b {
        return Some(Cplx::zero());
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
        fa_re: fa_c.re,
        fb_re: fb_c.re,
        tol: ADAPTIVE_TOL,
        depth: 0,
    };

    while stack_len > 0 {
        stack_len -= 1;
        let task = stack[stack_len as usize];

        if task.depth >= ADAPTIVE_MAX_DEPTH || total_evals >= ADAPTIVE_MAX_EVALS {
            let h = task.b.wrapping_sub(task.a);
            let m = task.a + (h >> 1);
            vars.write_register(variable, Cplx::from_real(m));
            let fm = evaluate_node(tree, body_index, vars, angle_mode, MathMode::Advanced)?;
            let fm_c = fm.to_complex()?;
            if !fm_c.is_real() {
                return None;
            }
            total_evals += 1;
            let s = simpson_step(h, task.fa_re, fm_c.re, task.fb_re)?;
            result = result.saturating_add(s);
            continue;
        }

        let h = task.b.checked_sub(task.a)?;
        let m = task.a + (h >> 1);

        vars.write_register(variable, Cplx::from_real(m));
        let fm = evaluate_node(tree, body_index, vars, angle_mode, MathMode::Advanced)?;
        let fm_c = fm.to_complex()?;
        if !fm_c.is_real() {
            return None;
        }
        total_evals += 1;

        let s_ab = simpson_step(h, task.fa_re, fm_c.re, task.fb_re)?;

        let am_mid = task.a + ((m - task.a) >> 1);
        vars.write_register(variable, Cplx::from_real(am_mid));
        let fl = evaluate_node(tree, body_index, vars, angle_mode, MathMode::Advanced)?;
        let fl_c = fl.to_complex()?;
        if !fl_c.is_real() {
            return None;
        }
        total_evals += 1;

        let mb_mid = m + ((task.b - m) >> 1);
        vars.write_register(variable, Cplx::from_real(mb_mid));
        let fr = evaluate_node(tree, body_index, vars, angle_mode, MathMode::Advanced)?;
        let fr_c = fr.to_complex()?;
        if !fr_c.is_real() {
            return None;
        }
        total_evals += 1;

        let s_am = simpson_step(m - task.a, task.fa_re, fl_c.re, fm_c.re)?;
        let s_mb = simpson_step(task.b - m, fm_c.re, fr_c.re, task.fb_re)?;

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
                    fa_re: fm_c.re,
                    fb_re: task.fb_re,
                    tol: child_tol,
                    depth: task.depth + 1,
                };
                stack[idx + 1] = AdSimpTask {
                    a: task.a,
                    b: m,
                    fa_re: task.fa_re,
                    fb_re: fm_c.re,
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
        Some(Cplx::from_real(nearest))
    } else {
        Some(Cplx::from_real(final_result))
    }
}

fn evaluate_loop_aggregate(
    operation: LoopOperation,
    variable: u8,
    start: Cplx,
    end: Cplx,
    body_index: usize,
    tree: &ParseTree,
    vars: &mut VariableStore,
    angle_mode: AngleMode,
) -> Option<Cplx> {
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
                return Some(Cplx::zero());
            }
            if end_int - start_int > 10_000 {
                return None;
            }

            let mut accumulator = Cplx::zero();
            let mut k = start_int;
            while k <= end_int {
                vars.write_register(variable, Cplx::from_real(fp::from_integer(k)));
                let term = evaluate_node(tree, body_index, vars, angle_mode, MathMode::Advanced)?;
                let term_c = term.to_complex()?;
                accumulator = accumulator.add(term_c);
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
