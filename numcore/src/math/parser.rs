//! # Parser (Math Engine — Layer 6)
//!
//! Converts the flat token stream from the lexer into an Abstract Syntax Tree
//! that encodes operator precedence, associativity, and function application.
//!
//! ## Grammar (recursive descent)
//!
//!   expression  =  term   ( ( '+' | '−' )   term   )*
//!   term        =  power  ( ( '*' | '/' | '%' ) power )*
//!   power       =  unary  ( '^' power )*          ← right-associative
//!   unary       =  '−' unary  |  postfix
//!   postfix     =  primary                         ← (future: factorial)
//!   primary     =  NUMBER | CONSTANT | VARIABLE
//!               |  FUNCTION '(' expression ')'
//!               |  '(' expression ')'
//!
//! Precedence (low → high):
//!   + −   (additive)
//!   * / % (multiplicative)
//!   ^     (exponentiation, right-associative)
//!   unary −
//!   function calls, grouping

use super::lexer::{LexResult, Token};

/// Maximum AST nodes for one expression.
pub const MAX_NODE_COUNT: usize = 64;
// 64 nodes covers deeply nested expressions within our 32-token budget.
// Keeps ParseTree at ~1 KB — safe for static allocation in CalcState.

// ─── AST node types ───────────────────────────────────────────────────────────

/// A single node in the Abstract Syntax Tree.
#[derive(Clone, Copy)]
pub enum AstNode {
    /// A numeric literal in Q31.32.
    Literal(i64),

    /// A named constant (π, e).
    Constant(MathConstant),

    /// A variable reference (Ans, A–F).
    Variable(VariableRef),

    /// A binary arithmetic operation.
    BinaryOperation {
        operator: BinaryOperator,
        left_child_index: usize,
        right_child_index: usize,
    },

    /// Unary negation of a sub-expression.
    UnaryNegation { operand_index: usize },

    /// A mathematical function applied to one argument.
    FunctionCall {
        function: MathFunction,
        argument_index: usize,
    },

    /// A numeric function applied to exactly three arguments.
    /// Used by: binomP(n, k, p)
    ThreeArgFunction {
        function: ThreeArgMathFunction,
        arg_indices: [usize; 3],
    },

    /// A numeric function applied to exactly two arguments.
    /// Used by: poissonP(λ, k), chiCDF(x, k)
    TwoArgFunction {
        function: TwoArgMathFunction,
        arg_indices: [usize; 2],
    },

    /// Store a value into a user register.
    /// Used by sto(value, var). Returns the stored value.
    Store {
        /// Index of the value expression node.
        value_index: usize,
        /// The register letter to store into (e.g. b'A' for register A).
        register: u8,
    },

    /// A looping aggregate over an expression body with a bound variable.
    /// Used by sum(expr, var, start, end) and int(expr, var, a, b).
    ///
    /// The `variable` field identifies which register (A–F) serves as the
    /// loop variable. Its value in the VariableStore is temporarily shadowed
    /// during evaluation — the original value is restored afterward.
    LoopAggregate {
        operation: LoopOperation,
        /// The register letter used as the loop variable (e.g. b'K' for K).
        variable: u8,
        /// Index of the start-bound expression node.
        start_index: usize,
        /// Index of the end-bound expression node.
        end_index: usize,
        /// Index of the expression body node (evaluated once per step).
        body_index: usize,
    },
}

/// Functions that take exactly three numeric arguments.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ThreeArgMathFunction {
    /// P(X=k) for X ~ Binomial(n, p). Arguments: n, k, p.
    BinomialProbability,
}

/// Functions that take exactly two numeric arguments.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TwoArgMathFunction {
    /// P(X=k) for X ~ Poisson(λ). Arguments: λ, k.
    PoissonProbability,
    /// P(X≤x) for X ~ χ²(k). Arguments: x, k.
    ChiSquaredCDF,
    /// n-th root of a number. Arguments: x, n.
    NthRoot,
}

/// The two looping aggregate operations.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LoopOperation {
    /// Σ: sum the body expression over the integer range [start, end].
    Summation,
    /// ∫: integrate the body expression from start to end via Simpson's rule.
    Integration,
}

/// Named mathematical constants.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MathConstant {
    Pi,
    E,
}

/// Variable references.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VariableRef {
    /// The `Ans` variable — last computed answer.
    Ans,
    /// A user register, identified by its letter byte ('A'–'F').
    Register(u8),
}

/// Binary arithmetic operators.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Power,
}

/// Single-argument mathematical functions.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MathFunction {
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    SinH,
    CosH,
    TanH,
    ASinH,
    ACosH,
    ATanH,
    Sqrt,
    Abs,
    Log, // log10
    Ln,  // natural log
    Log2,
    Exp, // e^x
    Floor,
    Ceil,
    Round,
    Deg,     // radians → degrees
    Rad,     // degrees → radians
    LnGamma, // ln(Γ(x))
}

// ─── Parse tree ───────────────────────────────────────────────────────────────

/// A complete AST stored as a flat arena of nodes.
pub struct ParseTree {
    pub nodes: [AstNode; MAX_NODE_COUNT],
    pub node_count: usize,
    pub root_index: usize,
}

impl ParseTree {
    /// Allocate a new node in the arena and return its index.
    pub fn allocate_node(&mut self, node: AstNode) -> Option<usize> {
        if self.node_count >= MAX_NODE_COUNT {
            return None;
        }
        let index = self.node_count;
        self.nodes[index] = node;
        self.node_count += 1;
        Some(index)
    }
}

// ─── Parser cursor ────────────────────────────────────────────────────────────

struct ParserCursor<'a> {
    tokens: &'a [Token],
    position: usize,
}

impl<'a> ParserCursor<'a> {
    fn new(lex: &'a LexResult) -> Self {
        Self {
            tokens: &lex.tokens[..lex.token_count],
            position: 0,
        }
    }

    fn peek(&self) -> Option<Token> {
        self.tokens.get(self.position).copied()
    }

    fn advance(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.position).copied();
        if t.is_some() {
            self.position += 1;
        }
        t
    }

    fn is_finished(&self) -> bool {
        self.position >= self.tokens.len()
    }
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Parse a `LexResult` into `tree` (a caller-provided scratch `ParseTree`).
///
/// The caller provides the `ParseTree` (typically from `CalcState`) so no
/// stack allocation occurs. The tree is reset before use.
///
/// Returns `None` if the token stream is not a syntactically valid expression.
pub fn parse_token_stream<'a>(lex: &LexResult, tree: &'a mut ParseTree) -> Option<&'a ParseTree> {
    // Reset the scratch buffer before reuse.
    tree.node_count = 0;
    tree.root_index = 0;

    let mut cursor = ParserCursor::new(lex);

    let root = parse_expression(&mut cursor, tree)?;
    tree.root_index = root;

    // Any unconsumed tokens mean the input was malformed.
    if !cursor.is_finished() {
        return None;
    }
    Some(tree)
}

// ─── Grammar rule implementations ────────────────────────────────────────────

/// expression = term ( ( '+' | '−' ) term )*
fn parse_expression(cursor: &mut ParserCursor, tree: &mut ParseTree) -> Option<usize> {
    let mut left = parse_term(cursor, tree)?;

    while let Some(token) = cursor.peek() {
        let op = match token {
            Token::Plus => BinaryOperator::Add,
            Token::Minus => BinaryOperator::Subtract,
            _ => break,
        };
        cursor.advance();
        let right = parse_term(cursor, tree)?;
        left = tree.allocate_node(AstNode::BinaryOperation {
            operator: op,
            left_child_index: left,
            right_child_index: right,
        })?;
    }
    Some(left)
}

/// term = power ( ( '*' | '/' | '%' | implicit_mult ) power )*
///
/// Implicit multiplication fires when a primary expression is immediately
/// followed by the start of another primary with no explicit operator, e.g.
/// `3(5)`, `(a)b`, `(x)(y)`, `a b`, `a 3`. This makes the multiplication
/// operator optional before parentheses, variables, constants, and numbers.
fn parse_term(cursor: &mut ParserCursor, tree: &mut ParseTree) -> Option<usize> {
    let mut left = parse_power(cursor, tree)?;

    loop {
        // ── Explicit multiplication / division / modulo ───────────────
        let op = match cursor.peek() {
            Some(Token::Star) => {
                cursor.advance();
                Some(BinaryOperator::Multiply)
            }
            Some(Token::Slash) => {
                cursor.advance();
                Some(BinaryOperator::Divide)
            }
            Some(Token::Percent) => {
                cursor.advance();
                Some(BinaryOperator::Modulo)
            }
            _ => None,
        };
        if let Some(op) = op {
            let right = parse_power(cursor, tree)?;
            left = tree.allocate_node(AstNode::BinaryOperation {
                operator: op,
                left_child_index: left,
                right_child_index: right,
            })?;
            continue;
        }

        // ── Implicit multiplication ───────────────────────────────────
        // If the next token starts a primary expression (number, variable,
        // constant, function call, or parenthesised group), treat it as an
        // implicit multiply: a(b) → a * b, (a)b → a * b, 3(5) → 3 * 5.
        if let Some(token) = cursor.peek() {
            if is_primary_start(token) {
                let right = parse_power(cursor, tree)?;
                left = tree.allocate_node(AstNode::BinaryOperation {
                    operator: BinaryOperator::Multiply,
                    left_child_index: left,
                    right_child_index: right,
                })?;
                continue;
            }
        }

        break;
    }
    Some(left)
}

/// power = unary ( '^' power )*     ← right-associative via recursion
fn parse_power(cursor: &mut ParserCursor, tree: &mut ParseTree) -> Option<usize> {
    let base = parse_unary(cursor, tree)?;

    if cursor.peek() == Some(Token::Caret) {
        cursor.advance();
        // Recurse into parse_power for right-associativity:
        // 2^3^4 = 2^(3^4), not (2^3)^4
        let exponent = parse_power(cursor, tree)?;
        tree.allocate_node(AstNode::BinaryOperation {
            operator: BinaryOperator::Power,
            left_child_index: base,
            right_child_index: exponent,
        })
    } else {
        Some(base)
    }
}

/// unary = '−' unary | primary
fn parse_unary(cursor: &mut ParserCursor, tree: &mut ParseTree) -> Option<usize> {
    if cursor.peek() == Some(Token::UnaryMinus) {
        cursor.advance();
        let operand = parse_unary(cursor, tree)?;
        tree.allocate_node(AstNode::UnaryNegation {
            operand_index: operand,
        })
    } else {
        parse_primary(cursor, tree)
    }
}

/// primary = NUMBER | CONSTANT | VARIABLE
///          | SINGLE_ARG_FUNC '(' expr ')'
///          | THREE_ARG_FUNC '(' expr ',' expr ',' expr ')'
///          | LOOP_AGGREGATE '(' expr ',' VAR ',' expr ',' expr ')'
///          | '(' expr ')'
fn parse_primary(cursor: &mut ParserCursor, tree: &mut ParseTree) -> Option<usize> {
    match cursor.advance()? {
        // Numeric literal.
        Token::Number(value) => tree.allocate_node(AstNode::Literal(value)),

        // Named constants.
        Token::ConstPi => tree.allocate_node(AstNode::Constant(MathConstant::Pi)),
        Token::ConstE => tree.allocate_node(AstNode::Constant(MathConstant::E)),

        // Variables.
        Token::VarAns => tree.allocate_node(AstNode::Variable(VariableRef::Ans)),
        Token::VarRegister(ch) => tree.allocate_node(AstNode::Variable(VariableRef::Register(ch))),

        // Single-argument function calls: FUNC '(' expression ')'
        func_token if is_single_arg_function_token(func_token) => {
            if cursor.advance() != Some(Token::LeftParen) {
                return None;
            }
            let argument_index = parse_expression(cursor, tree)?;
            if cursor.advance() != Some(Token::RightParen) {
                return None;
            }
            let function = token_to_single_arg_function(func_token)?;
            tree.allocate_node(AstNode::FunctionCall {
                function,
                argument_index,
            })
        }

        // Three-argument functions: FUNC '(' expr ',' expr ',' expr ')'
        func_token if is_three_arg_function_token(func_token) => {
            if cursor.advance() != Some(Token::LeftParen) {
                return None;
            }
            let arg0 = parse_expression(cursor, tree)?;
            if cursor.advance() != Some(Token::Comma) {
                return None;
            }
            let arg1 = parse_expression(cursor, tree)?;
            if cursor.advance() != Some(Token::Comma) {
                return None;
            }
            let arg2 = parse_expression(cursor, tree)?;
            if cursor.advance() != Some(Token::RightParen) {
                return None;
            }
            let function = token_to_three_arg_function(func_token)?;
            tree.allocate_node(AstNode::ThreeArgFunction {
                function,
                arg_indices: [arg0, arg1, arg2],
            })
        }

        // Two-argument functions: FUNC '(' expr ',' expr ')'
        func_token if is_two_arg_function_token(func_token) => {
            if cursor.advance() != Some(Token::LeftParen) {
                return None;
            }
            let arg0 = parse_expression(cursor, tree)?;
            if cursor.advance() != Some(Token::Comma) {
                return None;
            }
            let arg1 = parse_expression(cursor, tree)?;
            if cursor.advance() != Some(Token::RightParen) {
                return None;
            }
            let function = token_to_two_arg_function(func_token)?;
            tree.allocate_node(AstNode::TwoArgFunction {
                function,
                arg_indices: [arg0, arg1],
            })
        }

        // sto(value, var): store expression value into a register.
        Token::FuncSto => {
            if cursor.advance() != Some(Token::LeftParen) {
                return None;
            }
            let value_index = parse_expression(cursor, tree)?;
            if cursor.advance() != Some(Token::Comma) {
                return None;
            }
            let register = match cursor.advance()? {
                Token::VarRegister(ch) => ch,
                _ => return None,
            };
            if cursor.advance() != Some(Token::RightParen) {
                return None;
            }
            tree.allocate_node(AstNode::Store {
                value_index,
                register,
            })
        }

        // Loop aggregates: sum/int '(' body ',' var ',' start ',' end ')'
        func_token if is_loop_aggregate_token(func_token) => {
            if cursor.advance() != Some(Token::LeftParen) {
                return None;
            }
            let body_index = parse_expression(cursor, tree)?;
            if cursor.advance() != Some(Token::Comma) {
                return None;
            }
            // Variable must be a single register letter (A–F)
            let variable = match cursor.advance()? {
                Token::VarRegister(ch) => ch,
                _ => return None,
            };
            if cursor.advance() != Some(Token::Comma) {
                return None;
            }
            let start_index = parse_expression(cursor, tree)?;
            if cursor.advance() != Some(Token::Comma) {
                return None;
            }
            let end_index = parse_expression(cursor, tree)?;
            if cursor.advance() != Some(Token::RightParen) {
                return None;
            }
            let operation = if func_token == Token::FuncSum {
                LoopOperation::Summation
            } else {
                LoopOperation::Integration
            };
            tree.allocate_node(AstNode::LoopAggregate {
                operation,
                variable,
                start_index,
                end_index,
                body_index,
            })
        }

        // Parenthesised sub-expression.
        Token::LeftParen => {
            let inner = parse_expression(cursor, tree)?;
            if cursor.advance() != Some(Token::RightParen) {
                return None;
            }
            Some(inner)
        }

        // Anything else is a syntax error.
        _ => None,
    }
}

// ─── Token classification helpers ────────────────────────────────────────────

/// Return true if the token represents a single-argument mathematical function.
fn is_single_arg_function_token(token: Token) -> bool {
    matches!(
        token,
        Token::FuncSin
            | Token::FuncCos
            | Token::FuncTan
            | Token::FuncAsin
            | Token::FuncAcos
            | Token::FuncAtan
            | Token::FuncSinH
            | Token::FuncCosH
            | Token::FuncTanH
            | Token::FuncASinH
            | Token::FuncACosH
            | Token::FuncATanH
            | Token::FuncSqrt
            | Token::FuncAbs
            | Token::FuncLog
            | Token::FuncLn
            | Token::FuncLog2
            | Token::FuncExp
            | Token::FuncFloor
            | Token::FuncCeil
            | Token::FuncRound
            | Token::FuncDeg
            | Token::FuncRad
            | Token::FuncLnGamma
    )
}

/// Return true if the token is a three-argument numeric function.
fn is_three_arg_function_token(token: Token) -> bool {
    matches!(token, Token::FuncBinomP)
}

/// Return true if the token is a two-argument numeric function.
fn is_two_arg_function_token(token: Token) -> bool {
    matches!(
        token,
        Token::FuncPoissonP | Token::FuncChiCDF | Token::FuncNthRoot
    )
}

/// Return true if the token is a loop aggregate (sum or int).
fn is_loop_aggregate_token(token: Token) -> bool {
    matches!(token, Token::FuncSum | Token::FuncInt)
}

/// Return true if the token can start a primary expression.
///
/// Used by `parse_term` for implicit multiplication detection. A primary
/// starts with a literal, variable, constant, parenthesised group, or any
/// named function (all of which demand `(` next in `parse_primary`).
fn is_primary_start(token: Token) -> bool {
    matches!(
        token,
        Token::Number(_)
            | Token::VarAns
            | Token::VarRegister(_)
            | Token::ConstPi
            | Token::ConstE
            | Token::LeftParen
    ) || is_single_arg_function_token(token)
        || is_three_arg_function_token(token)
        || is_two_arg_function_token(token)
        || is_loop_aggregate_token(token)
        || token == Token::FuncSto
}

/// Map a single-argument function token to its MathFunction variant.
fn token_to_single_arg_function(token: Token) -> Option<MathFunction> {
    Some(match token {
        Token::FuncSin => MathFunction::Sin,
        Token::FuncCos => MathFunction::Cos,
        Token::FuncTan => MathFunction::Tan,
        Token::FuncAsin => MathFunction::Asin,
        Token::FuncAcos => MathFunction::Acos,
        Token::FuncAtan => MathFunction::Atan,
        Token::FuncSinH => MathFunction::SinH,
        Token::FuncCosH => MathFunction::CosH,
        Token::FuncTanH => MathFunction::TanH,
        Token::FuncASinH => MathFunction::ASinH,
        Token::FuncACosH => MathFunction::ACosH,
        Token::FuncATanH => MathFunction::ATanH,
        Token::FuncSqrt => MathFunction::Sqrt,
        Token::FuncAbs => MathFunction::Abs,
        Token::FuncLog => MathFunction::Log,
        Token::FuncLn => MathFunction::Ln,
        Token::FuncLog2 => MathFunction::Log2,
        Token::FuncExp => MathFunction::Exp,
        Token::FuncFloor => MathFunction::Floor,
        Token::FuncCeil => MathFunction::Ceil,
        Token::FuncRound => MathFunction::Round,
        Token::FuncDeg => MathFunction::Deg,
        Token::FuncRad => MathFunction::Rad,
        Token::FuncLnGamma => MathFunction::LnGamma,
        _ => return None,
    })
}

/// Map a three-argument function token to its ThreeArgMathFunction variant.
fn token_to_three_arg_function(token: Token) -> Option<ThreeArgMathFunction> {
    Some(match token {
        Token::FuncBinomP => ThreeArgMathFunction::BinomialProbability,
        _ => return None,
    })
}

/// Map a two-argument function token to its TwoArgMathFunction variant.
fn token_to_two_arg_function(token: Token) -> Option<TwoArgMathFunction> {
    Some(match token {
        Token::FuncPoissonP => TwoArgMathFunction::PoissonProbability,
        Token::FuncChiCDF => TwoArgMathFunction::ChiSquaredCDF,
        Token::FuncNthRoot => TwoArgMathFunction::NthRoot,
        _ => return None,
    })
}
