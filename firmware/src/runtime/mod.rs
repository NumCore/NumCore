//! # Core Runtime — Firmware Kernel (Layer 5)
//!
//! The control centre of the firmware. Sits between HAL (Layer 4) and the
//! application layers (math, UI, modes). Touches hardware ONLY through HAL calls.
//!
//! ## Responsibilities
//!   - Initialise all HAL peripherals in the correct order
//!   - Own the main event loop
//!   - Own and update CalcState (including the variable store)
//!   - Route input events to handlers
//!   - Trigger UI re-renders after state changes

pub mod event;
pub mod state;

use crate::math::engine;
use crate::ui::formula;
use event::{translate_input_byte_to_event, CalcEvent};
use hal::{i2c, oled, uart};
use state::CalcState;

static mut CALCULATOR_STATE: CalcState = CalcState::new();

// ─── Firmware entry point ─────────────────────────────────────────────────────

/// Called by `boot::Reset` after memory init. Never returns.
pub fn start() -> ! {
    initialise_all_hardware();
    print_welcome_banner();

    // Safety: single-threaded firmware. The runtime takes one exclusive
    // reference to the global calculator state and never returns.
    let calculator_state = unsafe { &mut *(&raw mut CALCULATOR_STATE) };

    // Print the initial prompt once.
    uart::transmit_bytes(b"> ");
    render_oled(calculator_state.current_input(), Some(b"ready"));

    run_event_loop(calculator_state)
}

/// Initialise every peripheral the firmware uses.
/// Add new HAL init calls here as peripherals are enabled.
fn initialise_all_hardware() {
    uart::initialise_uart();
    // ps2::initialise_ps2();
    i2c::initialise_i2c();
    oled::initialise_oled();
    oled::clear_display();
}

fn print_welcome_banner() {
    uart::transmit_bytes(b"\r\n");
    uart::transmit_bytes(b"===========================================\r\n");
    uart::transmit_bytes(b"  NumCore v0.4\r\n");
    uart::transmit_bytes(b"  LM3S811  Cortex-M3  (Rust)\r\n");
    uart::transmit_bytes(b"  Q31.32 fixed-point  |  PEMDAS\r\n");
    uart::transmit_bytes(b"===========================================\r\n");
    uart::transmit_bytes(b"  Ops : + - * / ^ %\r\n");
    uart::transmit_bytes(b"  Fns : sin cos tan asin acos atan\r\n");
    uart::transmit_bytes(b"        sinh cosh tanh asinh acosh atanh\r\n");
    uart::transmit_bytes(b"        sqrt abs exp log ln log2\r\n");
    uart::transmit_bytes(b"        floor ceil round deg rad\r\n");
    uart::transmit_bytes(b"        nthroot binomp poissonp chicdf sum int\r\n");
    uart::transmit_bytes(b"  Const: pi  e\r\n");
    uart::transmit_bytes(b"  Vars : Ans  A B C D E F G H I J K L M\r\n");
    uart::transmit_bytes(b"         N O P Q R S T U V W X Y Z\r\n");
    uart::transmit_bytes(b"  Cmd  : sto(value, var)\r\n");
    uart::transmit_bytes(b"===========================================\r\n\r\n");
}

// ─── Main event loop ──────────────────────────────────────────────────────────

/// Block on input, translate to events, dispatch handlers. Runs forever.
fn run_event_loop(calculator_state: &mut CalcState) -> ! {
    loop {
        if let Some(raw_byte) = uart::poll_byte() {
            let event = translate_input_byte_to_event(raw_byte);
            handle_event(event, calculator_state);
        }

        // if let Some(scan_code) = ps2::poll_key() {
        //     handle_event(CalcEvent::KeyboardScancode(scan_code), calculator_state);
        // }
    }
}

/// Dispatch a CalcEvent to the appropriate handler.
fn handle_event(event: CalcEvent, state: &mut CalcState) {
    match event {
        CalcEvent::DigitOrOperator(byte) => handle_input_character(byte, state),
        CalcEvent::KeyboardScancode(byte) => {
            handle_event(translate_input_byte_to_event(byte), state);
        }
        CalcEvent::Submit => handle_expression_submission(state),
        CalcEvent::Backspace => handle_backspace(state),
        CalcEvent::Ignored => {}
    }
}

// ─── Event handlers ───────────────────────────────────────────────────────────

/// Append a printable character and echo it.
fn handle_input_character(byte: u8, state: &mut CalcState) {
    if state.append_character_to_input(byte) {
        uart::transmit_byte(byte);
        render_oled(state.current_input(), None);
    }
    // If the buffer is full: silently discard — do NOT echo a character that
    // wasn't stored, as that would make the terminal display lie.
}

/// Remove the last character from the buffer and erase it on the terminal.
fn handle_backspace(state: &mut CalcState) {
    if state.remove_last_input_character() {
        // VT100 backspace: move back one, write space, move back again.
        uart::transmit_bytes(b"\x08 \x08");
        render_oled(state.current_input(), None);
    }
}

/// Evaluate the current expression and display the result.
fn handle_expression_submission(state: &mut CalcState) {
    uart::transmit_bytes(b"\r\n");

    // Copy expression before releasing the borrow on state.
    let mut expr_copy = [0u8; 64];
    let expr_len = {
        let expression = state.current_input();
        let len = expression.len();
        expr_copy[..len].copy_from_slice(expression);
        len
    };
    let expr_slice = &expr_copy[..expr_len];

    // Ignore blank or whitespace-only submissions.
    if !expr_slice.iter().any(|&b| b != b' ') {
        state.clear_input();
        uart::transmit_bytes(b"> ");
        render_oled(state.current_input(), None);
        return;
    }

    // Access disjoint fields of CalcState directly via raw field borrows.
    // The borrow checker cannot prove these are disjoint through method calls,
    // so we borrow the fields directly. This is safe — each borrow touches a
    // completely separate region of the CalcState struct.
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
            )
        }
    };

    let mut result_line = [0u8; 24];
    let result_text = match result {
        Some(result) => {
            // Write result to Ans.
            state.record_answer(result);

            uart::transmit_bytes(b"= ");
            let formatted = engine::format_result(result, &mut result_line);
            uart::transmit_bytes(formatted);
            uart::transmit_bytes(b"\r\n");
            formatted
        }
        None => {
            uart::transmit_bytes(
                b"! error: invalid expression, domain error, or division by zero\r\n",
            );
            b"error"
        }
    };

    render_oled(expr_slice, Some(result_text));
    state.clear_input();
    uart::transmit_bytes(b"> ");
}

// ─── OLED rendering ──────────────────────────────────────────────────────────

/// Render the current input and optional result to the 96×16 OLED.
fn render_oled(expression: &[u8], result_text: Option<&[u8]>) {
    let mut framebuffer: oled::Framebuffer = [0u8; oled::FRAMEBUFFER_SIZE];
    formula::render_screen(&mut framebuffer, expression, result_text);
    oled::render_screen(&framebuffer);
}
