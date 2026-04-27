//! # Evaluator (Math Engine — Layer 6)
//!
//! Recursively walks the AST produced by the parser and computes a Q20.12
//! fixed-point result. This is intentionally the simplest stage — all
//! structural complexity was resolved during parsing.
//!
//! The evaluator is the only math stage that:
//!   - Can fail at runtime (division by zero, domain errors like sqrt(-1))
//!   - Reads variable values (via the VariableStore reference)
//!
//! It never writes variables — that is the runtime's responsibility.

use super::fixed_point as fp;
use super::parser::{AstNode, BinaryOperator, MathConstant, MathFunction, ParseTree, VariableRef};
use super::vars::VariableStore;

// ─── Public API ───────────────────────────────────────────────────────────────

/// Evaluate a `ParseTree` given a read-only view of the variable store.
///
/// Returns the Q20.12 result, or `None` on:
///   - Division by zero or modulo by zero
///   - Domain error (sqrt of negative, log of non-positive, asin/acos out of range)
///   - Integer overflow in a checked operation
///   - Reference to `Ans` before any expression has been evaluated
pub fn evaluate_tree(tree: &ParseTree, variables: &VariableStore) -> Option<i32> {
    evaluate_node(tree, tree.root_index, variables)
}

// ─── Tree walker ──────────────────────────────────────────────────────────────

/// Recursively evaluate the node at `node_index`.
fn evaluate_node(tree: &ParseTree, node_index: usize, vars: &VariableStore) -> Option<i32> {
    match tree.nodes[node_index] {

        // ── Base cases (leaves) ───────────────────────────────────────────────

        AstNode::Literal(value) => Some(value),

        AstNode::Constant(constant) => Some(match constant {
            MathConstant::Pi => fp::FIXED_PI,
            MathConstant::E  => fp::FIXED_E,
        }),

        AstNode::Variable(var_ref) => match var_ref {
            // Ans is None until the first successful evaluation.
            VariableRef::Ans           => vars.read_ans(),
            VariableRef::Register(ch)  => vars.read_register(ch),
        },

        // ── Unary negation ────────────────────────────────────────────────────

        AstNode::UnaryNegation { operand_index } => {
            let value = evaluate_node(tree, operand_index, vars)?;
            Some(-value)
        }

        // ── Binary operations ─────────────────────────────────────────────────

        AstNode::BinaryOperation { operator, left_child_index, right_child_index } => {
            let left  = evaluate_node(tree, left_child_index,  vars)?;
            let right = evaluate_node(tree, right_child_index, vars)?;
            apply_binary_operator(operator, left, right)
        }

        // ── Function calls ────────────────────────────────────────────────────

        AstNode::FunctionCall { function, argument_index } => {
            let arg = evaluate_node(tree, argument_index, vars)?;
            apply_function(function, arg)
        }
    }
}

// ─── Binary operator dispatch ─────────────────────────────────────────────────

/// Apply a binary operator to two Q20.12 operands.
fn apply_binary_operator(operator: BinaryOperator, left: i32, right: i32) -> Option<i32> {
    match operator {
        BinaryOperator::Add      => left.checked_add(right),
        BinaryOperator::Subtract => left.checked_sub(right),
        BinaryOperator::Multiply => Some(fp::multiply(left, right)),
        BinaryOperator::Divide   => fp::divide(left, right),
        BinaryOperator::Modulo   => {
            // Modulo in fixed-point: (a % b) where a, b are Q20.12.
            // We perform integer modulo on the raw values which gives Q20.12 result.
            if right == 0 { return None; }
            Some(left % right)
        }
        BinaryOperator::Power    => fp::power(left, right),
    }
}

// ─── Function dispatch ────────────────────────────────────────────────────────

/// Apply a single-argument mathematical function to a Q20.12 argument.
fn apply_function(function: MathFunction, arg: i32) -> Option<i32> {
    match function {
        // Trigonometry — argument in radians.
        MathFunction::Sin   => Some(fp::sin(arg)),
        MathFunction::Cos   => Some(fp::cos(arg)),
        MathFunction::Tan   => fp::tan(arg),

        // Inverse trig — result in radians.
        MathFunction::Asin  => fp::asin(arg),
        MathFunction::Acos  => fp::acos(arg),
        MathFunction::Atan  => Some(fp::atan(arg)),

        // Roots and absolute value.
        MathFunction::Sqrt  => fp::sqrt(arg),
        MathFunction::Abs   => Some(fp::abs(arg)),

        // Logarithms and exponentials.
        MathFunction::Log   => fp::log10(arg),
        MathFunction::Ln    => fp::natural_log(arg),
        MathFunction::Log2  => fp::log2(arg),
        MathFunction::Exp   => Some(fp::natural_exp(arg)),

        // Rounding.
        MathFunction::Floor => Some(fp::floor(arg)),
        MathFunction::Ceil  => Some(fp::ceil(arg)),
        MathFunction::Round => Some(fp::round(arg)),

        // Angle unit conversion.
        MathFunction::Deg   => Some(fp::radians_to_degrees(arg)),
        MathFunction::Rad   => Some(fp::degrees_to_radians(arg)),
    }
}
