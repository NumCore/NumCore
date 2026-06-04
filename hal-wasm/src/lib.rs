use std::cell::RefCell;
use std::collections::VecDeque;

use wasm_bindgen::prelude::*;

use numcore::hal::{Display, Uart};
use numcore::math::engine::{self, EvalResult};
use numcore::runtime::event::{translate_input_byte_to_event, CalcEvent};
use numcore::runtime::state::CalcState;
use numcore::ui::formula;
use numcore::ui::matrix_display;

thread_local! {
    static INPUT_QUEUE: RefCell<VecDeque<u8>> = const { RefCell::new(VecDeque::new()) };
    static SERIAL_OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static FRAMEBUFFER: RefCell<[u8; 192]> = const { RefCell::new([0u8; 192]) };
}

pub struct WasmDisplay;

impl Display for WasmDisplay {
    type Buffer = [u8; 192];
    const WIDTH: usize = 96;
    const HEIGHT: usize = 16;

    fn init() {}

    fn new_buffer() -> Self::Buffer {
        [0u8; 192]
    }

    fn render(fb: &Self::Buffer) {
        FRAMEBUFFER.with(|f| f.borrow_mut().copy_from_slice(fb));
    }

    fn set_pixel(fb: &mut Self::Buffer, col: usize, row: usize, on: bool) {
        if col >= Self::WIDTH || row >= Self::HEIGHT {
            return;
        }
        let byte_idx = (row / 8) * Self::WIDTH + col;
        let bit = row % 8;
        if on {
            fb[byte_idx] |= 1 << bit;
        } else {
            fb[byte_idx] &= !(1 << bit);
        }
    }
}

pub struct WasmUart;

impl Uart for WasmUart {
    fn init() {}

    fn transmit_bytes(bytes: &[u8]) {
        SERIAL_OUTPUT.with(|out| out.borrow_mut().extend_from_slice(bytes));
    }

    fn transmit_byte(byte: u8) {
        SERIAL_OUTPUT.with(|out| out.borrow_mut().push(byte));
    }

    fn poll_byte() -> Option<u8> {
        INPUT_QUEUE.with(|q| q.borrow_mut().pop_front())
    }
}

static mut CALC_STATE: CalcState = CalcState::new();

fn with_state<F, R>(f: F) -> R
where
    F: FnOnce(&mut CalcState) -> R,
{
    unsafe { f(&mut CALC_STATE) }
}

fn render_display(state: &CalcState) {
    let mut fb = WasmDisplay::new_buffer();
    if state.current_input().is_empty() {
        if let Some(ref mat) = state.get_matrix_result() {
            let mut grid = numcore::runtime::state::DisplayGrid::empty();
            matrix_display::build_grid(mat, &mut grid);
            matrix_display::render_matrix::<WasmDisplay>(
                &mut fb,
                &grid,
                state.matrix_scroll_offset(),
                state.matrix_col_offset(),
            );
            WasmDisplay::render(&fb);
            return;
        }
    }
    let result: Option<&[u8]> = if state.has_result() {
        Some(state.last_result())
    } else {
        None
    };
    formula::render_screen::<WasmDisplay>(
        &mut fb,
        state.current_input(),
        state.cursor_position(),
        result,
        state.result_scroll_offset(),
    );
    WasmDisplay::render(&fb);
}

fn render_result_display(state: &CalcState) {
    let mut fb = WasmDisplay::new_buffer();
    if let Some(ref mat) = state.get_matrix_result() {
        let mut grid = numcore::runtime::state::DisplayGrid::empty();
        matrix_display::build_grid(mat, &mut grid);
        matrix_display::render_matrix::<WasmDisplay>(
            &mut fb,
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
        formula::render_screen::<WasmDisplay>(&mut fb, b"", 0, result, 0);
    }
    WasmDisplay::render(&fb);
}

fn handle_event(state: &mut CalcState, event: CalcEvent) {
    use CalcEvent::*;
    match event {
        DigitOrOperator(byte) => {
            if state.append_character_to_input(byte) {
                WasmUart::transmit_byte(byte);
                state.clear_last_result();
                render_display(state);
            }
        }
        Submit => {
            WasmUart::transmit_bytes(b"\r\n");
            handle_submit(state);
        }
        Backspace => {
            if state.cursor_position() > 0 && !state.current_input().is_empty() {
                state.remove_last_input_character();
                WasmUart::transmit_bytes(b"\x08 \x08");
                render_display(state);
            }
        }
        CursorLeft => {
            if state.has_matrix_result() && state.current_input().is_empty() {
                state.scroll_matrix_left();
                render_display(state);
            } else if state.current_input().len() > 0 {
                state.move_cursor_left();
                render_display(state);
            } else if state.has_result() {
                state.scroll_result_left();
                render_display(state);
            }
        }
        CursorRight => {
            if state.has_matrix_result() && state.current_input().is_empty() {
                state.scroll_matrix_right();
                render_display(state);
            } else if state.current_input().len() > 0 {
                state.move_cursor_right();
                render_display(state);
            } else if state.has_result() {
                state.scroll_result_right();
                render_display(state);
            }
        }
        CursorUp => {
            if state.has_matrix_result() && state.current_input().is_empty() {
                state.scroll_matrix_up();
                render_display(state);
            } else if state.current_input().len() > 0 {
                state.move_cursor_left();
                render_display(state);
            }
        }
        CursorDown => {
            if state.has_matrix_result() && state.current_input().is_empty() {
                state.scroll_matrix_down();
                render_display(state);
            } else if state.current_input().len() > 0 {
                state.move_cursor_right();
                render_display(state);
            }
        }
        ToggleMode => {
            use numcore::runtime::state::CalculatorMode as CM;
            let new_mode = match state.active_mode() {
                CM::Standard => CM::Advanced,
                CM::Advanced => CM::Matrix,
                CM::Matrix => CM::Scientific,
                CM::Scientific => CM::Standard,
            };
            let name: &[u8] = match new_mode {
                CM::Standard => b"Standard",
                CM::Advanced => b"Advanced",
                CM::Matrix => b"Matrix",
                CM::Scientific => b"Scientific",
            };
            state.switch_mode(new_mode);
            state.set_last_result(name);
            WasmUart::transmit_bytes(b"\r\n");
            WasmUart::transmit_bytes(name);
            WasmUart::transmit_bytes(b"\r\n> ");
            render_display(state);
        }
        ToggleAngleMode => {
            state.toggle_angle_mode();
            let name = match state.angle_mode() {
                numcore::math::AngleMode::Radians => b"Rad",
                numcore::math::AngleMode::Degrees => b"Deg",
            };
            state.set_last_result(name);
            WasmUart::transmit_bytes(b"\r\n");
            WasmUart::transmit_bytes(name);
            WasmUart::transmit_bytes(b"\r\n> ");
            render_display(state);
        }
        Ignored | KeyboardScancode(_) => {}
    }
}

fn handle_submit(state: &mut CalcState) {
    let input = state.current_input().to_vec();
    let expr_len = input.len();
    state.expr_scratch[..expr_len].copy_from_slice(&input);
    let expr_slice = &state.expr_scratch[..expr_len];

    if expr_slice.iter().all(|&b| b == b' ') {
        state.clear_input();
        state.clear_last_result();
        WasmUart::transmit_bytes(b"> ");
        render_display(state);
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
    match result {
        EvalResult::Matrix(mat) => {
            use numcore::math::matrix::MatrixKind;
            WasmUart::transmit_bytes(b"= ");
            match mat.kind {
                MatrixKind::Scalar | MatrixKind::Complex => {
                    let s = engine::format_result(&mat, state.math_mode(), &mut result_line);
                    WasmUart::transmit_bytes(s);
                    WasmUart::transmit_bytes(b"\r\n");
                    state.set_last_result(s);
                    if let Some(c) = mat.to_complex() {
                        state.variables.write_ans(c);
                    }
                }
                MatrixKind::Scientific => {
                    if mat.data[1] > 99 || mat.data[1] < -99 {
                        WasmUart::transmit_bytes(b"! overflow\r\n");
                        state.set_last_result(b"overflow");
                    } else {
                        let s = engine::format_result(&mat, state.math_mode(), &mut result_line);
                        WasmUart::transmit_bytes(s);
                        WasmUart::transmit_bytes(b"\r\n");
                        state.set_last_result(s);
                    }
                }
                MatrixKind::Mat => {
                    let dims = [b'0' + mat.rows, b'x', b'0' + mat.cols];
                    WasmUart::transmit_bytes(&dims);
                    WasmUart::transmit_bytes(b"\r\n");
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
            WasmUart::transmit_bytes(b"= ");
            match engine::format_overflow(mantissa, exponent, negative, &mut result_line) {
                Some(formatted) => {
                    WasmUart::transmit_bytes(formatted);
                    WasmUart::transmit_bytes(b"\r\n");
                    state.set_last_result(formatted);
                }
                None => {
                    WasmUart::transmit_bytes(b"! overflow\r\n");
                    state.set_last_result(b"overflow");
                }
            }
        }
        EvalResult::DomainError => {
            WasmUart::transmit_bytes(b"! error\r\n");
            state.set_last_result(b"error");
        }
    }

    state.clear_input();
    WasmUart::transmit_bytes(b"> ");
    render_result_display(state);
}

#[wasm_bindgen]
pub fn init() {
    unsafe {
        CALC_STATE = CalcState::new();
    }
    INPUT_QUEUE.with(|q| q.borrow_mut().clear());
    SERIAL_OUTPUT.with(|out| out.borrow_mut().clear());
    FRAMEBUFFER.with(|f| *f.borrow_mut() = [0u8; 192]);

    WasmUart::transmit_bytes(b"> ");
    with_state(|s| s.set_last_result(b"ready"));
    with_state(|s| render_display(s));
}

#[wasm_bindgen]
pub fn feed_input_byte(byte: u8) {
    INPUT_QUEUE.with(|q| q.borrow_mut().push_back(byte));
}

#[wasm_bindgen]
pub fn tick() {
    while let Some(byte) = WasmUart::poll_byte() {
        let event = translate_input_byte_to_event(byte);
        with_state(|s| handle_event(s, event));
    }
}

#[wasm_bindgen]
pub fn feed_cursor_key(direction: u8) {
    let event = match direction {
        0 => CalcEvent::CursorUp,
        1 => CalcEvent::CursorDown,
        2 => CalcEvent::CursorLeft,
        3 => CalcEvent::CursorRight,
        _ => return,
    };
    with_state(|s| handle_event(s, event));
}

#[wasm_bindgen]
pub fn feed_toggle_mode() {
    with_state(|s| handle_event(s, CalcEvent::ToggleMode));
}

#[wasm_bindgen]
pub fn feed_toggle_angle() {
    with_state(|s| handle_event(s, CalcEvent::ToggleAngleMode));
}

#[wasm_bindgen]
pub fn get_framebuffer() -> Vec<u8> {
    FRAMEBUFFER.with(|f| f.borrow().to_vec())
}

#[wasm_bindgen]
pub fn get_framebuffer_ptr() -> *const u8 {
    FRAMEBUFFER.with(|f| f.borrow().as_ptr())
}

#[wasm_bindgen]
pub fn get_mode() -> u8 {
    with_state(|s| match s.active_mode() {
        numcore::runtime::state::CalculatorMode::Standard => 0,
        numcore::runtime::state::CalculatorMode::Advanced => 1,
        numcore::runtime::state::CalculatorMode::Matrix => 2,
        numcore::runtime::state::CalculatorMode::Scientific => 3,
    })
}

#[wasm_bindgen]
pub fn get_serial_output() -> String {
    SERIAL_OUTPUT.with(|out| {
        let bytes = out.borrow().clone();
        out.borrow_mut().clear();
        String::from_utf8_lossy(&bytes).to_string()
    })
}
