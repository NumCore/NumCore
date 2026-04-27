//! # Variable Store (Math Engine — Layer 6)
//!
//! Stores named calculator variables as Q20.12 fixed-point values.
//!
//! ## Supported variables
//!   Ans   — the result of the most recent successful evaluation
//!   A–F   — six user-assignable storage registers (Casio-style)
//!
//! ## Usage
//!   Variables are read by the evaluator during expression evaluation.
//!   They are written by the runtime after a successful evaluation (Ans)
//!   or by an explicit assignment expression `A=expression` (A–F, future).
//!
//! ## Design
//!   The `VariableStore` is owned by `CalcState` (Layer 5) and passed as
//!   an immutable reference into the math engine. The math engine never
//!   writes variables — that is the runtime's responsibility, maintaining
//!   the rule that the math engine has no side effects.

/// Number of user-assignable registers (A through F).
const USER_REGISTER_COUNT: usize = 6;

/// Maps a register letter ('A'–'F') to an index into the user register array.
const FIRST_REGISTER_LETTER: u8 = b'A';

/// Stores all calculator variables as Q20.12 fixed-point values.
#[derive(Clone, Copy)]
pub struct VariableStore {
    /// The last successfully evaluated result (the `Ans` variable).
    last_answer: i32,

    /// Whether `last_answer` has ever been set (false on fresh boot).
    has_last_answer: bool,

    /// User registers A through F. Index 0 = A, index 5 = F.
    user_registers: [i32; USER_REGISTER_COUNT],

    /// Which user registers have been explicitly assigned.
    /// Unassigned registers evaluate to 0 with no error.
    register_assigned: [bool; USER_REGISTER_COUNT],
}

impl VariableStore {
    /// Create an empty variable store — all registers zero, Ans undefined.
    pub fn new() -> Self {
        Self {
            last_answer: 0,
            has_last_answer: false,
            user_registers: [0i32; USER_REGISTER_COUNT],
            register_assigned: [false; USER_REGISTER_COUNT],
        }
    }

    /// Read the `Ans` variable (last answer).
    ///
    /// Returns `Some(value)` if an answer has been computed, `None` on fresh boot.
    /// The value is a Q20.12 fixed-point number.
    pub fn read_ans(&self) -> Option<i32> {
        if self.has_last_answer { Some(self.last_answer) } else { None }
    }

    /// Write the `Ans` variable. Called by the runtime after every evaluation.
    pub fn write_ans(&mut self, value: i32) {
        self.last_answer = value;
        self.has_last_answer = true;
    }

    /// Read a user register by letter ('A'–'F').
    ///
    /// Returns the stored Q20.12 value, or 0 if the register was never assigned.
    /// Returns `None` if the letter is outside 'A'–'F'.
    pub fn read_register(&self, letter: u8) -> Option<i32> {
        let index = register_letter_to_index(letter)?;
        Some(self.user_registers[index])
    }

    /// Write a user register by letter ('A'–'F').
    ///
    /// Returns `false` if the letter is outside 'A'–'F'.
    pub fn write_register(&mut self, letter: u8, value: i32) -> bool {
        if let Some(index) = register_letter_to_index(letter) {
            self.user_registers[index] = value;
            self.register_assigned[index] = true;
            true
        } else {
            false
        }
    }

    /// Check whether a user register has ever been explicitly assigned.
    pub fn is_register_assigned(&self, letter: u8) -> bool {
        register_letter_to_index(letter)
            .map(|i| self.register_assigned[i])
            .unwrap_or(false)
    }
}

/// Convert a register letter ('A'–'F', case-insensitive) to a 0-based index.
/// Returns None if the letter is not in the valid range.
fn register_letter_to_index(letter: u8) -> Option<usize> {
    let upper = letter.to_ascii_uppercase();
    if upper >= FIRST_REGISTER_LETTER && upper < FIRST_REGISTER_LETTER + USER_REGISTER_COUNT as u8 {
        Some((upper - FIRST_REGISTER_LETTER) as usize)
    } else {
        None
    }
}
