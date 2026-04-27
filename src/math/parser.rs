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
    /// A numeric literal in Q20.12.
    Literal(i32),

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
    UnaryNegation {
        operand_index: usize,
    },

    /// A mathematical function applied to one argument.
    FunctionCall {
        function: MathFunction,
        argument_index: usize,
    },
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
    Sin, Cos, Tan,
    Asin, Acos, Atan,
    Sqrt,
    Abs,
    Log,   // log10
    Ln,    // natural log
    Log2,
    Exp,   // e^x
    Floor, Ceil, Round,
    Deg,   // radians → degrees
    Rad,   // degrees → radians
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
        if self.node_count >= MAX_NODE_COUNT { return None; }
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
        Self { tokens: &lex.tokens[..lex.token_count], position: 0 }
    }

    fn peek(&self) -> Option<Token> {
        self.tokens.get(self.position).copied()
    }

    fn advance(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.position).copied();
        if t.is_some() { self.position += 1; }
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
    if !cursor.is_finished() { return None; }
    Some(tree)
}

// ─── Grammar rule implementations ────────────────────────────────────────────

/// expression = term ( ( '+' | '−' ) term )*
fn parse_expression(cursor: &mut ParserCursor, tree: &mut ParseTree) -> Option<usize> {
    let mut left = parse_term(cursor, tree)?;

    while let Some(token) = cursor.peek() {
        let op = match token {
            Token::Plus  => BinaryOperator::Add,
            Token::Minus => BinaryOperator::Subtract,
            _            => break,
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

/// term = power ( ( '*' | '/' | '%' ) power )*
fn parse_term(cursor: &mut ParserCursor, tree: &mut ParseTree) -> Option<usize> {
    let mut left = parse_power(cursor, tree)?;

    while let Some(token) = cursor.peek() {
        let op = match token {
            Token::Star    => BinaryOperator::Multiply,
            Token::Slash   => BinaryOperator::Divide,
            Token::Percent => BinaryOperator::Modulo,
            _              => break,
        };
        cursor.advance();
        let right = parse_power(cursor, tree)?;
        left = tree.allocate_node(AstNode::BinaryOperation {
            operator: op,
            left_child_index: left,
            right_child_index: right,
        })?;
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
        tree.allocate_node(AstNode::UnaryNegation { operand_index: operand })
    } else {
        parse_primary(cursor, tree)
    }
}

/// primary = NUMBER | CONSTANT | VARIABLE | FUNC '(' expr ')' | '(' expr ')'
fn parse_primary(cursor: &mut ParserCursor, tree: &mut ParseTree) -> Option<usize> {
    match cursor.advance()? {
        // Numeric literal.
        Token::Number(value) => tree.allocate_node(AstNode::Literal(value)),

        // Named constants.
        Token::ConstPi => tree.allocate_node(AstNode::Constant(MathConstant::Pi)),
        Token::ConstE  => tree.allocate_node(AstNode::Constant(MathConstant::E)),

        // Variables.
        Token::VarAns          => tree.allocate_node(AstNode::Variable(VariableRef::Ans)),
        Token::VarRegister(ch) => tree.allocate_node(AstNode::Variable(VariableRef::Register(ch))),

        // Function calls: FUNC '(' expression ')'
        func_token if is_function_token(func_token) => {
            // Expect opening paren.
            if cursor.advance() != Some(Token::LeftParen) { return None; }
            let argument_index = parse_expression(cursor, tree)?;
            // Expect closing paren.
            if cursor.advance() != Some(Token::RightParen) { return None; }
            let function = token_to_function(func_token)?;
            tree.allocate_node(AstNode::FunctionCall { function, argument_index })
        }

        // Parenthesised sub-expression.
        Token::LeftParen => {
            let inner = parse_expression(cursor, tree)?;
            if cursor.advance() != Some(Token::RightParen) { return None; }
            Some(inner)
        }

        // Anything else is a syntax error.
        _ => None,
    }
}

// ─── Token classification helpers ────────────────────────────────────────────

/// Return true if the token represents a named mathematical function.
fn is_function_token(token: Token) -> bool {
    matches!(token,
        Token::FuncSin | Token::FuncCos | Token::FuncTan |
        Token::FuncAsin | Token::FuncAcos | Token::FuncAtan |
        Token::FuncSqrt | Token::FuncAbs | Token::FuncLog |
        Token::FuncLn | Token::FuncLog2 | Token::FuncExp |
        Token::FuncFloor | Token::FuncCeil | Token::FuncRound |
        Token::FuncDeg | Token::FuncRad
    )
}

/// Map a function token to its `MathFunction` enum variant.
fn token_to_function(token: Token) -> Option<MathFunction> {
    Some(match token {
        Token::FuncSin   => MathFunction::Sin,
        Token::FuncCos   => MathFunction::Cos,
        Token::FuncTan   => MathFunction::Tan,
        Token::FuncAsin  => MathFunction::Asin,
        Token::FuncAcos  => MathFunction::Acos,
        Token::FuncAtan  => MathFunction::Atan,
        Token::FuncSqrt  => MathFunction::Sqrt,
        Token::FuncAbs   => MathFunction::Abs,
        Token::FuncLog   => MathFunction::Log,
        Token::FuncLn    => MathFunction::Ln,
        Token::FuncLog2  => MathFunction::Log2,
        Token::FuncExp   => MathFunction::Exp,
        Token::FuncFloor => MathFunction::Floor,
        Token::FuncCeil  => MathFunction::Ceil,
        Token::FuncRound => MathFunction::Round,
        Token::FuncDeg   => MathFunction::Deg,
        Token::FuncRad   => MathFunction::Rad,
        _ => return None,
    })
}