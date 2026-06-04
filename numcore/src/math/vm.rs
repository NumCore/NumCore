use super::compiler::Bytecode;
use super::complex::Complex as Cplx;
use super::fixed_point as fp;
use super::matrix::{normalize_scientific, Matrix, MatrixKind};
use super::opcodes::Op;
use super::parser::{
    BinaryOperator, MathFunction, MatrixFunction, ThreeArgMathFunction, TwoArgMathFunction,
};
use super::vars::VariableStore;
use super::{AngleMode, MathMode};
use crate::math::evaluator::{
    apply_binary_operator, apply_function, apply_three_arg_function, apply_two_arg_function,
    compute_overflow, take_overflow_info, EvalResult,
};

pub const VALUE_STACK_SIZE: usize = 8;

macro_rules! push {
    ($st:expr, $sp:expr, $v:expr) => {{
        if $sp >= VALUE_STACK_SIZE {
            return EvalResult::DomainError;
        }
        $st[$sp] = Some($v);
        $sp += 1;
    }};
}
macro_rules! pop {
    ($st:expr, $sp:expr) => {{
        if $sp == 0 {
            return EvalResult::DomainError;
        }
        $sp -= 1;
        match $st[$sp].take() {
            Some(v) => v,
            None => return EvalResult::DomainError,
        }
    }};
}
macro_rules! unary_op {
    ($st:expr, $sp:expr, $of:expr, $func:expr) => {{
        if $sp == 0 {
            return EvalResult::DomainError;
        }
        $sp -= 1;
        let m = match $st[$sp].take() {
            Some(v) => v,
            None => return EvalResult::DomainError,
        };
        let c = match m.to_complex() {
            Some(c) => c,
            None => return EvalResult::DomainError,
        };
        match ($func)(c) {
            Some(r) => {
                $st[$sp] = Some(Matrix::from_complex(r));
                $sp += 1;
            }
            None => match take_overflow_info() {
                Some(info) => {
                    $of = Some(info);
                    // Push a sentinel that to_complex() returns None for,
                    // so binary ops know this operand overflowed.
                    $st[$sp] = Some(Matrix {
                        data: [0; 16],
                        rows: 0,
                        cols: 0,
                        kind: MatrixKind::Mat,
                    });
                    $sp += 1;
                }
                None => return EvalResult::DomainError, // domain error, not overflow
            },
        }
    }};
}

pub fn execute(
    bc: &Bytecode,
    vars: &mut VariableStore,
    angle_mode: AngleMode,
    mode: MathMode,
) -> EvalResult {
    let mut st: [Option<Matrix>; VALUE_STACK_SIZE] = [None; VALUE_STACK_SIZE];
    let mut sp: usize = 0;
    let mut pc: usize = 0;
    let mut overflow: Option<(i64, bool)> = None;
    take_overflow_info(); // clear any stale overflow state from previous evaluations

    loop {
        let op_byte = match bc.code.get(pc) {
            Some(&b) => b,
            None => return EvalResult::DomainError,
        };
        pc += 1;
        let op = match Op::from_u8(op_byte) {
            Some(o) => o,
            None => return EvalResult::DomainError,
        };

        // If we have pending overflow, skip unary function calls.
        // Other opcodes (Push, arithmetic, Sto) execute normally so
        // subsequent binary ops have the right operands on the stack.
        if overflow.is_some()
            && matches!(
                op,
                Op::CallFunction
                    | Op::CallMatrixFunc
                    | Op::CallBinomP
                    | Op::CallPoissonP
                    | Op::CallChiCDF
                    | Op::CallNthRoot
                    | Op::ConstructSci
            )
        {
            continue;
        }

        match op {
            Op::PushI64 => {
                let end = pc + 8;
                let val = i64::from_le_bytes(match bc.code.get(pc..end) {
                    Some(s) => s.try_into().unwrap(),
                    None => return EvalResult::DomainError,
                });
                pc = end;
                push!(st, sp, Matrix::scalar(val));
            }
            Op::PushReg => {
                let ch = bc.code[pc];
                pc += 1;
                let c = vars.read_register(ch).unwrap_or(Cplx::zero());
                push!(
                    st,
                    sp,
                    if c.im == 0 {
                        Matrix::scalar(c.re)
                    } else {
                        Matrix::complex(c.re, c.im)
                    }
                );
            }
            Op::PushAns => {
                let m = vars
                    .read_matrix_ans()
                    .or_else(|| {
                        vars.read_ans().map(|c| {
                            if c.im == 0 {
                                Matrix::scalar(c.re)
                            } else {
                                Matrix::complex(c.re, c.im)
                            }
                        })
                    })
                    .unwrap_or(Matrix::scalar(0));
                push!(st, sp, m);
            }
            Op::PushConstPi => push!(st, sp, Matrix::scalar(fp::FIXED_PI)),
            Op::PushConstE => push!(st, sp, Matrix::scalar(fp::FIXED_E)),
            Op::PushConstI => push!(st, sp, Matrix::complex(0, fp::FIXED_ONE)),
            Op::PushMatReg => {
                let ch = bc.code[pc];
                pc += 1;
                push!(
                    st,
                    sp,
                    vars.read_matrix_reg(ch).unwrap_or(Matrix::scalar(0))
                );
            }
            Op::PushMatLit => {
                let idx = bc.code[pc] as usize;
                pc += 1;
                push!(
                    st,
                    sp,
                    bc.mat_cache
                        .get(idx)
                        .copied()
                        .flatten()
                        .unwrap_or(Matrix::scalar(0))
                );
            }
            Op::ConstructSci => {
                let exp = pop!(st, sp);
                let mant = pop!(st, sp);
                let e = match exp.to_complex() {
                    Some(c) if c.im == 0 => c.re,
                    _ => return EvalResult::DomainError,
                };
                let m = match mant.to_complex() {
                    Some(c) if c.im == 0 => c.re,
                    _ => return EvalResult::DomainError,
                };
                if m == 0 {
                    push!(st, sp, Matrix::scalar(0));
                    continue;
                }
                let (m_n, ea) = match normalize_scientific(m, 0) {
                    Some(v) => v,
                    None => return EvalResult::DomainError,
                };
                let te = e + ea;
                if te > 99 || te < -99 {
                    let mut sentinel = Matrix::scalar(0);
                    sentinel.kind = MatrixKind::Scientific;
                    sentinel.data[1] = if te > 0 { 100 } else { -100 };
                    push!(st, sp, sentinel);
                    continue;
                }
                // Convert to Scalar if the value fits exactly in Q31.32
                // Only for non-negative exponents (multiply is exact).
                let sci = Matrix::scientific(m_n, te).unwrap_or(Matrix::scalar(0));
                if let Some((sm, se)) = sci.to_scientific() {
                    if (se >= 0 && se <= 8) || se == 9 {
                        if let Some(v) = crate::math::evaluator::sci_to_scalar(sm, se) {
                            push!(st, sp, Matrix::scalar(v));
                            continue;
                        }
                    }
                }
                push!(st, sp, sci);
            }

            Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Pow => {
                if let Some((log_est, neg)) = overflow {
                    // Overflow propagation: pop both, adjust overflow info, keep overflow
                    let right_val = pop!(st, sp);
                    let left_val = pop!(st, sp);
                    let bin_op = match op {
                        Op::Add => BinaryOperator::Add,
                        Op::Sub => BinaryOperator::Subtract,
                        Op::Mul => BinaryOperator::Multiply,
                        Op::Div => BinaryOperator::Divide,
                        Op::Mod => BinaryOperator::Modulo,
                        Op::Pow => BinaryOperator::Power,
                        _ => unreachable!(),
                    };
                    // Adjust overflow info like the original evaluate_node does
                    let l_scalar = left_val.to_complex();
                    let r_scalar = right_val.to_complex();
                    let adjusted = match (bin_op, l_scalar, r_scalar) {
                        (BinaryOperator::Divide, _, Some(r)) if r.re != 0 => Some(
                            fp::log10(r.re.unsigned_abs() as i64)
                                .map(|d| (log_est.wrapping_sub(d), neg != (r.re < 0)))
                                .unwrap_or((log_est, neg)),
                        ),
                        (BinaryOperator::Divide, Some(_), None) => None, // 5 / overflow → 0, clear overflow
                        (BinaryOperator::Multiply, _, Some(r)) => Some(
                            fp::log10(r.re.unsigned_abs() as i64)
                                .map(|m| (log_est.wrapping_add(m), neg != (r.re < 0)))
                                .unwrap_or((log_est, neg)),
                        ),
                        (BinaryOperator::Multiply, Some(l), _) => Some(
                            fp::log10(l.re.unsigned_abs() as i64)
                                .map(|m| (log_est.wrapping_add(m), neg != (l.re < 0)))
                                .unwrap_or((log_est, neg)),
                        ),
                        (BinaryOperator::Power, _, Some(r)) if r.im == 0 && r.re > 0 => Some(
                            fp::multiply(log_est, r.re)
                                .map(|p| (p, neg))
                                .unwrap_or((log_est, neg)),
                        ),
                        (BinaryOperator::Power, Some(_), None) => None,
                        (BinaryOperator::Add, Some(_), None)
                        | (BinaryOperator::Subtract, Some(_), None) => None,
                        _ => Some((log_est, neg)),
                    };
                    overflow = adjusted;
                    if adjusted.is_some() {
                        push!(st, sp, Matrix::scalar(0));
                    } else {
                        push!(st, sp, Matrix::scalar(0)); // push 0 for the cleared overflow case
                    }
                } else {
                    let right = pop!(st, sp);
                    let left = pop!(st, sp);
                    let bin_op = match op {
                        Op::Add => BinaryOperator::Add,
                        Op::Sub => BinaryOperator::Subtract,
                        Op::Mul => BinaryOperator::Multiply,
                        Op::Div => BinaryOperator::Divide,
                        Op::Mod => BinaryOperator::Modulo,
                        Op::Pow => BinaryOperator::Power,
                        _ => unreachable!(),
                    };
                    match apply_binary_operator(bin_op, left, right) {
                        Some(m) => {
                            // result_or_scalar in apply_sci_sci handles Scalar↔Scientific conversion.
                            // Do NOT re-promote here — it would undo the auto-conversion.
                            push!(st, sp, m);
                        }
                        None => match take_overflow_info() {
                            Some(v) => {
                                overflow = Some(v);
                                push!(st, sp, Matrix::scalar(0));
                            }
                            None => return EvalResult::DomainError,
                        },
                    }
                }
            }
            Op::Neg => {
                if overflow.is_some() {
                    if let Some((log_est, neg)) = overflow {
                        overflow = Some((log_est, !neg));
                    }
                } else {
                    let v = pop!(st, sp);
                    push!(st, sp, v.negate());
                }
            }
            Op::CallFunction => {
                let fn_idx = bc.code[pc];
                pc += 1;
                let func = match MathFunction::from_u8(fn_idx) {
                    Some(f) => f,
                    None => return EvalResult::DomainError,
                };
                unary_op!(st, sp, overflow, |c: Cplx| apply_function(
                    func, c, angle_mode
                ));
            }

            Op::CallMatrixFunc => {
                let fn_idx = bc.code[pc];
                pc += 1;
                let func = match MatrixFunction::from_u8(fn_idx) {
                    Some(f) => f,
                    None => return EvalResult::DomainError,
                };
                let m = pop!(st, sp);
                let result = match func {
                    MatrixFunction::Det => m.determinant().map(Matrix::scalar),
                    MatrixFunction::Transpose => m.transpose(),
                    MatrixFunction::Identity => match m.to_complex() {
                        Some(c) if c.im == 0 => {
                            let n = fp::to_integer_truncated(c.re);
                            if n >= 1 && (n as usize) <= 4 {
                                Matrix::identity(n as u8)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    },
                    MatrixFunction::Inv => m.inverse(),
                    MatrixFunction::Cofactor => m.cofactor(),
                    MatrixFunction::Adjugate => m.adjugate(),
                };
                match result {
                    Some(r) => push!(st, sp, r),
                    None => return EvalResult::DomainError,
                }
            }

            Op::CallBinomP => {
                let a2 = pop!(st, sp);
                let a1 = pop!(st, sp);
                let a0 = pop!(st, sp);
                match apply_three_arg_function(
                    ThreeArgMathFunction::BinomialProbability,
                    a0.to_complex().unwrap_or(Cplx::zero()),
                    a1.to_complex().unwrap_or(Cplx::zero()),
                    a2.to_complex().unwrap_or(Cplx::zero()),
                ) {
                    Some(r) => push!(st, sp, Matrix::from_complex(r)),
                    None => return EvalResult::DomainError,
                }
            }
            Op::CallPoissonP => {
                let a1 = pop!(st, sp);
                let a0 = pop!(st, sp);
                match apply_two_arg_function(
                    TwoArgMathFunction::PoissonProbability,
                    a0.to_complex().unwrap_or(Cplx::zero()),
                    a1.to_complex().unwrap_or(Cplx::zero()),
                ) {
                    Some(r) => push!(st, sp, Matrix::from_complex(r)),
                    None => return EvalResult::DomainError,
                }
            }
            Op::CallChiCDF => {
                let a1 = pop!(st, sp);
                let a0 = pop!(st, sp);
                match apply_two_arg_function(
                    TwoArgMathFunction::ChiSquaredCDF,
                    a0.to_complex().unwrap_or(Cplx::zero()),
                    a1.to_complex().unwrap_or(Cplx::zero()),
                ) {
                    Some(r) => push!(st, sp, Matrix::from_complex(r)),
                    None => return EvalResult::DomainError,
                }
            }
            Op::CallNthRoot => {
                let a1 = pop!(st, sp);
                let a0 = pop!(st, sp);
                match apply_two_arg_function(
                    TwoArgMathFunction::NthRoot,
                    a0.to_complex().unwrap_or(Cplx::zero()),
                    a1.to_complex().unwrap_or(Cplx::zero()),
                ) {
                    Some(r) => push!(st, sp, Matrix::from_complex(r)),
                    None => return EvalResult::DomainError,
                }
            }

            Op::Sto => {
                let ch = bc.code[pc];
                pc += 1;
                let v = pop!(st, sp);
                if let Some(c) = v.to_complex() {
                    vars.write_register(ch, c);
                }
                push!(st, sp, v);
            }
            Op::StoMat => {
                let ch = bc.code[pc];
                pc += 1;
                let v = pop!(st, sp);
                vars.write_matrix_reg(ch, v);
                push!(st, sp, v);
            }

            Op::LoopSum | Op::LoopInt => {
                let var = bc.code[pc];
                pc += 1;
                let past_end_off = read_u16(&bc.code, &mut pc);
                let body_len = read_u16(&bc.code, &mut pc) as usize;
                let body_start = pc;
                let body_end = pc + body_len;

                let end_m = pop!(st, sp);
                let start_m = pop!(st, sp);
                let start_c = start_m.to_complex().unwrap_or(Cplx::zero());
                let end_c = end_m.to_complex().unwrap_or(Cplx::zero());
                if start_c.im != 0 || end_c.im != 0 {
                    return EvalResult::DomainError;
                }

                let saved = vars.read_register(var);
                let result = match op {
                    Op::LoopSum => {
                        let si = fp::to_integer_truncated(start_c.re);
                        let ei = fp::to_integer_truncated(end_c.re);
                        if ei < si || ei - si > 10_000 {
                            return EvalResult::DomainError;
                        }
                        let mut acc = Cplx::zero();
                        let mut k = si;
                        while k <= ei {
                            vars.write_register(var, Cplx::from_real(fp::from_integer(k)));
                            let body_bc = bc_copy(bc, body_start, body_end);
                            match execute(&body_bc, vars, angle_mode, MathMode::Advanced) {
                                EvalResult::Matrix(m) => {
                                    if let Some(c) = m.to_complex() {
                                        acc = acc.add(c);
                                    }
                                }
                                _ => return EvalResult::DomainError,
                            }
                            k += 1;
                        }
                        Cplx::from_real(acc.re)
                    }
                    Op::LoopInt => match adaptive_simpson_exec(
                        var, start_c.re, end_c.re, bc, body_start, body_end, vars, angle_mode,
                    ) {
                        Some(r) => r,
                        None => return EvalResult::DomainError,
                    },
                    _ => unreachable!(),
                };
                if let Some(val) = saved {
                    vars.write_register(var, val);
                }
                push!(st, sp, Matrix::from_complex(result));
                pc = body_start + past_end_off as usize;
            }

            Op::Halt => {
                if let Some((log_est, negative)) = overflow {
                    return compute_overflow(log_est, negative);
                }
                if sp != 1 {
                    return EvalResult::DomainError;
                }
                let result = match st[0].take() {
                    Some(v) => v,
                    None => return EvalResult::DomainError,
                };
                return match (result.kind, mode) {
                    (MatrixKind::Complex, MathMode::Standard)
                    | (MatrixKind::Mat, MathMode::Standard)
                    | (MatrixKind::Mat, MathMode::Advanced) => EvalResult::DomainError,
                    _ => EvalResult::Matrix(result),
                };
            }
        }
    }
}

fn read_u16(code: &[u8], pc: &mut usize) -> u16 {
    let lo = code[*pc] as u16;
    let hi = code[*pc + 1] as u16;
    *pc += 2;
    lo | (hi << 8)
}

fn bc_copy(bc: &Bytecode, start: usize, end: usize) -> Bytecode {
    let mut copy = Bytecode::new();
    let len = end - start;
    if len <= 256 {
        copy.code[..len].copy_from_slice(&bc.code[start..end]);
        copy.len = len as u16;
    }
    copy
}

fn adaptive_simpson_exec(
    variable: u8,
    a: i64,
    b: i64,
    bc: &Bytecode,
    body_start: usize,
    body_end: usize,
    vars: &mut VariableStore,
    angle_mode: AngleMode,
) -> Option<Cplx> {
    use crate::math::evaluator::{ADAPTIVE_MAX_DEPTH, ADAPTIVE_MAX_EVALS, ADAPTIVE_TOL};
    let (start, end, negate) = if a <= b { (a, b, false) } else { (b, a, true) };

    let mut body_bc = Bytecode::new();
    let blen = body_end - body_start;
    if blen <= 256 {
        body_bc.code[..blen].copy_from_slice(&bc.code[body_start..body_end]);
        body_bc.len = blen as u16;
    } else {
        return None;
    }

    let eval_body = |x: i64, vars: &mut VariableStore| -> Option<i64> {
        vars.write_register(variable, Cplx::from_real(x));
        match execute(&body_bc, vars, angle_mode, MathMode::Advanced) {
            EvalResult::Matrix(m) => m.to_complex().map(|c| c.re),
            _ => None,
        }
    };

    let fa = eval_body(start, vars)?;
    let fb = eval_body(end, vars)?;
    if start == end {
        return Some(Cplx::zero());
    }

    #[derive(Clone, Copy)]
    struct AdSimpTask {
        a: i64,
        b: i64,
        fa_re: i64,
        fb_re: i64,
        tol: i64,
        depth: u32,
    }

    let mut tasks = [AdSimpTask {
        a: 0,
        b: 0,
        fa_re: 0,
        fb_re: 0,
        tol: 0,
        depth: 0,
    }; 24];
    let mut stack_len: u32 = 1;
    let mut result: i64 = 0;
    let mut total_evals: u32 = 2;

    tasks[0] = AdSimpTask {
        a: start,
        b: end,
        fa_re: fa,
        fb_re: fb,
        tol: ADAPTIVE_TOL,
        depth: 0,
    };

    while stack_len > 0 {
        stack_len -= 1;
        let task = tasks[stack_len as usize];
        if task.depth >= ADAPTIVE_MAX_DEPTH || total_evals >= ADAPTIVE_MAX_EVALS {
            let h = task.b.wrapping_sub(task.a);
            let m = task.a + (h >> 1);
            let fm = eval_body(m, vars)?;
            total_evals += 1;
            result = result.saturating_add(simpson_step(h, task.fa_re, fm, task.fb_re)?);
            continue;
        }
        let h = task.b.checked_sub(task.a)?;
        let m = task.a + (h >> 1);
        let fm = eval_body(m, vars)?;
        total_evals += 1;
        let s_ab = simpson_step(h, task.fa_re, fm, task.fb_re)?;
        let fl = eval_body(task.a + ((m - task.a) >> 1), vars)?;
        total_evals += 1;
        let fr = eval_body(m + ((task.b - m) >> 1), vars)?;
        total_evals += 1;
        let s_am = simpson_step(m - task.a, task.fa_re, fl, fm)?;
        let s_mb = simpson_step(task.b - m, fm, fr, task.fb_re)?;
        let error = (s_am as i128)
            .wrapping_add(s_mb as i128)
            .wrapping_sub(s_ab as i128);
        let error_abs = if error < 0 {
            error.wrapping_neg() as u128
        } else {
            error as u128
        };
        if error_abs < 15u128 * (task.tol as u128) {
            result = result.saturating_add(s_am.saturating_add(s_mb));
        } else {
            let ct = (task.tol >> 1).max(1);
            let nl = stack_len + 2;
            if (nl as usize) < tasks.len() {
                let idx = stack_len as usize;
                tasks[idx] = AdSimpTask {
                    a: m,
                    b: task.b,
                    fa_re: fm,
                    fb_re: task.fb_re,
                    tol: ct,
                    depth: task.depth + 1,
                };
                tasks[idx + 1] = AdSimpTask {
                    a: task.a,
                    b: m,
                    fa_re: task.fa_re,
                    fb_re: fm,
                    tol: ct,
                    depth: task.depth + 1,
                };
                stack_len = nl;
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
    Some(Cplx::from_real(if (final_result - nearest).abs() < 4295 {
        nearest
    } else {
        final_result
    }))
}

fn simpson_step(h: i64, fa: i64, fm: i64, fb: i64) -> Option<i64> {
    let ws = (fa as i128) + 4 * (fm as i128) + (fb as i128);
    let prod = (h as i128) * ws;
    let result = prod / (6 * fp::SCALE as i128);
    if result > i64::MAX as i128 || result < i64::MIN as i128 {
        None
    } else {
        Some(result as i64)
    }
}
