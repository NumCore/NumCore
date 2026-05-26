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
        let real = (ac - bd) >> 32;
        let imag = (ad + bc) >> 32;
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
            re: -self.re,
            im: -self.im,
        }
    }

    pub fn conj(self) -> Self {
        Self {
            re: self.re,
            im: -self.im,
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
        if z.im == 0 {
            return fp::sqrt(z.re).map(|r| Self { re: r, im: 0 });
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
            return Self { re: fp::FIXED_ONE, im: 0 }.div(pos);
        }
        let mut result = Self { re: fp::FIXED_ONE, im: 0 };
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
}
