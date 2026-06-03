use super::lexer::{LexResult, Token};
use super::matrix::Matrix;

pub const MAX_NODE_COUNT: usize = 64;

#[derive(Clone, Copy)]
pub enum StoreTarget {
    Scalar,
    Matrix,
}

pub const MATRIX_CACHE_SIZE: usize = 2;

#[derive(Clone, Copy)]
pub enum AstNode {
    Literal(i64),
    Constant(MathConstant),
    Variable(VariableRef),
    BinaryOperation {
        operator: BinaryOperator,
        left_child_index: usize,
        right_child_index: usize,
    },
    UnaryNegation {
        operand_index: usize,
    },
    FunctionCall {
        function: MathFunction,
        argument_index: usize,
    },
    ThreeArgFunction {
        function: ThreeArgMathFunction,
        arg_indices: [usize; 3],
    },
    TwoArgFunction {
        function: TwoArgMathFunction,
        arg_indices: [usize; 2],
    },
    Store {
        value_index: usize,
        register: u8,
        target: StoreTarget,
    },
    LoopAggregate {
        operation: LoopOperation,
        variable: u8,
        start_index: usize,
        end_index: usize,
        body_index: usize,
    },
    MatrixLiteral {
        cache_index: usize,
    },
    MatrixRegister(u8),
    MatrixFunctionCall {
        function: MatrixFunction,
        argument_index: usize,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MatrixFunction {
    Det,
    Transpose,
    Identity,
    Inv,
    Cofactor,
    Adjugate,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ThreeArgMathFunction {
    BinomialProbability,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TwoArgMathFunction {
    PoissonProbability,
    ChiSquaredCDF,
    NthRoot,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LoopOperation {
    Summation,
    Integration,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MathConstant {
    Pi,
    E,
    ImaginaryUnit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VariableRef {
    Ans,
    Register(u8),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Power,
}

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
    Log,
    Ln,
    Log2,
    Exp,
    Floor,
    Ceil,
    Round,
    Deg,
    Rad,
    LnGamma,
}

pub struct ParseTree {
    pub nodes: [AstNode; MAX_NODE_COUNT],
    pub node_count: usize,
    pub root_index: usize,
    pub mat_cache: [Option<Matrix>; MATRIX_CACHE_SIZE],
    pub mat_cache_count: usize,
}

impl ParseTree {
    pub fn allocate_node(&mut self, node: AstNode) -> Option<usize> {
        if self.node_count >= MAX_NODE_COUNT {
            return None;
        }
        let index = self.node_count;
        self.nodes[index] = node;
        self.node_count += 1;
        Some(index)
    }

    pub fn allocate_matrix(&mut self, m: Matrix) -> Option<usize> {
        if self.mat_cache_count >= MATRIX_CACHE_SIZE {
            return None;
        }
        let idx = self.mat_cache_count;
        self.mat_cache[idx] = Some(m);
        self.mat_cache_count += 1;
        Some(idx)
    }
}

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

pub fn parse_token_stream<'a>(lex: &LexResult, tree: &'a mut ParseTree) -> Option<&'a ParseTree> {
    tree.node_count = 0;
    tree.root_index = 0;
    tree.mat_cache_count = 0;

    let mut cursor = ParserCursor::new(lex);

    let root = parse_expression(&mut cursor, tree)?;
    tree.root_index = root;

    if !cursor.is_finished() {
        return None;
    }
    Some(tree)
}

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

fn parse_term(cursor: &mut ParserCursor, tree: &mut ParseTree) -> Option<usize> {
    let mut left = parse_power(cursor, tree)?;

    loop {
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

fn parse_power(cursor: &mut ParserCursor, tree: &mut ParseTree) -> Option<usize> {
    let base = parse_unary(cursor, tree)?;

    if cursor.peek() == Some(Token::Caret) {
        cursor.advance();
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

fn parse_primary(cursor: &mut ParserCursor, tree: &mut ParseTree) -> Option<usize> {
    match cursor.advance()? {
        Token::Number(value) => tree.allocate_node(AstNode::Literal(value)),

        Token::ConstPi => tree.allocate_node(AstNode::Constant(MathConstant::Pi)),
        Token::ConstE => tree.allocate_node(AstNode::Constant(MathConstant::E)),
        Token::ConstI => tree.allocate_node(AstNode::Constant(MathConstant::ImaginaryUnit)),

        Token::VarAns => tree.allocate_node(AstNode::Variable(VariableRef::Ans)),
        Token::VarRegister(ch) => tree.allocate_node(AstNode::Variable(VariableRef::Register(ch))),
        Token::MatRegister(ch) => tree.allocate_node(AstNode::MatrixRegister(ch)),

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

        Token::FuncSto => {
            if cursor.advance() != Some(Token::LeftParen) {
                return None;
            }
            let value_index = parse_expression(cursor, tree)?;
            if cursor.advance() != Some(Token::Comma) {
                return None;
            }
            let (register, target) = match cursor.advance()? {
                Token::VarRegister(ch) => (ch, StoreTarget::Scalar),
                Token::MatRegister(ch) => (ch, StoreTarget::Matrix),
                _ => return None,
            };
            if cursor.advance() != Some(Token::RightParen) {
                return None;
            }
            tree.allocate_node(AstNode::Store {
                value_index,
                register,
                target,
            })
        }

        func_token if is_loop_aggregate_token(func_token) => {
            if cursor.advance() != Some(Token::LeftParen) {
                return None;
            }
            let body_index = parse_expression(cursor, tree)?;
            if cursor.advance() != Some(Token::Comma) {
                return None;
            }
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

        tok @ (Token::FuncDet
        | Token::FuncTranspose
        | Token::FuncIdentity
        | Token::FuncInv
        | Token::FuncCofactor
        | Token::FuncAdjugate) => {
            let function = token_to_matrix_function(tok)?;
            if cursor.advance() != Some(Token::LeftParen) {
                return None;
            }
            let arg = parse_expression(cursor, tree)?;
            if cursor.advance() != Some(Token::RightParen) {
                return None;
            }
            tree.allocate_node(AstNode::MatrixFunctionCall {
                function,
                argument_index: arg,
            })
        }

        Token::LeftBracket => {
            let mut rows = 0u8;
            let mut cols: Option<u8> = None;
            let mut data = [0i64; super::matrix::MAX_MATRIX_CELLS];
            let mut pos = 0usize;

            loop {
                if cursor.peek() != Some(Token::LeftParen) {
                    return None;
                }
                cursor.advance();

                let mut row_cols = 0u8;
                loop {
                    let val = match cursor.advance()? {
                        Token::Number(v) => v,
                        _ => return None,
                    };
                    if pos >= super::matrix::MAX_MATRIX_CELLS {
                        return None;
                    }
                    data[pos] = val;
                    pos += 1;
                    row_cols += 1;

                    match cursor.peek() {
                        Some(Token::Comma) => {
                            cursor.advance();
                            continue;
                        }
                        Some(Token::RightParen) => {
                            cursor.advance();
                            break;
                        }
                        _ => return None,
                    }
                }

                if let Some(expected) = cols {
                    if row_cols != expected {
                        return None;
                    }
                } else {
                    if row_cols == 0 || row_cols as usize > super::matrix::MAX_MATRIX_DIM {
                        return None;
                    }
                    cols = Some(row_cols);
                }
                rows += 1;
                if rows as usize > super::matrix::MAX_MATRIX_DIM {
                    return None;
                }

                match cursor.peek() {
                    Some(Token::LeftParen) => continue,
                    Some(Token::RightBracket) => {
                        cursor.advance();
                        break;
                    }
                    _ => return None,
                }
            }

            if rows == 0 || pos == 0 {
                return None;
            }

            let m = Matrix::mat_from_slice(&data[..pos], rows, cols.unwrap_or(0))?;
            let idx = tree.allocate_matrix(m)?;
            tree.allocate_node(AstNode::MatrixLiteral { cache_index: idx })
        }

        Token::LeftParen => {
            let inner = parse_expression(cursor, tree)?;
            if cursor.advance() != Some(Token::RightParen) {
                return None;
            }
            Some(inner)
        }

        _ => None,
    }
}

fn is_single_arg_function_token(token: Token) -> bool {
    token_to_single_arg_function(token).is_some()
}

fn is_three_arg_function_token(token: Token) -> bool {
    matches!(token, Token::FuncBinomP)
}

fn is_two_arg_function_token(token: Token) -> bool {
    matches!(
        token,
        Token::FuncPoissonP | Token::FuncChiCDF | Token::FuncNthRoot
    )
}

fn is_loop_aggregate_token(token: Token) -> bool {
    matches!(token, Token::FuncSum | Token::FuncInt)
}

fn is_primary_start(token: Token) -> bool {
    matches!(
        token,
        Token::Number(_)
            | Token::VarAns
            | Token::VarRegister(_)
            | Token::MatRegister(_)
            | Token::ConstPi
            | Token::ConstE
            | Token::ConstI
            | Token::LeftParen
            | Token::LeftBracket
            | Token::FuncDet
            | Token::FuncTranspose
            | Token::FuncIdentity
            | Token::FuncInv
            | Token::FuncCofactor
            | Token::FuncAdjugate
    ) || is_single_arg_function_token(token)
        || is_three_arg_function_token(token)
        || is_two_arg_function_token(token)
        || is_loop_aggregate_token(token)
        || token == Token::FuncSto
}

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

fn token_to_three_arg_function(token: Token) -> Option<ThreeArgMathFunction> {
    Some(match token {
        Token::FuncBinomP => ThreeArgMathFunction::BinomialProbability,
        _ => return None,
    })
}

fn token_to_two_arg_function(token: Token) -> Option<TwoArgMathFunction> {
    Some(match token {
        Token::FuncPoissonP => TwoArgMathFunction::PoissonProbability,
        Token::FuncChiCDF => TwoArgMathFunction::ChiSquaredCDF,
        Token::FuncNthRoot => TwoArgMathFunction::NthRoot,
        _ => return None,
    })
}

fn token_to_matrix_function(token: Token) -> Option<MatrixFunction> {
    Some(match token {
        Token::FuncDet => MatrixFunction::Det,
        Token::FuncTranspose => MatrixFunction::Transpose,
        Token::FuncIdentity => MatrixFunction::Identity,
        Token::FuncInv => MatrixFunction::Inv,
        Token::FuncCofactor => MatrixFunction::Cofactor,
        Token::FuncAdjugate => MatrixFunction::Adjugate,
        _ => return None,
    })
}
