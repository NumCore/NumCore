#[derive(Clone, Copy)]
pub enum StoreTarget {
    Scalar,
    Matrix,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Power,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MathFunction {
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    SinH,
    CosH,
    TanH,
    ASinH,
    ACosH,
    ATanH,
    Sqrt,
    Abs,
    Log,
    Ln,
    Log2,
    Exp,
    Floor,
    Ceil,
    Round,
    Deg,
    Rad,
    LnGamma,
}

impl MathFunction {
    pub fn from_u8(b: u8) -> Option<Self> {
        match b {
            0 => Some(MathFunction::Sin),
            1 => Some(MathFunction::Cos),
            2 => Some(MathFunction::Tan),
            3 => Some(MathFunction::Asin),
            4 => Some(MathFunction::Acos),
            5 => Some(MathFunction::Atan),
            6 => Some(MathFunction::SinH),
            7 => Some(MathFunction::CosH),
            8 => Some(MathFunction::TanH),
            9 => Some(MathFunction::ASinH),
            10 => Some(MathFunction::ACosH),
            11 => Some(MathFunction::ATanH),
            12 => Some(MathFunction::Sqrt),
            13 => Some(MathFunction::Abs),
            14 => Some(MathFunction::Log),
            15 => Some(MathFunction::Ln),
            16 => Some(MathFunction::Log2),
            17 => Some(MathFunction::Exp),
            18 => Some(MathFunction::Floor),
            19 => Some(MathFunction::Ceil),
            20 => Some(MathFunction::Round),
            21 => Some(MathFunction::Deg),
            22 => Some(MathFunction::Rad),
            23 => Some(MathFunction::LnGamma),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ThreeArgMathFunction {
    BinomialProbability,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TwoArgMathFunction {
    PoissonProbability,
    ChiSquaredCDF,
    NthRoot,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LoopOperation {
    Summation,
    Integration,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MathConstant {
    Pi,
    E,
    ImaginaryUnit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VariableRef {
    Ans,
    Register(u8),
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MatrixFunction {
    Det,
    Transpose,
    Identity,
    Inv,
    Cofactor,
    Adjugate,
}

impl MatrixFunction {
    pub fn from_u8(b: u8) -> Option<Self> {
        match b {
            0 => Some(MatrixFunction::Det),
            1 => Some(MatrixFunction::Transpose),
            2 => Some(MatrixFunction::Identity),
            3 => Some(MatrixFunction::Inv),
            4 => Some(MatrixFunction::Cofactor),
            5 => Some(MatrixFunction::Adjugate),
            _ => None,
        }
    }
}
