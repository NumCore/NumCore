use super::complex::Complex;
use super::matrix::Matrix;

const USER_REGISTER_COUNT: usize = 26;
const FIRST_REGISTER_LETTER: u8 = b'A';
const MATRIX_REGISTER_COUNT: usize = 3;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct VariableStore {
    last_answer: Complex,
    has_last_answer: bool,
    user_registers: [Complex; USER_REGISTER_COUNT],
    last_matrix_result: Option<Matrix>,
    matrix_registers: [Option<Matrix>; MATRIX_REGISTER_COUNT],
}

impl VariableStore {
    pub const fn new() -> Self {
        Self {
            last_answer: Complex::zero(),
            has_last_answer: false,
            user_registers: [Complex::zero(); USER_REGISTER_COUNT],
            last_matrix_result: None,
            matrix_registers: [None; MATRIX_REGISTER_COUNT],
        }
    }

    pub fn read_ans(&self) -> Option<Complex> {
        if self.has_last_answer {
            Some(self.last_answer)
        } else {
            None
        }
    }

    pub fn write_ans(&mut self, value: Complex) {
        self.last_answer = value;
        self.has_last_answer = true;
    }

    pub fn read_register(&self, letter: u8) -> Option<Complex> {
        Some(self.user_registers[register_letter_to_index(letter)?])
    }

    pub fn write_register(&mut self, letter: u8, value: Complex) -> bool {
        match register_letter_to_index(letter) {
            Some(index) => {
                self.user_registers[index] = value;
                true
            }
            None => false,
        }
    }

    pub fn read_matrix_ans(&self) -> Option<Matrix> {
        self.last_matrix_result
    }

    pub fn write_matrix_ans(&mut self, value: Matrix) {
        self.last_matrix_result = Some(value);
    }

    pub fn clear_matrix_ans(&mut self) {
        self.last_matrix_result = None;
    }

    pub fn read_matrix_reg(&self, letter: u8) -> Option<Matrix> {
        let idx = matrix_letter_to_index(letter)?;
        self.matrix_registers[idx]
    }

    pub fn write_matrix_reg(&mut self, letter: u8, value: Matrix) -> bool {
        match matrix_letter_to_index(letter) {
            Some(index) => {
                self.matrix_registers[index] = Some(value);
                true
            }
            None => false,
        }
    }

    pub fn matrix_reg_slot(&self, idx: usize) -> Option<Matrix> {
        if idx < MATRIX_REGISTER_COUNT {
            self.matrix_registers[idx]
        } else {
            None
        }
    }
}

fn register_letter_to_index(letter: u8) -> Option<usize> {
    if letter >= FIRST_REGISTER_LETTER && letter < FIRST_REGISTER_LETTER + USER_REGISTER_COUNT as u8
    {
        Some((letter - FIRST_REGISTER_LETTER) as usize)
    } else {
        None
    }
}

fn matrix_letter_to_index(letter: u8) -> Option<usize> {
    if letter >= b'A' && letter <= b'C' {
        Some((letter - b'A') as usize)
    } else {
        None
    }
}
