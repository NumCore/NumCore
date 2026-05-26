use super::fixed_point as fp;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Complex {
    pub re: i64,
    pub im: i64,
}

impl Complex {
    pub const fn new(re: i64, im: i64) -> Self {
        Self { re, im }
    }

    pub const fn zero() -> Self {
        Self { re: 0, im: 0 }
    }

    pub const fn from_real(re: i64) -> Self {
        Self { re, im: 0 }
    }

    pub fn is_real(self) -> bool {
        self.im == 0
    }

    pub fn add(self, other: Self) -> Self {
        Self {
            re: self.re.saturating_add(other.re),
            im: self.im.saturating_add(other.im),
        }
    }

    pub fn sub(self, other: Self) -> Self {
        Self {
            re: self.re.saturating_sub(other.re),
            im: self.im.saturating_sub(other.im),
        }
    }

    pub fn mul(self, other: Self) -> Option<Self> {
        let ac = (self.re as i128) * (other.re as i128);
        let bd = (self.im as i128) * (other.im as i128);
        let ad = (self.re as i128) * (other.im as i128);
        let bc = (self.im as i128) * (other.re as i128);
        let real = ac.checked_sub(bd)? >> 32;
        let imag = ad.checked_add(bc)? >> 32;
        if real > i64::MAX as i128 || real < i64::MIN as i128 {
            return None;
        }
        if imag > i64::MAX as i128 || imag < i64::MIN as i128 {
            return None;
        }
        Some(Self {
            re: real as i64,
            im: imag as i64,
        })
    }

    pub fn div(self, other: Self) -> Option<Self> {
        let c2 = fp::multiply(other.re, other.re)?;
        let d2 = fp::multiply(other.im, other.im)?;
        let den = c2.checked_add(d2)?;
        if den == 0 {
            return None;
        }
        let ac = fp::multiply(self.re, other.re)?;
        let bd = fp::multiply(self.im, other.im)?;
        let num_re = ac.checked_add(bd)?;
        let bc = fp::multiply(self.im, other.re)?;
        let ad = fp::multiply(self.re, other.im)?;
        let num_im = bc.checked_sub(ad)?;
        let re = fp::divide(num_re, den)?;
        let im = fp::divide(num_im, den)?;
        Some(Self { re, im })
    }

    pub fn neg(self) -> Self {
        Self {
            re: self.re.saturating_neg(),
            im: self.im.saturating_neg(),
        }
    }

    pub fn conj(self) -> Self {
        Self {
            re: self.re,
            im: self.im.saturating_neg(),
        }
    }

    pub fn norm_sq(self) -> Option<i64> {
        fp::multiply(self.re, self.re)?.checked_add(fp::multiply(self.im, self.im)?)
    }

    pub fn arg(self) -> i64 {
        fp::atan2(self.im, self.re)
    }

    pub fn from_polar(r: i64, theta: i64) -> Option<Self> {
        let (s, c) = fp::sin_cos(theta);
        let re = fp::multiply(r, c)?;
        let im = fp::multiply(r, s)?;
        Some(Self { re, im })
    }

    pub fn sqrt(z: Self) -> Option<Self> {
        if z.im == 0 && z.re >= 0 {
            return fp::sqrt(z.re).map(|r| Self { re: r, im: 0 });
        }
        if z.im == 0 {
            // sqrt of a negative real: sqrt(-|r|) = 0 + i*sqrt(|r|)
            return fp::sqrt(z.re.checked_neg()?).map(|r| Self { re: 0, im: r });
        }
        let norm = z.norm_sq()?;
        let r = fp::sqrt(norm)?;
        let two = fp::from_integer(2);
        let half_sum = fp::divide(r.checked_add(z.re)?, two)?;
        let sqrt_r = fp::sqrt(half_sum)?;
        let half_diff = fp::divide(r.checked_sub(z.re)?, two)?;
        let sqrt_diff = fp::sqrt(half_diff)?;
        let im = if z.im >= 0 { sqrt_diff } else { -sqrt_diff };
        Some(Self { re: sqrt_r, im })
    }

    pub fn integer_pow(self, exp: i64) -> Option<Self> {
        if exp < 0 {
            let pos = self.integer_pow(-exp)?;
            return Self {
                re: fp::FIXED_ONE,
                im: 0,
            }
            .div(pos);
        }
        let mut result = Self {
            re: fp::FIXED_ONE,
            im: 0,
        };
        let mut base = self;
        let mut e = exp;
        while e > 0 {
            if e & 1 == 1 {
                result = result.mul(base)?;
            }
            base = base.mul(base)?;
            e >>= 1;
        }
        Some(result)
    }

    pub fn exp(z: Self) -> Option<Self> {
        let mag = fp::natural_exp(z.re)?;
        let (s, c) = fp::sin_cos(z.im);
        let re = fp::multiply(mag, c)?;
        let im = fp::multiply(mag, s)?;
        Some(Self { re, im })
    }

    // ── Logarithm ────────────────────────────────────────────────────────────

    pub fn ln(z: Self) -> Option<Self> {
        let norm = z.norm_sq()?;
        let r = fp::sqrt(norm)?;
        let re = fp::natural_log(r)?;
        let im = z.arg();
        Some(Self { re, im })
    }

    pub fn log10(z: Self) -> Option<Self> {
        let ln_z = Self::ln(z)?;
        let ln_10 = fp::natural_log(fp::from_integer(10))?;
        Some(Self {
            re: fp::divide(ln_z.re, ln_10)?,
            im: fp::divide(ln_z.im, ln_10)?,
        })
    }

    pub fn log2(z: Self) -> Option<Self> {
        let ln_z = Self::ln(z)?;
        let ln_2 = fp::natural_log(fp::from_integer(2))?;
        Some(Self {
            re: fp::divide(ln_z.re, ln_2)?,
            im: fp::divide(ln_z.im, ln_2)?,
        })
    }

    // ── Trigonometric ────────────────────────────────────────────────────────

    pub fn sin(z: Self) -> Option<Self> {
        let sh = fp::sinh(z.im)?;
        let ch = fp::cosh(z.im)?;
        let re = fp::multiply(fp::sin(z.re), ch)?;
        let im = fp::multiply(fp::cos(z.re), sh)?;
        Some(Self { re, im })
    }

    pub fn cos(z: Self) -> Option<Self> {
        let sh = fp::sinh(z.im)?;
        let ch = fp::cosh(z.im)?;
        let re = fp::multiply(fp::cos(z.re), ch)?;
        let im = fp::multiply(fp::sin(z.re), sh)?;
        Some(Self {
            re,
            im: im.checked_neg()?,
        })
    }

    pub fn tan(z: Self) -> Option<Self> {
        Self::sin(z)?.div(Self::cos(z)?)
    }

    // ── Hyperbolic ──────────────────────────────────────────────────────────

    pub fn sinh(z: Self) -> Option<Self> {
        let sh = fp::sinh(z.re)?;
        let ch = fp::cosh(z.re)?;
        let re = fp::multiply(sh, fp::cos(z.im))?;
        let im = fp::multiply(ch, fp::sin(z.im))?;
        Some(Self { re, im })
    }

    pub fn cosh(z: Self) -> Option<Self> {
        let sh = fp::sinh(z.re)?;
        let ch = fp::cosh(z.re)?;
        let re = fp::multiply(ch, fp::cos(z.im))?;
        let im = fp::multiply(sh, fp::sin(z.im))?;
        Some(Self { re, im })
    }

    pub fn tanh(z: Self) -> Option<Self> {
        Self::sinh(z)?.div(Self::cosh(z)?)
    }

    // ── Inverse trigonometric ───────────────────────────────────────────────

    pub fn asin(z: Self) -> Option<Self> {
        let i = Self {
            re: 0,
            im: fp::FIXED_ONE,
        };
        let one = Self::from_real(fp::FIXED_ONE);
        let iz = i.mul(z)?;
        let z_sq = z.mul(z)?;
        let inner = Self::sqrt(one.sub(z_sq))?;
        let sum = iz.add(inner);
        let ln_val = Self::ln(sum)?;
        let neg_i = Self {
            re: 0,
            im: -fp::FIXED_ONE,
        };
        neg_i.mul(ln_val)
    }

    pub fn acos(z: Self) -> Option<Self> {
        let i = Self {
            re: 0,
            im: fp::FIXED_ONE,
        };
        let one = Self::from_real(fp::FIXED_ONE);
        let z_sq = z.mul(z)?;
        let inner = Self::sqrt(one.sub(z_sq))?;
        let i_inner = i.mul(inner)?;
        let sum = z.add(i_inner);
        let ln_val = Self::ln(sum)?;
        let neg_i = Self {
            re: 0,
            im: -fp::FIXED_ONE,
        };
        neg_i.mul(ln_val)
    }

    pub fn atan(z: Self) -> Option<Self> {
        let i = Self {
            re: 0,
            im: fp::FIXED_ONE,
        };
        let two = Self::from_real(fp::from_integer(2));
        let i_plus_z = i.add(z);
        let i_minus_z = i.sub(z);
        let q = i_plus_z.div(i_minus_z)?;
        let ln_val = Self::ln(q)?;
        let i_over_2 = i.div(two)?;
        i_over_2.mul(ln_val)
    }

    // ── Inverse hyperbolic ──────────────────────────────────────────────────

    pub fn asinh(z: Self) -> Option<Self> {
        let one = Self::from_real(fp::FIXED_ONE);
        let z_sq = z.mul(z)?;
        let inner = Self::sqrt(z_sq.add(one))?;
        let sum = z.add(inner);
        Self::ln(sum)
    }

    pub fn acosh(z: Self) -> Option<Self> {
        let one = Self::from_real(fp::FIXED_ONE);
        let z_minus_one = Self::sqrt(z.sub(one))?;
        let z_plus_one = Self::sqrt(z.add(one))?;
        let prod = z_minus_one.mul(z_plus_one)?;
        let sum = z.add(prod);
        Self::ln(sum)
    }

    pub fn atanh(z: Self) -> Option<Self> {
        let one = Self::from_real(fp::FIXED_ONE);
        let half = Self::from_real(fp::divide(fp::FIXED_ONE, fp::from_integer(2))?);
        let one_plus_z = one.add(z);
        let one_minus_z = one.sub(z);
        let q = one_plus_z.div(one_minus_z)?;
        let ln_val = Self::ln(q)?;
        half.mul(ln_val)
    }
}
