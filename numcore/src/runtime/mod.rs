pub mod event;
pub mod state;

use crate::hal::{Display, Uart};
use crate::math::engine;
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
    U::transmit_bytes(b"\r\n");
    U::transmit_bytes(b"===========================================\r\n");
    U::transmit_bytes(b"  NumCore v0.4\r\n");
    U::transmit_bytes(b"  LM3S811  Cortex-M3  (Rust)\r\n");
    U::transmit_bytes(b"  Q31.32 fixed-point  |  PEMDAS\r\n");
    U::transmit_bytes(b"===========================================\r\n");
    U::transmit_bytes(b"  Ops : + - * / ^ %\r\n");
    U::transmit_bytes(b"  Fns : sin cos tan asin acos atan\r\n");
    U::transmit_bytes(b"        sinh cosh tanh asinh acosh atanh\r\n");
    U::transmit_bytes(b"        sqrt abs exp log ln log2\r\n");
    U::transmit_bytes(b"        floor ceil round deg rad\r\n");
    U::transmit_bytes(b"        nthroot binomp poissonp chicdf sum int\r\n");
    U::transmit_bytes(b"  Const: pi  e\r\n");
    U::transmit_bytes(b"  Vars : Ans  A B C D E F G H I J K L M\r\n");
    U::transmit_bytes(b"         N O P Q R S T U V W X Y Z\r\n");
    U::transmit_bytes(b"  Cmd  : sto(value, var)\r\n");
    U::transmit_bytes(b"===========================================\r\n\r\n");
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
                        // 0x1B followed by non-[ → standalone Escape,
                        // then process the current byte normally.
                        handle_event::<U, D>(CalcEvent::ToggleMode, state);
                        handle_event::<U, D>(translate_input_byte_to_event(raw_byte), state);
                        ansi_state = AnsiSeq::None;
                        ansi_len = 0;
                    }
                }
                AnsiSeq::PendingBracket => {
                    // Third byte determines the arrow direction.
                    ansi_state = AnsiSeq::None;
                    ansi_len = 0;
                    let event = match raw_byte {
                        b'D' => CalcEvent::CursorLeft,
                        b'C' => CalcEvent::CursorRight,
                        _ => CalcEvent::Ignored,
                    };
                    handle_event::<U, D>(event, state);
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
        CalcEvent::CursorLeft => handle_cursor_left::<U, D>(state),
        CalcEvent::CursorRight => handle_cursor_right::<U, D>(state),
        CalcEvent::ToggleMode => {
            let new_mode = match state.active_mode() {
                state::CalculatorMode::Standard => state::CalculatorMode::Advanced,
                state::CalculatorMode::Advanced => state::CalculatorMode::Standard,
            };
            let name = match new_mode {
                state::CalculatorMode::Standard => b"Standard",
                state::CalculatorMode::Advanced => b"Advanced",
            };
            state.switch_mode(new_mode);
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

    let mut expr_copy = [0u8; 64];
    let expr_len = {
        let expression = state.current_input();
        let len = expression.len();
        expr_copy[..len].copy_from_slice(expression);
        len
    };
    let expr_slice = &expr_copy[..expr_len];

    if !expr_slice.iter().any(|&b| b != b' ') {
        state.clear_input();
        state.clear_last_result();
        U::transmit_bytes(b"> ");
        render_oled::<D>(state);
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
            )
        }
    };

    let mut result_line = [0u8; 48];
    let mut display_copy = [0u8; 48];
    let display_len = match result {
        Some(result) => {
            state.record_answer(result);

            U::transmit_bytes(b"= ");
            let formatted = engine::format_result(result, state.math_mode(), &mut result_line);
            U::transmit_bytes(formatted);
            U::transmit_bytes(b"\r\n");
            state.set_last_result(formatted);
            let len = state.last_result().len().min(48);
            display_copy[..len].copy_from_slice(&state.last_result()[..len]);
            len
        }
        None => {
            U::transmit_bytes(
                b"! error: invalid expression, domain error, or division by zero\r\n",
            );
            state.clear_last_result();
            const ERROR: &[u8] = b"error";
            let len = ERROR.len().min(48);
            display_copy[..len].copy_from_slice(&ERROR[..len]);
            len
        }
    };

    state.clear_input();
    U::transmit_bytes(b"> ");
    render_oled_result::<D>(&display_copy[..display_len]);
}

fn render_oled<D: Display>(state: &CalcState) {
    let mut framebuffer = D::new_buffer();
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

fn render_oled_result<D: Display>(result_text: &[u8]) {
    let mut framebuffer = D::new_buffer();
    formula::render_screen::<D>(&mut framebuffer, b"", 0, Some(result_text), 0);
    D::render(&framebuffer);
}
