//! # Variable Store (Math Engine — Layer 6)
//!
//! Stores named calculator variables as Q31.32 fixed-point values.
//!
//! ## Supported variables
//!   Ans   — result of the most recent successful evaluation
//!   A–Z   — 26 user-assignable storage registers
//!
//! ## Design
//!   Owned by CalcState. Passed as &VariableStore (immutable) into the math
//!   engine so evaluation has no side effects. Written only by the runtime
//!   (after evaluation) and by evaluate_loop_aggregate (local copy only).

/// Number of user-assignable registers (A through Z).
const USER_REGISTER_COUNT: usize = 26;

/// First letter of the register range.
const FIRST_REGISTER_LETTER: u8 = b'A';

/// Stores all calculator variables as Q31.32 fixed-point values.
///
/// Derives Copy so evaluate_loop_aggregate can cheaply clone it onto the
/// stack as a scoped shadow for loop variable writes.
#[derive(Clone, Copy)]
pub struct VariableStore {
    /// The last successfully evaluated result (the Ans variable).
    last_answer: i64,

    /// Whether last_answer has been set yet (false on fresh boot).
    has_last_answer: bool,

    /// User registers A–Z. Index 0 = A, index 25 = Z.
    /// All initialise to 0. Unwritten registers silently return 0.
    user_registers: [i64; USER_REGISTER_COUNT],
}

impl VariableStore {
    /// Create an empty variable store — all registers zero, Ans undefined.
    pub const fn new() -> Self {
        Self {
            last_answer: 0,
            has_last_answer: false,
            user_registers: [0i64; USER_REGISTER_COUNT],
        }
    }

    /// Read the Ans variable. Returns None if no evaluation has occurred yet.
    pub fn read_ans(&self) -> Option<i64> {
        if self.has_last_answer {
            Some(self.last_answer)
        } else {
            None
        }
    }

    /// Write the Ans variable. Called by the runtime after every evaluation.
    pub fn write_ans(&mut self, value: i64) {
        self.last_answer = value;
        self.has_last_answer = true;
    }

    /// Read a user register by letter (uppercase A–Z only).
    /// Returns Some(0) for unwritten registers. Returns None for invalid letters.
    pub fn read_register(&self, letter: u8) -> Option<i64> {
        Some(self.user_registers[register_letter_to_index(letter)?])
    }

    /// Write a user register by letter (uppercase A–Z only).
    /// Returns false if the letter is outside A–Z.
    pub fn write_register(&mut self, letter: u8, value: i64) -> bool {
        match register_letter_to_index(letter) {
            Some(index) => {
                self.user_registers[index] = value;
                true
            }
            None => false,
        }
    }
}

/// Map a register letter (A–Z, case-insensitive) to a 0-based array index.
/// Returns None if the letter is outside A–Z.
fn register_letter_to_index(letter: u8) -> Option<usize> {
    if letter >= FIRST_REGISTER_LETTER && letter < FIRST_REGISTER_LETTER + USER_REGISTER_COUNT as u8
    {
        Some((letter - FIRST_REGISTER_LETTER) as usize)
    } else {
        None
    }
}
