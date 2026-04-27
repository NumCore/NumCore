//! # Calculator State (Layer 5 — Runtime)
//!
//! Owns the complete runtime state of the calculator. The state struct is the
//! single source of truth for:
//!   - The current expression being typed (input buffer)
//!   - The last computed answer (Ans variable)
//!   - The active calculator mode (standard, scientific, etc.)
//!
//! No hardware knowledge lives here. No math logic lives here. This is purely
//! a data container with safe, intention-revealing mutator methods.

// ─── Calculator modes ─────────────────────────────────────────────────────────

/// The operating mode the calculator is currently in.
///
/// Each mode defines different available functions, UI layout, and input
/// behaviour. Add new variants here as modes are designed.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CalculatorMode {
    /// Basic four-function arithmetic. The initial mode on boot.
    Standard,

    /// Extended functions: trig, log, powers, roots.
    /// Not yet implemented — placeholder for future expansion.
    Scientific,
}

// ─── Input buffer capacity ────────────────────────────────────────────────────

/// Maximum number of characters in a single input expression.
///
/// 64 bytes fits comfortably in the LM3S811's 8 KB SRAM while leaving plenty
/// of room for the stack and future features. Increase if longer expressions
/// are needed; decrease if memory pressure becomes an issue.
const INPUT_BUFFER_CAPACITY: usize = 64;

// ─── CalcState ────────────────────────────────────────────────────────────────

/// The complete runtime state of the calculator.
///
/// Owned by the runtime event loop. Passed by mutable reference to event
/// handlers. Never accessed directly by the math engine or UI layer — those
/// receive slices or values extracted from this struct.
pub struct CalcState {
    /// The expression the user is currently typing.
    input_buffer: [u8; INPUT_BUFFER_CAPACITY],

    /// Number of valid bytes currently in `input_buffer`.
    input_length: usize,

    /// The result of the most recent successful evaluation.
    /// Accessible to the math engine as the `Ans` variable.
    last_answer: i32,

    /// Whether `last_answer` holds a meaningful value yet.
    /// False on startup and after a clear that had no prior answer.
    has_last_answer: bool,

    /// The calculator mode currently active.
    active_mode: CalculatorMode,
}

impl CalcState {
    /// Create a fresh calculator state, as if just powered on.
    pub fn new() -> Self {
        Self {
            input_buffer: [0u8; INPUT_BUFFER_CAPACITY],
            input_length: 0,
            last_answer: 0,
            has_last_answer: false,
            active_mode: CalculatorMode::Standard,
        }
    }

    /// Attempt to append one character to the input buffer.
    ///
    /// Returns `true` if the character was added, `false` if the buffer is full.
    pub fn append_character_to_input(&mut self, character: u8) -> bool {
        if self.input_length >= INPUT_BUFFER_CAPACITY - 1 {
            return false; // Buffer full — character cannot be stored
        }
        self.input_buffer[self.input_length] = character;
        self.input_length += 1;
        true
    }

    /// Remove the last character from the input buffer.
    ///
    /// Returns `true` if a character was removed, `false` if the buffer was
    /// already empty.
    pub fn remove_last_input_character(&mut self) -> bool {
        if self.input_length == 0 {
            return false; // Nothing to remove
        }
        self.input_length -= 1;
        // Zero the vacated slot so stale data never leaks into a future read.
        self.input_buffer[self.input_length] = 0;
        true
    }

    /// Return an immutable view of the current input expression.
    ///
    /// The slice contains exactly the characters the user has typed, with no
    /// trailing null bytes. Pass this to the math engine for evaluation.
    pub fn current_input(&self) -> &[u8] {
        &self.input_buffer[..self.input_length]
    }

    /// Clear the input buffer, ready for a new expression.
    pub fn clear_input(&mut self) {
        // Zero the used portion of the buffer to prevent stale data leaking.
        for byte in &mut self.input_buffer[..self.input_length] {
            *byte = 0;
        }
        self.input_length = 0;
    }

    /// Store the result of a completed evaluation as the `Ans` variable.
    pub fn set_last_answer(&mut self, answer: i32) {
        self.last_answer = answer;
        self.has_last_answer = true;
    }

    /// Retrieve the last answer, if one exists.
    ///
    /// Returns `None` if no expression has been successfully evaluated yet.
    pub fn last_answer(&self) -> Option<i32> {
        if self.has_last_answer { Some(self.last_answer) } else { None }
    }

    /// Return the currently active calculator mode.
    pub fn active_mode(&self) -> CalculatorMode {
        self.active_mode
    }

    /// Switch to a different calculator mode.
    ///
    /// Clears the input buffer when switching — an expression typed in one
    /// mode should not be carried into another where it may be invalid.
    pub fn switch_mode(&mut self, new_mode: CalculatorMode) {
        self.active_mode = new_mode;
        self.clear_input();
    }
}
