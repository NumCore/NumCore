//! # Evaluator (Math Engine — Layer 6)
//!
//! Recursively walks the AST produced by the parser and computes a Q31.32
//! fixed-point result. This is intentionally the simplest stage — all
//! structural complexity was resolved during parsing.
//!
//! The evaluator is the only math stage that:
//!   - Can fail at runtime (division by zero, domain errors like sqrt(-1))
//!   - Reads and writes variable values (via the VariableStore reference)
//!
//! The `sto()` function writes to a register. Loop aggregates write to a
//! local copy of the store that is discarded after the loop completes, so
//! loop variables are scoped to their aggregate expression.

use super::distributions;
use super::fixed_point as fp;
use super::parser::{
    AstNode, BinaryOperator, LoopOperation, MathConstant, MathFunction, ParseTree,
    ThreeArgMathFunction, TwoArgMathFunction, VariableRef,
};
use super::vars::VariableStore;

// ─── Public API ───────────────────────────────────────────────────────────────

/// Evaluate a `ParseTree` given a mutable view of the variable store.
///
/// The store is mutable so that `sto()` can write to registers during
/// evaluation. Loop aggregates operate on a local copy, so loop-variable
/// writes never escape the aggregate expression.
///
/// Returns the Q31.32 result, or `None` on:
///   - Division by zero or modulo by zero
///   - Domain error (sqrt of negative, log of non-positive, asin/acos out of range)
///   - Integer overflow in a checked operation
///   - Reference to `Ans` before any expression has been evaluated
pub fn evaluate_tree(tree: &ParseTree, variables: &mut VariableStore) -> Option<i64> {
    evaluate_node(tree, tree.root_index, variables)
}

// ─── Tree walker ──────────────────────────────────────────────────────────────

/// Recursively evaluate the node at `node_index`.
fn evaluate_node(tree: &ParseTree, node_index: usize, vars: &mut VariableStore) -> Option<i64> {
    match tree.nodes[node_index] {
        // ── Base cases (leaves) ───────────────────────────────────────────────
        AstNode::Literal(value) => Some(value),

        AstNode::Constant(constant) => Some(match constant {
            MathConstant::Pi => fp::FIXED_PI,
            MathConstant::E => fp::FIXED_E,
        }),

        AstNode::Variable(var_ref) => match var_ref {
            // Ans is None until the first successful evaluation.
            VariableRef::Ans => vars.read_ans(),
            VariableRef::Register(ch) => vars.read_register(ch),
        },

        // ── Unary negation ────────────────────────────────────────────────────
        AstNode::UnaryNegation { operand_index } => {
            let value = evaluate_node(tree, operand_index, vars)?;
            Some(-value)
        }

        // ── Binary operations ─────────────────────────────────────────────────
        AstNode::BinaryOperation {
            operator,
            left_child_index,
            right_child_index,
        } => {
            let left = evaluate_node(tree, left_child_index, vars)?;
            let right = evaluate_node(tree, right_child_index, vars)?;
            apply_binary_operator(operator, left, right)
        }

        // ── Single-argument function calls ────────────────────────────────────
        AstNode::FunctionCall {
            function,
            argument_index,
        } => {
            let arg = evaluate_node(tree, argument_index, vars)?;
            apply_function(function, arg)
        }

        // ── Three-argument numeric functions ──────────────────────────────────
        AstNode::ThreeArgFunction {
            function,
            arg_indices,
        } => {
            let a0 = evaluate_node(tree, arg_indices[0], vars)?;
            let a1 = evaluate_node(tree, arg_indices[1], vars)?;
            let a2 = evaluate_node(tree, arg_indices[2], vars)?;
            apply_three_arg_function(function, a0, a1, a2)
        }

        // ── Two-argument numeric functions ────────────────────────────────────
        AstNode::TwoArgFunction {
            function,
            arg_indices,
        } => {
            let a0 = evaluate_node(tree, arg_indices[0], vars)?;
            let a1 = evaluate_node(tree, arg_indices[1], vars)?;
            apply_two_arg_function(function, a0, a1)
        }

        // ── Store value into register ─────────────────────────────────────────
        AstNode::Store {
            value_index,
            register,
        } => {
            let value = evaluate_node(tree, value_index, vars)?;
            vars.write_register(register, value);
            Some(value)
        }

        // ── Loop aggregates (summation and integration) ───────────────────────
        AstNode::LoopAggregate {
            operation,
            variable,
            start_index,
            end_index,
            body_index,
        } => {
            let start = evaluate_node(tree, start_index, vars)?;
            let end = evaluate_node(tree, end_index, vars)?;
            evaluate_loop_aggregate(operation, variable, start, end, body_index, tree, vars)
        }
    }
}

// ─── Binary operator dispatch ─────────────────────────────────────────────────

/// Apply a binary operator to two Q31.32 operands.
fn apply_binary_operator(operator: BinaryOperator, left: i64, right: i64) -> Option<i64> {
    match operator {
        BinaryOperator::Add => left.checked_add(right),
        BinaryOperator::Subtract => left.checked_sub(right),
        BinaryOperator::Multiply => Some(fp::multiply(left, right)),
        BinaryOperator::Divide => fp::divide(left, right),
        BinaryOperator::Modulo => {
            // Modulo in fixed-point: (a % b) where a, b are Q31.32.
            // We perform integer modulo on the raw values which gives Q31.32 result.
            if right == 0 {
                return None;
            }
            Some(left % right)
        }
        BinaryOperator::Power => fp::power(left, right),
    }
}

// ─── Function dispatch ────────────────────────────────────────────────────────

/// Apply a single-argument mathematical function to a Q31.32 argument.
fn apply_function(function: MathFunction, arg: i64) -> Option<i64> {
    match function {
        // Trigonometry — argument in radians.
        MathFunction::Sin => Some(fp::sin(arg)),
        MathFunction::Cos => Some(fp::cos(arg)),
        MathFunction::Tan => fp::tan(arg),

        // Inverse trig — result in radians.
        MathFunction::Asin => fp::asin(arg),
        MathFunction::Acos => fp::acos(arg),
        MathFunction::Atan => Some(fp::atan(arg)),

        // Hyperbolic trig functions.
        MathFunction::SinH => Some(fp::sinh(arg)),
        MathFunction::CosH => Some(fp::cosh(arg)),
        MathFunction::TanH => fp::tanh(arg),

        // Inverse hyperbolic trig functions.
        MathFunction::ASinH => fp::asinh(arg),
        MathFunction::ACosH => fp::acosh(arg),
        MathFunction::ATanH => fp::atanh(arg),

        // Roots and absolute value.
        MathFunction::Sqrt => fp::sqrt(arg),
        MathFunction::Abs => Some(fp::abs(arg)),

        // Logarithms and exponentials.
        MathFunction::Log => fp::log10(arg),
        MathFunction::Ln => fp::natural_log(arg),
        MathFunction::Log2 => fp::log2(arg),
        MathFunction::Exp => Some(fp::natural_exp(arg)),

        // Rounding.
        MathFunction::Floor => Some(fp::floor(arg)),
        MathFunction::Ceil => Some(fp::ceil(arg)),
        MathFunction::Round => Some(fp::round(arg)),

        // Angle unit conversion.
        MathFunction::Deg => Some(fp::degrees_to_radians(arg)),
        MathFunction::Rad => Some(fp::radians_to_degrees(arg)),

        // Special functions.
        MathFunction::LnGamma => distributions::ln_gamma(arg),
    }
}

// ─── Two-argument function dispatch ──────────────────────────────────────────

/// Dispatch a two-argument mathematical function.
fn apply_two_arg_function(function: TwoArgMathFunction, a0: i64, a1: i64) -> Option<i64> {
    match function {
        TwoArgMathFunction::PoissonProbability => distributions::poisson_probability(a0, a1),
        TwoArgMathFunction::ChiSquaredCDF => distributions::chi_squared_cdf(a0, a1),
        TwoArgMathFunction::NthRoot => fp::nthroot(a0, a1),
    }
}

// ─── Three-argument function dispatch ────────────────────────────────────────

/// Dispatch a three-argument mathematical function.
fn apply_three_arg_function(
    function: ThreeArgMathFunction,
    a0: i64,
    a1: i64,
    a2: i64,
) -> Option<i64> {
    match function {
        ThreeArgMathFunction::BinomialProbability => {
            distributions::binomial_probability(a0, a1, a2)
        }
    }
}

// ─── Loop aggregate evaluation ────────────────────────────────────────────────

/// Number of Simpson's rule intervals. Must be even.
/// 100 gives polynomial-exact results and runs in reasonable time on Cortex-M3.
const SIMPSONS_INTERVALS: i64 = 100;

/// Snap-to-integer threshold for integration results.
/// If the result is within 4295 Q31.32 units (≈ 1×10⁻⁶) of an integer,
/// snap to that integer. This makes integrals of polynomials return exact
/// whole numbers (e.g. ∫sin from 0 to π = 2 exactly).
const INTEGRATION_SNAP_THRESHOLD: i64 = 4295;

/// Evaluate a summation or integration over a bound loop variable.
///
/// The loop variable (a register A–F) is temporarily written with each
/// step value during evaluation, then restored to its original value
/// afterward. This is the standard calculator convention — the loop
/// variable is scoped to the aggregate expression.
fn evaluate_loop_aggregate(
    operation: LoopOperation,
    variable: u8,
    start: i64,
    end: i64,
    body_index: usize,
    tree: &ParseTree,
    vars: &mut VariableStore,
) -> Option<i64> {
    // The loop variable must be scoped to this aggregate. Clone the store
    // and use the local copy so that sto() inside the body writes to the
    // loop-local copy, not the caller's store.
    let mut local_vars = *vars;

    match operation {
        // ── Summation: Σ body for variable = start, start+1, ..., end ──────────
        LoopOperation::Summation => {
            // start and end must be integers.
            if start & (fp::SCALE - 1) != 0 {
                return None;
            }
            if end & (fp::SCALE - 1) != 0 {
                return None;
            }

            let start_int = fp::to_integer_truncated(start);
            let end_int = fp::to_integer_truncated(end);

            if end_int < start_int {
                return Some(0);
            }
            // Guard against runaway sums.
            if end_int - start_int > 10_000 {
                return None;
            }

            let mut accumulator: i64 = 0;
            let mut k = start_int;
            while k <= end_int {
                local_vars.write_register(variable, fp::from_integer(k));
                let term = evaluate_node(tree, body_index, &mut local_vars)?;
                accumulator = accumulator.checked_add(term)?;
                k += 1;
            }
            Some(accumulator)
        }

        // ── Integration: ∫ body dx from start to end via composite Simpson ──────
        LoopOperation::Integration => {
            let n = SIMPSONS_INTERVALS;

            let range = end.checked_sub(start)?;
            let h = fp::divide(range, fp::from_integer(n))?;

            // endpoints
            local_vars.write_register(variable, start);
            let f_start = evaluate_node(tree, body_index, &mut local_vars)?;

            local_vars.write_register(variable, end);
            let f_end = evaluate_node(tree, body_index, &mut local_vars)?;

            // IMPORTANT: keep accumulator in i64 (NOT i128)
            let mut sum = f_start + f_end;

            for i in 1..n {
                // compute x EXACTLY from formula, no accumulation drift
                let i_fp = fp::from_integer(i);
                let x = start.checked_add(fp::multiply(i_fp, h))?;

                local_vars.write_register(variable, x);
                let f_x = evaluate_node(tree, body_index, &mut local_vars)?;

                let coeff = if i % 2 == 1 { 4 } else { 2 };

                sum = sum.checked_add(fp::multiply(fp::from_integer(coeff), f_x))?;
            }

            // final scaling (keep this order!)
            let result = fp::divide(fp::multiply(h, sum), fp::from_integer(3))?;

            let nearest = fp::round(result);

            if (result - nearest).abs() < INTEGRATION_SNAP_THRESHOLD {
                Some(nearest)
            } else {
                Some(result)
            }
        }
    }
}
