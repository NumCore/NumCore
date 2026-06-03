use crate::math::complex::Complex;
use crate::math::lexer::{LexResult, Token, MAX_TOKEN_COUNT};
use crate::math::matrix::Matrix;
use crate::math::parser::{AstNode, ParseTree, MAX_NODE_COUNT};
use crate::math::vars::VariableStore;
use crate::math::AngleMode;
use crate::math::MathMode;

/// Max characters per row in the virtual matrix buffer.
pub const MAX_ROW_LEN: usize = 80;

/// Pre-rendered matrix virtual buffer.
/// Each row is a flat string of `row_len` chars:
///   bracket + gap + padded_value + gap + padded_value + ... + gap
/// where each value is padded to its column's max width.
/// The display shows a 14-char viewport (cols 0-13) of this buffer
/// offset by col_off. Cols 14-15 on the display are fixed overlays
/// (whitespace margin + scroll arrow).
pub struct DisplayGrid {
    /// Full formatted rows (max 5 rows × MAX_ROW_LEN chars).
    pub rows: [[u8; MAX_ROW_LEN]; 5],
    /// Actual length of each formatted row (all rows same length).
    pub row_len: u8,
    /// Number of matrix rows.
    pub num_rows: u8,
}

impl DisplayGrid {
    pub const fn empty() -> Self {
        Self {
            rows: [[0u8; MAX_ROW_LEN]; 5],
            row_len: 0,
            num_rows: 0,
        }
    }
}

// ─── Calculator modes ─────────────────────────────────────────────────────────

/// The operating mode currently active.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CalculatorMode {
    /// Basic four-function arithmetic + scientific functions. Initial mode.
    Standard,
    /// Complex numbers, imaginary unit i.
    Advanced,
    /// Matrix operations (MatA-MatC, det, transpose, identity).
    Matrix,
}

// ─── Capacity constants ───────────────────────────────────────────────────────

/// Maximum characters in a single input expression.
const INPUT_BUFFER_CAPACITY: usize = 64;

/// Maximum length of a formatted result string.
const RESULT_BUFFER_CAPACITY: usize = 48;

// ─── CalcState ────────────────────────────────────────────────────────────────

#[repr(C)]
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

    /// Vertical scroll offset for matrix result display (which row is at top).
    matrix_scroll_offset: usize,

    /// Horizontal scroll offset for matrix display (in character positions).
    matrix_col_offset: usize,

    /// All calculator variables (Ans, A–Z, MatA–MatC). Owned here, borrowed by math engine.
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
            matrix_scroll_offset: 0,
            matrix_col_offset: 0,
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
                mat_cache: [None; crate::math::parser::MATRIX_CACHE_SIZE],
                mat_cache_count: 0,
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

    /// Store the last formatted result for scrolling. Also clears the matrix
    /// Ans so the OLED doesn't show a stale matrix after a scalar result.
    pub fn set_last_result(&mut self, result: &[u8]) {
        let len = result.len().min(RESULT_BUFFER_CAPACITY);
        self.last_result[..len].copy_from_slice(result);
        self.last_result_len = len;
        self.result_scroll_offset = 0;
        self.variables.clear_matrix_ans();
    }

    /// The full last result string.
    pub fn last_result(&self) -> &[u8] {
        &self.last_result[..self.last_result_len]
    }

    /// Clear the displayed result (not the Ans/matrix Ans — those persist
    /// so subsequent Ans expressions can still reference them).
    pub fn clear_last_result(&mut self) {
        self.last_result_len = 0;
        self.result_scroll_offset = 0;
    }

    pub fn has_result(&self) -> bool {
        self.last_result_len > 0
    }

    pub fn has_matrix_result(&self) -> bool {
        self.variables.read_matrix_ans().is_some()
    }

    pub fn get_matrix_result(&self) -> Option<Matrix> {
        self.variables.read_matrix_ans()
    }

    pub fn clear_matrix_result(&mut self) {
        self.matrix_scroll_offset = 0;
        self.matrix_col_offset = 0;
    }

    pub fn matrix_scroll_offset(&self) -> usize {
        self.matrix_scroll_offset
    }

    pub fn matrix_col_offset(&self) -> usize {
        self.matrix_col_offset
    }

    pub fn scroll_matrix_up(&mut self) {
        if self.matrix_scroll_offset > 0 {
            self.matrix_scroll_offset -= 1;
        }
    }

    pub fn scroll_matrix_down(&mut self) {
        if let Some(m) = self.variables.read_matrix_ans() {
            let rows = m.rows as usize;
            if rows > 2 && self.matrix_scroll_offset + 2 < rows {
                self.matrix_scroll_offset += 1;
            }
        }
    }

    pub fn scroll_matrix_left(&mut self) {
        if self.matrix_col_offset > 0 {
            self.matrix_col_offset -= 1;
        }
    }

    pub fn scroll_matrix_right(&mut self) {
        self.matrix_col_offset += 1;
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
        self.matrix_scroll_offset = 0;
        self.matrix_col_offset = 0;
    }

    pub fn math_mode(&self) -> MathMode {
        match self.active_mode {
            CalculatorMode::Standard => MathMode::Standard,
            CalculatorMode::Advanced => MathMode::Advanced,
            CalculatorMode::Matrix => MathMode::Matrix,
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
