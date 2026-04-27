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

    /// The user pressed Enter (CR or LF) — submit the current expression.
    Submit,

    /// The user pressed Backspace or Delete — remove the last character.
    Backspace,

    /// A byte with no meaning in the current context (null, escape, etc.).
    /// The event loop discards these without taking any action.
    Ignored,
}

// ─── Translation ──────────────────────────────────────────────────────────────

/// Translate a raw byte received from the input peripheral into a `CalcEvent`.
///
/// This function encodes all knowledge of terminal conventions and ASCII
/// control codes. Nothing outside this module needs to know what `0x08` means.
///
/// # Byte classification
///   0x08, 0x7F        → Backspace (BS and DEL both erase the last character)
///   0x0D, 0x0A        → Submit (carriage return and line feed both confirm input)
///   0x20 – 0x7E       → DigitOrOperator (the full printable ASCII range)
///   everything else   → Ignored
pub fn translate_input_byte_to_event(raw_byte: u8) -> CalcEvent {
    match raw_byte {
        // Carriage return (Enter on most terminals) and line feed.
        b'\r' | b'\n' => CalcEvent::Submit,

        // Backspace (0x08) and Delete (0x7F) both remove the previous character.
        0x08 | 0x7F => CalcEvent::Backspace,

        // Printable ASCII: digits, operators, letters, punctuation.
        // The math engine will validate whether the content is a legal expression.
        0x20..=0x7E => CalcEvent::DigitOrOperator(raw_byte),

        // Everything else: null bytes, escape sequences, function keys, etc.
        _ => CalcEvent::Ignored,
    }
}
