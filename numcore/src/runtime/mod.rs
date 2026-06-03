pub mod event;
pub mod state;

use crate::hal::{Display, Uart};
use crate::math::engine;
use crate::math::engine::EvalResult;
use crate::ui::formula;
use event::{translate_input_byte_to_event, CalcEvent};
use state::CalcState;

static mut CALCULATOR_STATE: CalcState = CalcState::new();

pub fn start<U: Uart, D: Display>() -> ! {
    initialise_all_hardware::<U, D>();
    print_welcome_banner::<U>();

    let calculator_state = unsafe { &mut *(&raw mut CALCULATOR_STATE) };

    U::transmit_bytes(b"> ");
    calculator_state.set_last_result(b"ready");
    render_oled::<D>(calculator_state);

    run_event_loop::<U, D>(calculator_state)
}

fn initialise_all_hardware<U: Uart, D: Display>() {
    U::init();
    D::init();
    D::render(&D::new_buffer());
}

fn print_welcome_banner<U: Uart>() {
    U::transmit_bytes(b"\r\nNumCore v0.5  LM3S811  Q31.32 PEMDAS\r\n");
    U::transmit_bytes(b"Modes: Standard | Advanced | Matrix  (Esc cycles)\r\n");
    U::transmit_bytes(b"Scalar/Complex: + - * / ^ %  |  sin cos tan asin acos atan\r\n");
    U::transmit_bytes(b"  sinh cosh tanh asinh acosh atanh sqrt abs exp log ln\r\n");
    U::transmit_bytes(b"  log2 floor ceil round deg rad nthroot lngamma sto(v,r)\r\n");
    U::transmit_bytes(b"  binomp(n,k,p) poissonp(l,k) chicdf(x,k)\r\n");
    U::transmit_bytes(b"  sum(expr,v,a,b) int(expr,v,a,b)\r\n");
    U::transmit_bytes(b"Matrix: det inv transpose identity cofactor adj\r\n");
    U::transmit_bytes(b"  MatA MatB MatC  |  [(a,b)(c,d)]  |  k*MatA  MatA*MatB\r\n");
    U::transmit_bytes(b"Const: pi e  |  Vars: Ans A-Z  |  MatA MatB MatC\r\n\r\n");
}

// ─── ANSI escape sequence parser ─────────────────────────────────────────────
//
// Physical arrow keys send multi-byte sequences: 0x1B [ D (left) or 0x1B [ C
// (right).  The event loop buffers up to 3 bytes to detect these sequences.
// Standalone 0x1B (Escape key) fires ToggleMode — but only when no second byte
// follows in the same poll cycle (a short timeout by MCU standards at 50 MHz).

const ANSI_BUF_CAP: usize = 3;

#[derive(PartialEq)]
enum AnsiSeq {
    None,
    PendingEscape,
    PendingBracket,
}

fn run_event_loop<U: Uart, D: Display>(state: &mut CalcState) -> ! {
    let mut ansi_buf = [0u8; ANSI_BUF_CAP];
    let mut ansi_len = 0usize;
    let mut ansi_state = AnsiSeq::None;
    let mut ansi_idle = 0usize;

    loop {
        if let Some(raw_byte) = U::poll_byte() {
            ansi_idle = 0;
            match ansi_state {
                AnsiSeq::None => {
                    if raw_byte == 0x1B {
                        ansi_buf[0] = 0x1B;
                        ansi_len = 1;
                        ansi_state = AnsiSeq::PendingEscape;
                    } else {
                        handle_event::<U, D>(translate_input_byte_to_event(raw_byte), state);
                    }
                }
                AnsiSeq::PendingEscape => {
                    if raw_byte == b'[' {
                        ansi_buf[1] = b'[';
                        ansi_len = 2;
                        ansi_state = AnsiSeq::PendingBracket;
                    } else {
                        // 0x1B followed by non-[ → standalone Escape (ToggleMode),
                        // then process the current byte. If it's another 0x1B,
                        // chain into another PendingEscape instead of toggling.
                        if raw_byte == 0x1B {
                            // Chained Escape — stay in pending state
                            ansi_len = 1;
                        } else {
                            handle_event::<U, D>(CalcEvent::ToggleMode, state);
                            handle_event::<U, D>(translate_input_byte_to_event(raw_byte), state);
                            ansi_state = AnsiSeq::None;
                            ansi_len = 0;
                        }
                    }
                }
                AnsiSeq::PendingBracket => {
                    // Third byte determines the arrow direction.
                    // If it's 0x1B (next Escape in rapid repeats), restart as PendingEscape.
                    ansi_state = AnsiSeq::None;
                    ansi_len = 0;
                    if raw_byte == 0x1B {
                        ansi_state = AnsiSeq::PendingEscape;
                        ansi_buf[0] = 0x1B;
                        ansi_len = 1;
                    } else {
                        let event = match raw_byte {
                            b'A' => CalcEvent::CursorUp,
                            b'B' => CalcEvent::CursorDown,
                            b'D' => CalcEvent::CursorLeft,
                            b'C' => CalcEvent::CursorRight,
                            _ => CalcEvent::Ignored,
                        };
                        handle_event::<U, D>(event, state);
                    }
                }
            }
        } else if ansi_state != AnsiSeq::None {
            // No new byte from UART this cycle — increment idle counter.
            // Flush only after 2 consecutive idle polls, giving the next byte
            // of a multi-byte ANSI sequence (e.g. '[' after 0x1B) time to arrive.
            ansi_idle += 1;
            if ansi_idle >= 2 {
                if ansi_len == 1 {
                    handle_event::<U, D>(CalcEvent::ToggleMode, state);
                }
                ansi_state = AnsiSeq::None;
                ansi_len = 0;
                ansi_idle = 0;
            }
        }
    }
}

fn handle_event<U: Uart, D: Display>(event: CalcEvent, state: &mut CalcState) {
    match event {
        CalcEvent::DigitOrOperator(byte) => handle_input_character::<U, D>(byte, state),
        CalcEvent::KeyboardScancode(byte) => {
            handle_event::<U, D>(translate_input_byte_to_event(byte), state);
        }
        CalcEvent::Submit => handle_expression_submission::<U, D>(state),
        CalcEvent::Backspace => handle_backspace::<U, D>(state),
        CalcEvent::CursorLeft => {
            if state.has_matrix_result() && state.current_input().is_empty() {
                state.scroll_matrix_left();
                render_oled::<D>(state);
            } else {
                handle_cursor_left::<U, D>(state);
            }
        }
        CalcEvent::CursorRight => {
            if state.has_matrix_result() && state.current_input().is_empty() {
                state.scroll_matrix_right();
                render_oled::<D>(state);
            } else {
                handle_cursor_right::<U, D>(state);
            }
        }
        CalcEvent::CursorUp => {
            if state.has_matrix_result() && state.current_input().is_empty() {
                state.scroll_matrix_up();
                render_oled::<D>(state);
            } else if state.current_input().len() > 0 {
                state.move_cursor_left();
                render_oled::<D>(state);
            }
        }

        CalcEvent::CursorDown => {
            if state.has_matrix_result() && state.current_input().is_empty() {
                state.scroll_matrix_down();
                render_oled::<D>(state);
            } else if state.current_input().len() > 0 {
                state.move_cursor_right();
                render_oled::<D>(state);
            }
        }

        CalcEvent::ToggleMode => {
            use state::CalculatorMode as CM;
            let new_mode = match state.active_mode() {
                CM::Standard => CM::Advanced,
                CM::Advanced => CM::Matrix,
                CM::Matrix => CM::Standard,
            };
            let name: &[u8] = match new_mode {
                CM::Standard => b"Standard",
                CM::Advanced => b"Advanced",
                CM::Matrix => b"Matrix",
            };
            state.switch_mode(new_mode);
            state.set_last_result(name);
            U::transmit_bytes(b"\r\n");
            U::transmit_bytes(name);
            U::transmit_bytes(b"\r\n> ");
            render_oled::<D>(state);
        }
        CalcEvent::ToggleAngleMode => {
            state.toggle_angle_mode();
            let name = match state.angle_mode() {
                crate::math::AngleMode::Radians => b"Rad",
                crate::math::AngleMode::Degrees => b"Deg",
            };
            state.set_last_result(name);
            U::transmit_bytes(b"\r\n");
            U::transmit_bytes(name);
            U::transmit_bytes(b"\r\n> ");
            render_oled::<D>(state);
        }
        CalcEvent::Ignored => {}
    }
}

fn handle_input_character<U: Uart, D: Display>(byte: u8, state: &mut CalcState) {
    if state.append_character_to_input(byte) {
        U::transmit_byte(byte);
        state.clear_last_result();
        render_oled::<D>(state);
    }
}

fn handle_backspace<U: Uart, D: Display>(state: &mut CalcState) {
    if state.cursor_position() > 0 && state.current_input().len() > 0 {
        state.remove_last_input_character();
        U::transmit_bytes(b"\x08 \x08");
        render_oled::<D>(state);
    }
}

fn handle_cursor_left<U: Uart, D: Display>(state: &mut CalcState) {
    if state.current_input().len() > 0 {
        state.move_cursor_left();
        render_oled::<D>(state);
    } else if state.has_result() {
        state.scroll_result_left();
        render_oled::<D>(state);
    }
}

fn handle_cursor_right<U: Uart, D: Display>(state: &mut CalcState) {
    if state.current_input().len() > 0 {
        state.move_cursor_right();
        render_oled::<D>(state);
    } else if state.has_result() {
        state.scroll_result_right();
        render_oled::<D>(state);
    }
}

fn handle_expression_submission<U: Uart, D: Display>(state: &mut CalcState) {
    U::transmit_bytes(b"\r\n");

    let expr_len = state.input_length;
    state.expr_scratch[..expr_len].copy_from_slice(&state.input_buffer[..expr_len]);
    let expr_slice = &state.expr_scratch[..expr_len];

    if !expr_slice.iter().any(|&b| b != b' ') {
        state.clear_input();
        state.clear_last_result();
        U::transmit_bytes(b"> ");
        render_oled::<D>(state);
        return;
    }

    // Stack guard: refuse evaluation if stack pointer is too close to .bss.
    // This prevents silent corruption from stack overflow into static RAM.
    const BSS_END: usize = 0x2000_13E0;
    const STACK_MARGIN: usize = 128;
    let sp: usize;
    unsafe { core::arch::asm!("mov {}, sp", out(reg) sp) };
    if sp <= BSS_END + STACK_MARGIN {
        U::transmit_bytes(b"! large\r\n");
        state.set_last_result(b"large");
        state.clear_input();
        U::transmit_bytes(b"> ");
        render_oled_result::<D>(state);
        return;
    }

    let result = {
        let variables = &mut state.variables as *mut _;
        let lex_scratch = &mut state.lex_scratch as *mut _;
        let parse_scratch = &mut state.parse_scratch as *mut _;
        unsafe {
            engine::evaluate_expression(
                expr_slice,
                &mut *variables,
                &mut *lex_scratch,
                &mut *parse_scratch,
                state.math_mode(),
                state.angle_mode(),
            )
        }
    };

    let mut result_line = [0u8; 48];
    let mut uart_buf = [0u8; 192];
    match result {
        EvalResult::Matrix(mat) => {
            use crate::math::matrix::MatrixKind;
            U::transmit_bytes(b"= ");
            match mat.kind {
                MatrixKind::Scalar | MatrixKind::Complex => {
                    let s = engine::format_result(&mat, state.math_mode(), &mut result_line);
                    U::transmit_bytes(s);
                    U::transmit_bytes(b"\r\n");
                    state.set_last_result(s);
                    if let Some(c) = mat.to_complex() {
                        state.variables.write_ans(c);
                    }
                }
                MatrixKind::Mat => {
                    let u = engine::format_result_uart(&mat, &mut uart_buf);
                    U::transmit_bytes(u);
                    U::transmit_bytes(b"\r\n");
                    let dims = [b'0' + mat.rows, b'x', b'0' + mat.cols];
                    state.set_last_result(&dims);
                    state.variables.write_matrix_ans(mat);
                }
            }
        }
        EvalResult::Overflow {
            mantissa,
            exponent,
            negative,
        } => {
            U::transmit_bytes(b"= ");
            match engine::format_overflow(mantissa, exponent, negative, &mut result_line) {
                Some(formatted) => {
                    U::transmit_bytes(formatted);
                    U::transmit_bytes(b"\r\n");
                    state.set_last_result(formatted);
                }
                None => {
                    U::transmit_bytes(b"! overflow\r\n");
                    state.set_last_result(b"overflow");
                }
            }
        }
        EvalResult::DomainError => {
            U::transmit_bytes(b"! error\r\n");
            state.set_last_result(b"error");
        }
    };

    state.clear_input();
    U::transmit_bytes(b"> ");
    render_oled_result::<D>(state);
}

fn render_oled<D: Display>(state: &CalcState) {
    let mut framebuffer = D::new_buffer();
    // Show matrix result only when no input is being composed
    if state.current_input().is_empty() {
        if let Some(ref mat) = state.get_matrix_result() {
            let mut grid = crate::runtime::state::DisplayGrid::empty();
            crate::ui::matrix_display::build_grid(mat, &mut grid);
            crate::ui::matrix_display::render_matrix::<D>(
                &mut framebuffer,
                &grid,
                state.matrix_scroll_offset(),
                state.matrix_col_offset(),
            );
            D::render(&framebuffer);
            return;
        }
    }
    let result: Option<&[u8]> = if state.has_result() {
        Some(state.last_result())
    } else {
        None
    };
    formula::render_screen::<D>(
        &mut framebuffer,
        state.current_input(),
        state.cursor_position(),
        result,
        state.result_scroll_offset(),
    );
    D::render(&framebuffer);
}

fn render_oled_result<D: Display>(state: &CalcState) {
    let mut framebuffer = D::new_buffer();
    if let Some(ref mat) = state.get_matrix_result() {
        let mut grid = crate::runtime::state::DisplayGrid::empty();
        crate::ui::matrix_display::build_grid(mat, &mut grid);
        crate::ui::matrix_display::render_matrix::<D>(
            &mut framebuffer,
            &grid,
            state.matrix_scroll_offset(),
            state.matrix_col_offset(),
        );
    } else {
        let result = if state.has_result() {
            Some(state.last_result())
        } else {
            None
        };
        formula::render_screen::<D>(&mut framebuffer, b"", 0, result, 0);
    }
    D::render(&framebuffer);
}
