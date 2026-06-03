use crate::math::complex::Complex;
use crate::math::lexer::{LexResult, Token, MAX_TOKEN_COUNT};
use crate::math::parser::{AstNode, ParseTree, MAX_NODE_COUNT};
use crate::math::vars::VariableStore;
use crate::math::AngleMode;
use crate::math::MathMode;

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
const INPUT_BUFFER_CAPACITY: usize = 64;

/// Maximum length of a formatted result string.
const RESULT_BUFFER_CAPACITY: usize = 48;

// ─── CalcState ────────────────────────────────────────────────────────────────

pub struct CalcState {
    /// The expression the user is currently composing.
    pub(crate) input_buffer: [u8; INPUT_BUFFER_CAPACITY],

    /// Number of valid bytes in `input_buffer`.
    pub(crate) input_length: usize,

    /// Cursor position within `input_buffer` (0..=input_length).
    cursor_position: usize,

    /// The last formatted result string, stored for scrolling.
    last_result: [u8; RESULT_BUFFER_CAPACITY],

    /// Number of valid bytes in `last_result`.
    last_result_len: usize,

    /// Horizontal scroll offset for the result display (in characters).
    result_scroll_offset: usize,

    /// All calculator variables (Ans + A–F). Owned here, borrowed by math engine.
    pub(crate) variables: VariableStore,

    /// Currently active calculator mode.
    active_mode: CalculatorMode,

    /// Current angle unit for trigonometric functions.
    angle_mode: AngleMode,

    /// Reusable scratch buffer for the lexer output.
    pub lex_scratch: LexResult,

    /// Reusable scratch buffer for the parser output (AST).
    pub parse_scratch: ParseTree,

    /// Scratch buffer for expression submission (avoids stack allocation).
    pub expr_scratch: [u8; INPUT_BUFFER_CAPACITY],
}

impl CalcState {
    /// Create a fresh state as if just powered on.
    pub const fn new() -> Self {
        Self {
            input_buffer: [0u8; INPUT_BUFFER_CAPACITY],
            input_length: 0,
            cursor_position: 0,
            last_result: [0u8; RESULT_BUFFER_CAPACITY],
            last_result_len: 0,
            result_scroll_offset: 0,
            variables: VariableStore::new(),
            active_mode: CalculatorMode::Standard,
            angle_mode: AngleMode::Radians,
            lex_scratch: LexResult {
                tokens: [Token::Number(0i64); MAX_TOKEN_COUNT],
                token_count: 0,
            },
            parse_scratch: ParseTree {
                nodes: [AstNode::Literal(0i64); MAX_NODE_COUNT],
                node_count: 0,
                root_index: 0,
            },
            expr_scratch: [0u8; INPUT_BUFFER_CAPACITY],
        }
    }

    // ── Input buffer operations ───────────────────────────────────────────────

    pub fn append_character_to_input(&mut self, character: u8) -> bool {
        if self.input_length >= INPUT_BUFFER_CAPACITY - 1 {
            return false;
        }
        let pos = self.cursor_position;
        if pos < self.input_length {
            let end = self.input_length;
            let src = self.input_buffer.as_mut();
            src.copy_within(pos..end, pos + 1);
        }
        self.input_buffer[pos] = character;
        self.input_length += 1;
        self.cursor_position += 1;
        true
    }

    pub fn remove_last_input_character(&mut self) -> bool {
        if self.input_length == 0 {
            return false;
        }
        if self.cursor_position > 0 {
            let pos = self.cursor_position - 1;
            let end = self.input_length;
            let src = self.input_buffer.as_mut();
            src.copy_within(pos + 1..end, pos);
            self.input_buffer[end - 1] = 0;
            self.input_length -= 1;
            self.cursor_position = pos;
        }
        true
    }

    pub fn current_input(&self) -> &[u8] {
        &self.input_buffer[..self.input_length]
    }

    pub fn clear_input(&mut self) {
        for byte in &mut self.input_buffer[..self.input_length] {
            *byte = 0;
        }
        self.input_length = 0;
        self.cursor_position = 0;
    }

    // ── Cursor operations ─────────────────────────────────────────────────────

    pub fn move_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor_position < self.input_length {
            self.cursor_position += 1;
        }
    }

    pub fn cursor_position(&self) -> usize {
        self.cursor_position
    }

    // ── Result scroll operations ──────────────────────────────────────────────

    pub fn scroll_result_left(&mut self) {
        if self.result_scroll_offset > 0 {
            self.result_scroll_offset -= 1;
        }
    }

    pub fn scroll_result_right(&mut self) {
        if self.last_result_len <= 15 {
            return;
        }
        let max = self.last_result_len - 13;
        if self.result_scroll_offset < max {
            self.result_scroll_offset += 1;
        }
    }

    pub fn result_scroll_offset(&self) -> usize {
        self.result_scroll_offset
    }

    /// Store the last formatted result for scrolling.
    pub fn set_last_result(&mut self, result: &[u8]) {
        let len = result.len().min(RESULT_BUFFER_CAPACITY);
        self.last_result[..len].copy_from_slice(result);
        self.last_result_len = len;
        self.result_scroll_offset = 0;
    }

    /// The full last result string.
    pub fn last_result(&self) -> &[u8] {
        &self.last_result[..self.last_result_len]
    }

    /// Clear the stored result.
    pub fn clear_last_result(&mut self) {
        self.last_result_len = 0;
        self.result_scroll_offset = 0;
    }

    pub fn has_result(&self) -> bool {
        self.last_result_len > 0
    }

    // ── Variable store access ─────────────────────────────────────────────────

    pub fn variables(&self) -> &VariableStore {
        &self.variables
    }

    pub fn record_answer(&mut self, answer: Complex) {
        self.variables.write_ans(answer);
    }

    pub fn write_user_register(&mut self, letter: u8, value: Complex) -> bool {
        self.variables.write_register(letter, value)
    }

    // ── Mode operations ───────────────────────────────────────────────────────

    pub fn active_mode(&self) -> CalculatorMode {
        self.active_mode
    }

    pub fn switch_mode(&mut self, new_mode: CalculatorMode) {
        self.active_mode = new_mode;
        self.clear_input();
    }

    pub fn math_mode(&self) -> MathMode {
        match self.active_mode {
            CalculatorMode::Standard => MathMode::Standard,
            CalculatorMode::Advanced => MathMode::Advanced,
        }
    }

    pub fn angle_mode(&self) -> AngleMode {
        self.angle_mode
    }

    pub fn toggle_angle_mode(&mut self) {
        self.angle_mode = match self.angle_mode {
            AngleMode::Radians => AngleMode::Degrees,
            AngleMode::Degrees => AngleMode::Radians,
        };
        self.clear_input();
    }
}

impl Default for CalcState {
    fn default() -> Self {
        Self::new()
    }
}
