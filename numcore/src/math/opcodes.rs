#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    PushI64 = 0x01,
    PushReg = 0x02,
    PushAns = 0x03,
    PushConstPi = 0x04,
    PushConstE = 0x05,
    PushConstI = 0x06,
    PushMatReg = 0x07,
    PushMatLit = 0x08,
    ConstructSci = 0x09,

    Add = 0x10,
    Sub = 0x11,
    Mul = 0x12,
    Div = 0x13,
    Mod = 0x14,
    Pow = 0x15,
    Neg = 0x16,

    CallFunction = 0x20,
    CallBinomP = 0x40,
    CallPoissonP = 0x41,
    CallChiCDF = 0x42,
    CallNthRoot = 0x43,
    CallMatrixFunc = 0x50,

    Sto = 0x60,
    StoMat = 0x61,

    LoopSum = 0x70,
    LoopInt = 0x71,

    Halt = 0xFF,
}

impl Op {
    pub fn from_u8(b: u8) -> Option<Op> {
        match b {
            0x01 => Some(Op::PushI64),
            0x02 => Some(Op::PushReg),
            0x03 => Some(Op::PushAns),
            0x04 => Some(Op::PushConstPi),
            0x05 => Some(Op::PushConstE),
            0x06 => Some(Op::PushConstI),
            0x07 => Some(Op::PushMatReg),
            0x08 => Some(Op::PushMatLit),
            0x09 => Some(Op::ConstructSci),

            0x10 => Some(Op::Add),
            0x11 => Some(Op::Sub),
            0x12 => Some(Op::Mul),
            0x13 => Some(Op::Div),
            0x14 => Some(Op::Mod),
            0x15 => Some(Op::Pow),
            0x16 => Some(Op::Neg),

            0x20 => Some(Op::CallFunction),
            0x40 => Some(Op::CallBinomP),
            0x41 => Some(Op::CallPoissonP),
            0x42 => Some(Op::CallChiCDF),
            0x43 => Some(Op::CallNthRoot),
            0x50 => Some(Op::CallMatrixFunc),

            0x60 => Some(Op::Sto),
            0x61 => Some(Op::StoMat),

            0x70 => Some(Op::LoopSum),
            0x71 => Some(Op::LoopInt),

            0xFF => Some(Op::Halt),
            _ => None,
        }
    }

    pub fn width(&self) -> usize {
        match self {
            Op::PushI64 => 9,
            Op::PushReg | Op::PushMatReg | Op::Sto | Op::StoMat => 2,
            Op::CallFunction | Op::CallMatrixFunc => 2,
            Op::ConstructSci => 1,
            Op::LoopSum | Op::LoopInt => 5,
            _ => 1,
        }
    }
}
