//! # Calculator State (Layer 5 — Runtime)
//!
//! The single source of truth for all runtime state:
//!   - Current expression being typed (input buffer)
//!   - Variable store (Ans + A–F registers), owned here, passed to math engine
//!   - Active calculator mode
//!   - Scratch buffers for lexing and parsing — allocated here in static RAM
//!     so they are never stack-allocated during evaluation, which would
//!     overflow the LM3S811's 8 KB SRAM.
//!
//! No hardware knowledge lives here. No math logic lives here.
//! This is a pure data container with intention-revealing mutator methods.

use crate::math::lexer::{LexResult, Token, MAX_TOKEN_COUNT};
use crate::math::parser::{AstNode, ParseTree, MAX_NODE_COUNT};
use crate::math::vars::VariableStore;

// ─── Calculator modes ─────────────────────────────────────────────────────────

/// The operating mode currently active.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CalculatorMode {
    /// Basic four-function arithmetic + scientific functions. Initial mode.
    Standard,
    /// Future: matrix operations, complex numbers, etc.
    Advanced,
}

// ─── Capacity constants ───────────────────────────────────────────────────────

/// Maximum characters in a single input expression.
/// 64 bytes is generous for the LM3S811's 8 KB SRAM.
const INPUT_BUFFER_CAPACITY: usize = 64;

// ─── CalcState ────────────────────────────────────────────────────────────────

/// Complete runtime state of the calculator.
///
/// ## Memory layout rationale
/// The LM3S811 has only 8 KB of SRAM. `LexResult` (~256 bytes) and
/// `ParseTree` (~1 KB) must NOT be stack-allocated inside the evaluation
/// call chain — the call stack alone would exceed available RAM. Instead,
/// they live here as fields, allocated once in static RAM for the lifetime
/// of the firmware.
pub struct CalcState {
    /// The expression the user is currently composing.
    input_buffer: [u8; INPUT_BUFFER_CAPACITY],

    /// Number of valid bytes in `input_buffer`.
    input_length: usize,

    /// All calculator variables (Ans + A–F). Owned here, borrowed by math engine.
    pub(crate) variables: VariableStore,

    /// Currently active calculator mode.
    active_mode: CalculatorMode,

    /// Reusable scratch buffer for the lexer output.
    /// Lives here to avoid stack-allocating ~256 bytes per evaluation.
    pub lex_scratch: LexResult,

    /// Reusable scratch buffer for the parser output (AST).
    /// Lives here to avoid stack-allocating ~1 KB per evaluation.
    pub parse_scratch: ParseTree,
}

impl CalcState {
    /// Create a fresh state as if just powered on.
    pub fn new() -> Self {
        Self {
            input_buffer: [0u8; INPUT_BUFFER_CAPACITY],
            input_length: 0,
            variables: VariableStore::new(),
            active_mode: CalculatorMode::Standard,
            lex_scratch: LexResult {
                tokens: [Token::Number(0i64); MAX_TOKEN_COUNT],
                token_count: 0,
            },
            parse_scratch: ParseTree {
                nodes: [AstNode::Literal(0i64); MAX_NODE_COUNT],
                node_count: 0,
                root_index: 0,
            },
        }
    }

    // ── Input buffer operations ───────────────────────────────────────────────

    /// Append one character to the input buffer.
    /// Returns `true` if stored, `false` if buffer was full.
    pub fn append_character_to_input(&mut self, character: u8) -> bool {
        if self.input_length >= INPUT_BUFFER_CAPACITY - 1 { return false; }
        self.input_buffer[self.input_length] = character;
        self.input_length += 1;
        true
    }

    /// Remove the last character from the input buffer.
    /// Returns `true` if a character was removed, `false` if already empty.
    pub fn remove_last_input_character(&mut self) -> bool {
        if self.input_length == 0 { return false; }
        self.input_length -= 1;
        self.input_buffer[self.input_length] = 0;
        true
    }

    /// Immutable view of the current input expression bytes.
    pub fn current_input(&self) -> &[u8] {
        &self.input_buffer[..self.input_length]
    }

    /// Clear the input buffer for a new expression.
    pub fn clear_input(&mut self) {
        for byte in &mut self.input_buffer[..self.input_length] { *byte = 0; }
        self.input_length = 0;
    }

    // ── Variable store access ─────────────────────────────────────────────────

    /// Immutable reference to the variable store — passed to the math engine.
    pub fn variables(&self) -> &VariableStore {
        &self.variables
    }

    /// Store a new value for `Ans`. Called by the runtime after evaluation.
    pub fn record_answer(&mut self, answer: i64) {
        self.variables.write_ans(answer);
    }

    /// Write a user register (A–F). Returns false if letter is out of range.
    pub fn write_user_register(&mut self, letter: u8, value: i64) -> bool {
        self.variables.write_register(letter, value)
    }

    // ── Mode operations ───────────────────────────────────────────────────────

    /// Return the currently active mode.
    pub fn active_mode(&self) -> CalculatorMode {
        self.active_mode
    }

    /// Switch to a different mode, clearing the input buffer.
    pub fn switch_mode(&mut self, new_mode: CalculatorMode) {
        self.active_mode = new_mode;
        self.clear_input();
    }
}