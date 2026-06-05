use super::lexer::{LexResult, Token};
use super::matrix::Matrix;
use super::opcodes::Op;
use super::parser::{MathFunction, MatrixFunction};

pub const MAX_BYTECODE: usize = 256;
pub const MATRIX_CACHE_SIZE: usize = 2;

pub struct Bytecode {
    pub code: [u8; MAX_BYTECODE],
    pub len: u16,
    pub mat_cache: [Option<Matrix>; MATRIX_CACHE_SIZE],
    pub mat_cache_count: u16,
}

impl Bytecode {
    pub const fn new() -> Self {
        Self {
            code: [0; MAX_BYTECODE],
            len: 0,
            mat_cache: [None; MATRIX_CACHE_SIZE],
            mat_cache_count: 0,
        }
    }

    fn emit(&mut self, b: u8) -> Option<()> {
        if (self.len as usize) < MAX_BYTECODE {
            self.code[self.len as usize] = b;
            self.len += 1;
            Some(())
        } else {
            None
        }
    }

    fn emit_i64(&mut self, v: i64) -> Option<()> {
        self.emit(Op::PushI64 as u8)?;
        let bytes = v.to_le_bytes();
        for &b in &bytes {
            self.emit(b)?;
        }
        Some(())
    }

    fn reserve(&mut self, n: usize) -> Option<u16> {
        let pos = self.len;
        for _ in 0..n {
            self.emit(0)?;
        }
        Some(pos)
    }

    fn patch_u16(&mut self, pos: u16, val: u16) {
        let bytes = val.to_le_bytes();
        let p = pos as usize;
        self.code[p] = bytes[0];
        self.code[p + 1] = bytes[1];
    }

    pub fn allocate_matrix(&mut self, m: Matrix) -> Option<usize> {
        if self.mat_cache_count as usize >= MATRIX_CACHE_SIZE {
            return None;
        }
        let idx = self.mat_cache_count as usize;
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

pub fn compile(lex: &LexResult, bc: &mut Bytecode) -> Option<()> {
    bc.len = 0;
    bc.mat_cache_count = 0;
    let mut cursor = ParserCursor::new(lex);
    compile_expression(&mut cursor, bc)?;
    if !cursor.is_finished() {
        return None;
    }
    bc.emit(Op::Halt as u8)
}

fn compile_expression(cursor: &mut ParserCursor, bc: &mut Bytecode) -> Option<()> {
    compile_term(cursor, bc)?;
    while let Some(token) = cursor.peek() {
        match token {
            Token::Plus => {
                cursor.advance();
                compile_term(cursor, bc)?;
                bc.emit(Op::Add as u8)?;
            }
            Token::Minus => {
                cursor.advance();
                compile_term(cursor, bc)?;
                bc.emit(Op::Sub as u8)?;
            }
            _ => break,
        }
    }
    Some(())
}

fn compile_term(cursor: &mut ParserCursor, bc: &mut Bytecode) -> Option<()> {
    compile_power(cursor, bc)?;
    loop {
        let op = match cursor.peek() {
            Some(Token::Star) => {
                cursor.advance();
                Some(Op::Mul)
            }
            Some(Token::Slash) => {
                cursor.advance();
                Some(Op::Div)
            }
            Some(Token::Percent) => {
                cursor.advance();
                Some(Op::Mod)
            }
            _ => None,
        };
        if let Some(op) = op {
            compile_power(cursor, bc)?;
            bc.emit(op as u8)?;
            continue;
        }

        if let Some(token) = cursor.peek() {
            if is_primary_start(token) {
                compile_power(cursor, bc)?;
                bc.emit(Op::Mul as u8)?;
                continue;
            }
        }

        break;
    }
    Some(())
}

fn compile_power(cursor: &mut ParserCursor, bc: &mut Bytecode) -> Option<()> {
    compile_unary(cursor, bc)?;
    if cursor.peek() == Some(Token::Caret) {
        cursor.advance();
        compile_power(cursor, bc)?;
        bc.emit(Op::Pow as u8)?;
    }
    Some(())
}

fn compile_unary(cursor: &mut ParserCursor, bc: &mut Bytecode) -> Option<()> {
    if cursor.peek() == Some(Token::UnaryMinus) {
        cursor.advance();
        compile_unary(cursor, bc)?;
        bc.emit(Op::Neg as u8)?;
    } else {
        compile_primary(cursor, bc)?;
    }
    Some(())
}

fn compile_primary(cursor: &mut ParserCursor, bc: &mut Bytecode) -> Option<()> {
    let token = cursor.advance()?;
    match token {
        Token::Number(v) => {
            bc.emit_i64(v)?;
            if let Some(Token::Exponent(exp)) = cursor.peek() {
                cursor.advance();
                bc.emit_i64(exp)?;
                bc.emit(Op::ConstructSci as u8)?;
            }
        }
        Token::VarAns => bc.emit(Op::PushAns as u8)?,
        Token::VarRegister(ch) => {
            bc.emit(Op::PushReg as u8)?;
            bc.emit(ch)?;
        }
        Token::MatRegister(ch) => {
            bc.emit(Op::PushMatReg as u8)?;
            bc.emit(ch)?;
        }
        Token::ConstPi => bc.emit(Op::PushConstPi as u8)?,
        Token::ConstE => bc.emit(Op::PushConstE as u8)?,
        Token::ConstI => bc.emit(Op::PushConstI as u8)?,

        Token::LeftParen => {
            compile_expression(cursor, bc)?;
            if cursor.advance() != Some(Token::RightParen) {
                return None;
            }
        }

        Token::LeftBracket => compile_matrix_literal(cursor, bc)?,

        Token::FuncSin => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 0)?,
        Token::FuncCos => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 1)?,
        Token::FuncTan => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 2)?,
        Token::FuncAsin => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 3)?,
        Token::FuncAcos => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 4)?,
        Token::FuncAtan => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 5)?,
        Token::FuncSinH => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 6)?,
        Token::FuncCosH => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 7)?,
        Token::FuncTanH => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 8)?,
        Token::FuncASinH => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 9)?,
        Token::FuncACosH => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 10)?,
        Token::FuncATanH => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 11)?,
        Token::FuncSqrt => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 12)?,
        Token::FuncAbs => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 13)?,
        Token::FuncExp => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 17)?,
        Token::FuncLn => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 15)?,
        Token::FuncLog => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 14)?,
        Token::FuncLog2 => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 16)?,
        Token::FuncFloor => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 18)?,
        Token::FuncCeil => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 19)?,
        Token::FuncRound => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 20)?,
        Token::FuncDeg => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 21)?,
        Token::FuncRad => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 22)?,
        Token::FuncLnGamma => compile_single_arg_fn(cursor, bc, Op::CallFunction as u8, 23)?,

        Token::FuncBinomP => compile_three_arg(cursor, bc, Op::CallBinomP)?,
        Token::FuncPoissonP => compile_two_arg(cursor, bc, Op::CallPoissonP)?,
        Token::FuncChiCDF => compile_two_arg(cursor, bc, Op::CallChiCDF)?,
        Token::FuncNthRoot => compile_two_arg(cursor, bc, Op::CallNthRoot)?,

        Token::FuncDet => compile_single_arg_fn(cursor, bc, Op::CallMatrixFunc as u8, 0)?,
        Token::FuncTranspose => compile_single_arg_fn(cursor, bc, Op::CallMatrixFunc as u8, 1)?,
        Token::FuncIdentity => compile_single_arg_fn(cursor, bc, Op::CallMatrixFunc as u8, 2)?,
        Token::FuncInv => compile_single_arg_fn(cursor, bc, Op::CallMatrixFunc as u8, 3)?,
        Token::FuncCofactor => compile_single_arg_fn(cursor, bc, Op::CallMatrixFunc as u8, 4)?,
        Token::FuncAdjugate => compile_single_arg_fn(cursor, bc, Op::CallMatrixFunc as u8, 5)?,

        Token::FuncSto => {
            if cursor.advance() != Some(Token::LeftParen) {
                return None;
            }
            compile_expression(cursor, bc)?;
            if cursor.advance() != Some(Token::Comma) {
                return None;
            }
            let reg = cursor.advance()?;
            if cursor.advance() != Some(Token::RightParen) {
                return None;
            }
            match reg {
                Token::VarRegister(ch) => {
                    bc.emit(Op::Sto as u8)?;
                    bc.emit(ch)?;
                }
                Token::MatRegister(ch) => {
                    bc.emit(Op::StoMat as u8)?;
                    bc.emit(ch)?;
                }
                _ => return None,
            }
        }

        Token::FuncSum | Token::FuncInt => {
            if cursor.advance() != Some(Token::LeftParen) {
                return None;
            }
            // Compile body into a temp buffer first
            let mut body_bc = Bytecode::new();
            compile_expression(cursor, &mut body_bc)?;
            let body_len = body_bc.len;

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
            compile_expression(cursor, bc)?; // start (emits to main bc)
            if cursor.advance() != Some(Token::Comma) {
                return None;
            }
            compile_expression(cursor, bc)?; // end (emits to main bc)
            if cursor.advance() != Some(Token::RightParen) {
                return None;
            }

            // Emit: Loop, variable, u16(body_len), u16(past_end) then body bytes
            let loop_op = if token == Token::FuncSum {
                Op::LoopSum
            } else {
                Op::LoopInt
            };
            bc.emit(loop_op as u8)?;
            bc.emit(variable)?;
            let body_len_pos = bc.len;
            bc.reserve(2)?; // placeholder: body length
            let past_end_pos = bc.len;
            bc.reserve(2)?; // placeholder: past-end offset
                            // Copy body bytes + Halt terminator
            for i in 0..body_len as usize {
                bc.emit(body_bc.code[i])?;
            }
            bc.emit(Op::Halt as u8)?; // body terminator
                                      // Patch body_len (includes Halt byte)
            bc.patch_u16(body_len_pos, body_len + 1);
            // Patch past_end = distance from after header to end of body
            bc.patch_u16(past_end_pos, bc.len - (past_end_pos + 2));
        }

        _ => return None,
    }
    Some(())
}

fn compile_single_arg(cursor: &mut ParserCursor, bc: &mut Bytecode, op: Op) -> Option<()> {
    if cursor.advance() != Some(Token::LeftParen) {
        return None;
    }
    compile_expression(cursor, bc)?;
    if cursor.advance() != Some(Token::RightParen) {
        return None;
    }
    bc.emit(op as u8)
}

fn compile_single_arg_fn(
    cursor: &mut ParserCursor,
    bc: &mut Bytecode,
    op_byte: u8,
    fn_byte: u8,
) -> Option<()> {
    if cursor.advance() != Some(Token::LeftParen) {
        return None;
    }
    compile_expression(cursor, bc)?;
    if cursor.advance() != Some(Token::RightParen) {
        return None;
    }
    bc.emit(op_byte)?;
    bc.emit(fn_byte)
}

fn compile_two_arg(cursor: &mut ParserCursor, bc: &mut Bytecode, op: Op) -> Option<()> {
    if cursor.advance() != Some(Token::LeftParen) {
        return None;
    }
    compile_expression(cursor, bc)?;
    if cursor.advance() != Some(Token::Comma) {
        return None;
    }
    compile_expression(cursor, bc)?;
    if cursor.advance() != Some(Token::RightParen) {
        return None;
    }
    bc.emit(op as u8)
}

fn compile_three_arg(cursor: &mut ParserCursor, bc: &mut Bytecode, op: Op) -> Option<()> {
    if cursor.advance() != Some(Token::LeftParen) {
        return None;
    }
    compile_expression(cursor, bc)?;
    if cursor.advance() != Some(Token::Comma) {
        return None;
    }
    compile_expression(cursor, bc)?;
    if cursor.advance() != Some(Token::Comma) {
        return None;
    }
    compile_expression(cursor, bc)?;
    if cursor.advance() != Some(Token::RightParen) {
        return None;
    }
    bc.emit(op as u8)
}

fn compile_matrix_literal(cursor: &mut ParserCursor, bc: &mut Bytecode) -> Option<()> {
    let mut rows = 0u8;
    let mut cols: Option<u8> = None;
    let mut data = [0i64; 16];
    let mut pos = 0usize;

    loop {
        if cursor.peek() != Some(Token::LeftParen) {
            return None;
        }
        cursor.advance();
        let mut row_cols = 0u8;
        loop {
            let negative = cursor.peek() == Some(Token::UnaryMinus);
            if negative {
                cursor.advance();
            }
            let val = match cursor.advance()? {
                Token::Number(v) => {
                    if negative {
                        v.wrapping_neg()
                    } else {
                        v
                    }
                }
                _ => return None,
            };
            if pos >= 16 {
                return None;
            }
            data[pos] = val;
            pos += 1;
            row_cols += 1;
            match cursor.peek()? {
                Token::Comma => {
                    cursor.advance();
                    continue;
                }
                Token::RightParen => {
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
            if row_cols == 0 || row_cols as usize > 4 {
                return None;
            }
            cols = Some(row_cols);
        }
        rows += 1;
        if rows as usize > 4 {
            return None;
        }
        match cursor.peek()? {
            Token::LeftParen => continue,
            Token::RightBracket => {
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
    let idx = bc.allocate_matrix(m)?;
    bc.emit(Op::PushMatLit as u8)?;
    bc.emit(idx as u8)?;
    Some(())
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
    ) || is_single_arg(token)
        || is_three_arg(token)
        || is_two_arg(token)
        || is_loop(token)
        || is_matrix_func(token)
        || token == Token::FuncSto
}

fn is_single_arg(token: Token) -> bool {
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

fn is_three_arg(token: Token) -> bool {
    matches!(token, Token::FuncBinomP)
}

fn is_two_arg(token: Token) -> bool {
    matches!(
        token,
        Token::FuncPoissonP | Token::FuncChiCDF | Token::FuncNthRoot
    )
}

fn is_loop(token: Token) -> bool {
    matches!(token, Token::FuncSum | Token::FuncInt)
}

fn is_matrix_func(token: Token) -> bool {
    matches!(
        token,
        Token::FuncDet
            | Token::FuncTranspose
            | Token::FuncIdentity
            | Token::FuncInv
            | Token::FuncCofactor
            | Token::FuncAdjugate
    )
}
