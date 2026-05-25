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
    render_oled::<D>(calculator_state.current_input(), Some(b"ready"));

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

fn run_event_loop<U: Uart, D: Display>(calculator_state: &mut CalcState) -> ! {
    loop {
        if let Some(raw_byte) = U::poll_byte() {
            let event = translate_input_byte_to_event(raw_byte);
            handle_event::<U, D>(event, calculator_state);
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
        CalcEvent::Ignored => {}
    }
}

fn handle_input_character<U: Uart, D: Display>(byte: u8, state: &mut CalcState) {
    if state.append_character_to_input(byte) {
        U::transmit_byte(byte);
        render_oled::<D>(state.current_input(), None);
    }
}

fn handle_backspace<U: Uart, D: Display>(state: &mut CalcState) {
    if state.remove_last_input_character() {
        U::transmit_bytes(b"\x08 \x08");
        render_oled::<D>(state.current_input(), None);
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
        U::transmit_bytes(b"> ");
        render_oled::<D>(state.current_input(), None);
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
            )
        }
    };

    let mut result_line = [0u8; 24];
    let result_text = match result {
        Some(result) => {
            state.record_answer(result);

            U::transmit_bytes(b"= ");
            let formatted = engine::format_result(result, &mut result_line);
            U::transmit_bytes(formatted);
            U::transmit_bytes(b"\r\n");
            formatted
        }
        None => {
            U::transmit_bytes(
                b"! error: invalid expression, domain error, or division by zero\r\n",
            );
            b"error"
        }
    };

    render_oled::<D>(expr_slice, Some(result_text));
    state.clear_input();
    U::transmit_bytes(b"> ");
}

fn render_oled<D: Display>(expression: &[u8], result_text: Option<&[u8]>) {
    let mut framebuffer = D::new_buffer();
    formula::render_screen::<D>(&mut framebuffer, expression, result_text);
    D::render(&framebuffer);
}
