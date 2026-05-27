//! # Event Translator (Layer 5 — Runtime)
//!
//! Translates raw input bytes (from the HAL's UART receive function) into
//! typed, meaningful `CalcEvent` values that the rest of the runtime can
//! reason about without knowing anything about ASCII codes or terminal escapes.
//!
//! This is the boundary between "bytes from hardware" and "intent from user".
//! Adding support for a physical keypad later means only changing this file
//! and the HAL — the event loop and handlers remain unchanged.

// ─── CalcEvent ────────────────────────────────────────────────────────────────

/// A meaningful input event in the calculator's domain.
///
/// The runtime event loop receives one of these per input cycle and dispatches
/// it to the appropriate handler. Raw bytes never escape this module.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CalcEvent {
    /// A printable character (digit, operator, decimal point, parenthesis, etc.)
    /// The contained byte is the raw ASCII value — safe to echo and store.
    DigitOrOperator(u8),

    /// A byte produced by the PS/2 keyboard path.
    ///
    /// The HAL currently translates PS/2 make-code sequences into the same
    /// ASCII/control-byte convention used by UART before handing them to the
    /// runtime.
    KeyboardScancode(u8),

    /// The user pressed Enter (CR or LF) — submit the current expression.
    Submit,

    /// The user pressed Backspace or Delete — remove the last character.
    Backspace,

    /// Move the cursor one position to the left (in input or result scroll).
    CursorLeft,

    /// Move the cursor one position to the right.
    CursorRight,

    /// Toggle between Standard and Advanced calculator modes.
    ToggleMode,

    /// Toggle between Radians and Degrees for trig functions.
    ToggleAngleMode,

    /// A byte with no meaning in the current context (null, escape, etc.).
    /// The event loop discards these without taking any action.
    Ignored,
}

// ─── Translation ──────────────────────────────────────────────────────────────

/// Translate a raw byte received from the input peripheral into a `CalcEvent`.
///
/// This function handles single-byte inputs only. Multi-byte ANSI escape
/// sequences (arrow keys) are parsed by the runtime event loop before
/// reaching this function.
///
/// # Byte classification
///   0x08, 0x7F        → Backspace (BS and DEL both erase the last character)
///   0x0D, 0x0A        → Submit (carriage return and line feed both confirm input)
///   0x20 – 0x7E       → DigitOrOperator (the full printable ASCII range)
///   0x1B              → ToggleMode (standalone Escape — ANSI sequences
///                        are intercepted before reaching this function)
///   everything else   → Ignored
pub fn translate_input_byte_to_event(raw_byte: u8) -> CalcEvent {
    match raw_byte {
        b'\r' | b'\n' => CalcEvent::Submit,
        0x08 | 0x7F => CalcEvent::Backspace,
        0x04 => CalcEvent::ToggleAngleMode,
        0x20..=0x7E => CalcEvent::DigitOrOperator(raw_byte),
        0x1B => CalcEvent::ToggleMode,
        _ => CalcEvent::Ignored,
    }
}
