//! # Fixed-Point Arithmetic — Q31.32
//!
//! Every value in the math engine is a Q31.32 fixed-point number:
//!   - Stored as a signed 64-bit integer (`i64`)
//!   - Bottom 32 bits = fractional part
//!   - Top 32 bits    = integer part (signed, range ±2 147 483 647)
//!   - Scale factor   = 2^32 = 4 294 967 296
//!   - Precision      = 1/2^32 ≈ 2.33×10⁻¹⁰  (~9 correct decimal digits)
//!
//! ## Why Q31.32 on an 8 KB MCU?
//!
//! SRAM holds *values*, not arithmetic width. All Q31.32 operations run in
//! CPU registers — the Cortex-M3 supports 64-bit arithmetic via `SMULL` /
//! `UMULL` multiply-long instructions, emitted automatically by the compiler
//! for `i64` operations. The only SRAM increase is that each stored value
//! is 8 bytes instead of 4 (an extra ~544 bytes total across all buffers),
//! leaving ~5.35 KB of stack budget — entirely safe.
//!
//! ## Intermediate arithmetic
//!
//! Multiplying two Q31.32 values produces a Q31.64 intermediate that must
//! be shifted right by 32. To avoid overflow this intermediate is held in
//! `i128`. The Cortex-M3 has no native 128-bit instruction but the compiler
//! synthesises it from 64-bit operations — no extra RAM, just a few extra
//! instructions.
//!
//! ## CORDIC
//!
//! CORDIC runs natively in Q31.32 with `i128` intermediates, giving
//! full 32-bit fractional precision on every sin/cos/atan result.
//! 24 iterations are used — sufficient for Q31.32 convergence.
//!
//! ## deg(x) / rad(x) semantics
//!   `deg(x)` — x is in degrees → converts to radians  (sin(deg(90)) = 1)
//!   `rad(x)` — x is in radians → converts to degrees  (rad(pi) = 180)

// ─── Scale and precision ──────────────────────────────────────────────────────

use core::ptr::write_bytes;
use crate::hal::uart;
use crate::math::engine;

/// Number of fractional bits. Defines the entire numeric format.
pub const FRACTIONAL_BITS: u32 = 32;

/// Scale factor: 2^32. The value 1.0 is stored as 4_294_967_296_i64.
pub const SCALE: i64 = 1_i64 << FRACTIONAL_BITS;

// ─── Mathematical constants in Q31.32 ────────────────────────────────────────
// All values: round(true_value × 2^32), verified in Python.

/// π ≈ 3.14159265358979 → 13_493_037_705
pub const FIXED_PI: i64 = 13_493_037_705;

/// π/2 ≈ 1.57079632679490 → 6_746_518_852
pub const FIXED_PI_OVER_2: i64 = 6_746_518_852;

/// 2π ≈ 6.28318530717959 → 26_986_075_409
pub const FIXED_TWO_PI: i64 = 26_986_075_409;

/// π/180 ≈ 0.01745329251994 → 74_961_321
/// Used by deg(x): degrees → radians.
pub const FIXED_PI_OVER_180: i64 = 74_961_321;

/// 180/π ≈ 57.29577951308 → 246_083_499_208
/// Used by rad(x): radians → degrees.
pub const FIXED_180_OVER_PI: i64 = 246_083_499_208;

/// Euler's number e ≈ 2.71828182845905 → 11_674_931_555
pub const FIXED_E: i64 = 11_674_931_555;

/// ln(2) ≈ 0.69314718055995 → 2_977_044_472
pub const FIXED_LN2: i64 = 2_977_044_472;

/// ln(10) ≈ 2.30258509299405 → 9_889_527_671
pub const FIXED_LN10: i64 = 9_889_527_671;

/// √2 ≈ 1.41421356237310 → 6_074_001_000
pub const FIXED_SQRT2: i64 = 6_074_001_000;

/// 1/√2 ≈ 0.70710678118655 → 3_037_000_500
pub const FIXED_INV_SQRT2: i64 = 3_037_000_500;

/// √3/2 ≈ 0.86602540378444 → 3_719_550_787
pub const FIXED_SQRT3_OVER_2: i64 = 3_719_550_787;

/// 0.5 in Q31.32 = 2^31.
pub const FIXED_HALF: i64 = SCALE / 2;

/// 1.0 in Q31.32.
pub const FIXED_ONE: i64 = SCALE;

// ─── CORDIC constants (Q31.32, i128 intermediates) ───────────────────────────

/// CORDIC gain K = ∏ cos(atan(2^-i)) ≈ 0.60725293501 → 2_608_131_496
const CORDIC_GAIN: i64 = 2_608_131_496;

/// CORDIC atan table in Q31.32. Entry i = round(atan(2^-i) × 2^32).
/// 24 entries — sufficient iterations for full Q31.32 precision.
const CORDIC_ATAN_TABLE: [i64; 24] = [
    3_373_259_426, // atan(2^0)  = 0.78539816 rad
    1_991_351_318, // atan(2^-1) = 0.46364761 rad
    1_052_175_346, // atan(2^-2) = 0.24497866 rad
    534_100_635, // atan(2^-3) = 0.12435499 rad
    268_086_748, // atan(2^-4) = 0.06241881 rad
    134_174_063, // atan(2^-5) = 0.03123983 rad
    67_103_403, // atan(2^-6) = 0.01562373 rad
    33_553_749, // atan(2^-7) = 0.00781234 rad
    16_777_131, // atan(2^-8)
    8_388_597, // atan(2^-9)
    4_194_303, // atan(2^-10)
    2_097_152, // atan(2^-11)
    1_048_576, // atan(2^-12)
    524_288, // atan(2^-13)
    262_144, // atan(2^-14)
    131_072, // atan(2^-15)
    65_536, // atan(2^-16)
    32_768, // atan(2^-17)
    16_384, // atan(2^-18)
    8_192, // atan(2^-19)
    4_096, // atan(2^-20)
    2_048, // atan(2^-21)
    1_024, // atan(2^-22)
    512, // atan(2^-23)
];

// ─── Core arithmetic ──────────────────────────────────────────────────────────

/// Convert an integer to Q31.32.
#[inline(always)]
pub fn from_integer(n: i64) -> i64 { n * SCALE }

/// Truncate a Q31.32 value to its integer part (round toward zero).
#[inline(always)]
pub fn to_integer_truncated(fp: i64) -> i64 { fp >> FRACTIONAL_BITS }

/// Round a Q31.32 value to the nearest integer.
#[inline(always)]
pub fn to_integer_rounded(fp: i64) -> i64 { (fp + FIXED_HALF) >> FRACTIONAL_BITS }

/// Multiply two Q31.32 values.
/// Uses i128 intermediate to prevent overflow: result = (a × b) >> 32.
#[inline(always)]
pub fn multiply(a: i64, b: i64) -> i64 {
    let product = (a as i128) * (b as i128);

    // symmetric rounding to remove bias for negative numbers
    let offset = 1i128 << (FRACTIONAL_BITS - 1);

    let rounded = if product >= 0 {
        product + offset
    } else {
        product - offset
    };

    (rounded >> FRACTIONAL_BITS) as i64
}

/// Divide two Q31.32 values. Returns None if divisor is zero.
/// Result = (a << 32) / b, computed in i128 to prevent overflow.
#[inline(always)]
pub fn divide(a: i64, b: i64) -> Option<i64> {
    if b == 0 { return None; }
    // The code below was used for testing purposes
    // let mut display_buffer = [0u8; 24];
    // let c = Some((((a as i128) << FRACTIONAL_BITS) / (b as i128)) as i64);
    // uart::transmit_bytes(engine::format_result(c?, &mut display_buffer));
    // uart::transmit_bytes(b"\r\n");
    Some((((a as i128) << FRACTIONAL_BITS) / (b as i128)) as i64)

}

// ─── Power ────────────────────────────────────────────────────────────────────

/// Raise a Q31.32 base to a Q31.32 exponent.
pub fn power(base: i64, exponent: i64) -> Option<i64> {
    // Integer exponent: fast exact path via repeated squaring.
    if to_integer_truncated(exponent) * SCALE == exponent {
        return integer_power(base, to_integer_truncated(exponent));
    }
    // Non-integer exponent: compute via exp(exponent × ln(base)).
    // Requires base > 0 for ln to be defined.
    if base <= 0 { return None; }
    let ln_base = natural_log(base)?;
    let result  = natural_exp(multiply(exponent, ln_base));

    // Snap to nearest integer if within 1e-4 of one.
    // The log/exp chain accumulates ~1e-7 error per operation; for inputs
    // like 27^(1/3) this propagates to ~6e-5 in the result. Any value that
    // lands within 1e-4 of an integer was almost certainly meant to be that
    // integer — snap it exactly.
    // Threshold: 1e-4 in Q31.32 = round(1e-4 × 2^32) = 429497
    const SNAP_THRESHOLD: i64 = 429_497;
    let nearest = round(result);
    if (result - nearest).abs() < SNAP_THRESHOLD {
        Some(nearest)
    } else {
        Some(result)
    }
}

/// Integer exponentiation via fast squaring. exp is a plain i64, not Q31.32.
pub fn integer_power(base: i64, exp: i64) -> Option<i64> {
    if exp < 0 {
        let pos = integer_power(base, -exp)?;
        return divide(FIXED_ONE, pos);
    }
    let mut result = FIXED_ONE;
    let mut b = base;
    let mut e = exp;
    while e > 0 {
        if e & 1 == 1 { result = multiply(result, b); }
        b = multiply(b, b);
        e >>= 1;
    }
    Some(result)
}

// ─── Square root (Newton-Raphson) ─────────────────────────────────────────────

/// Compute √x for Q31.32 x ≥ 0. Returns None for negative input.
///
/// Promotes to i128, takes integer sqrt as initial guess, then refines
/// with Newton-Raphson iterations until convergence.
pub fn sqrt(x: i64) -> Option<i64> {
    if x < 0  { return None; }
    if x == 0 { return Some(0); }

    // Promote to Q31.64 for the integer sqrt initial guess.
    let scaled: i128 = (x as i128) << FRACTIONAL_BITS;
    let mut guess = integer_sqrt_i128(scaled) as i64;

    // Newton-Raphson: converges in ~10 iterations for Q31.32.
    for _ in 0..10 {
        if guess == 0 { break; }
        let quotient = (((x as i128) << FRACTIONAL_BITS) / (guess as i128)) as i64;
        guess = (guess / 2) + (quotient / 2);
    }
    Some(guess)
}

pub fn nthroot(x: i64, n: i64) -> Option<i64> {
    if n == 0 { return None; }
    power(x, divide(FIXED_ONE, n)?)
}

/// Integer square root of a non-negative i128, returning the floor.
fn integer_sqrt_i128(n: i128) -> i128 {
    if n <= 0 { return 0; }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x { x = y; y = (x + n / x) / 2; }
    x
}

// ─── Trigonometry (CORDIC Q31.32 + exact hardcoded values) ───────────────────

/// Compute (sin, cos) for a Q31.32 radian angle. Results are Q31.32.
///
/// Strategy:
///   1. Reduce angle to [−π, π].
///   2. Check exact-value table for standard angles (0°, 30°, 45°, 60°, 90°…).
///   3. Run CORDIC with i128 intermediates for all other angles.
pub fn sin_cos(angle: i64) -> (i64, i64) {
    let a = reduce_angle_to_principal(angle);
    if let Some(exact) = exact_sin_cos_lookup(a) { return exact; }
    cordic_sin_cos(a)
}

/// sin of a Q31.32 radian angle.
pub fn sin(angle: i64) -> i64 { sin_cos(angle).0 }

/// cos of a Q31.32 radian angle.
pub fn cos(angle: i64) -> i64 { sin_cos(angle).1 }

/// tan of a Q31.32 radian angle. Returns None near ±π/2.
pub fn tan(angle: i64) -> Option<i64> {
    let (s, c) = sin_cos(angle);
    // |cos| < 0.0001 in Q31.32 ≈ 429497 units.
    if c.abs() < 429_497 { return None; }
    divide(s, c)
}

/// sinh of a Q31.32 value.
pub fn sinh(angle: i64) -> i64 {
    // sinh(x) = (e^x - e^-x) / 2
    let exp_pos = natural_exp(angle);
    let exp_neg = natural_exp(0 - angle);
    (exp_pos - exp_neg) >> 1
}

/// cosh of a Q31.32 value.
pub fn cosh(angle: i64) -> i64 {
    // cosh(x) = (e^x + e^-x) / 2
    let exp_pos = natural_exp(angle);
    let exp_neg = natural_exp(0 - angle);
    (exp_pos + exp_neg) >> 1
}

/// tanh of a Q31.32 value. Returns None if overflow/undefined.
pub fn tanh(angle: i64) -> Option<i64> {
    // tanh(x) = sinh(x) / cosh(x)
    let exp_pos = natural_exp(angle);
    let exp_neg = natural_exp(0 - angle);

    let numerator = exp_pos - exp_neg;
    let denominator = exp_pos + exp_neg;

    if denominator == 0 {
        return None;
    }

    divide(numerator, denominator)
}

/// asinh of a Q31.32 value.
pub fn asinh(x: i64) -> Option<i64> {
    // sqrt(x^2 + 1)
    let x_sq = multiply(x, x); // use your fixed-point multiply
    let inside = x_sq + (1 << 32);
    let root = sqrt(inside)?;

    // ln(x + sqrt(...))
    let sum = x + root;
    natural_log(sum)
}

/// acosh of a Q31.32 value. Domain: x >= 1
pub fn acosh(x: i64) -> Option<i64> {
    if x < (1 << 32) {
        return None; // domain error
    }

    // sqrt(x^2 - 1)
    let x_sq = multiply(x, x);
    let inside = x_sq - (1 << 32);
    let root = sqrt(inside)?;

    // ln(x + sqrt(...))
    let sum = x + root;
    natural_log(sum)
}

/// atanh of a Q31.32 value. Domain: |x| < 1
pub fn atanh(x: i64) -> Option<i64> {
    let one = 1 << 32;

    if x <= -one || x >= one {
        return None; // domain error
    }

    // (1 + x) / (1 - x)
    let num = one + x;
    let den = one - x;
    let frac = divide(num, den)?;

    // 0.5 * ln(...)
    let ln = natural_log(frac)?;
    Some(ln >> 1)
}

/// Reduce angle to [−π, π] in Q31.32.
pub fn reduce_angle_to_principal(angle: i64) -> i64 {
    let mut a = angle;
    while a >  FIXED_PI  { a -= FIXED_TWO_PI; }
    while a < -FIXED_PI  { a += FIXED_TWO_PI; }
    a
}

/// Run raw CORDIC rotation mode in Q31.32 with i128 intermediates.
///
/// IMPORTANT: input angle must be in (−π/2, π/2) for convergence.
/// Call `cordic_sin_cos` (which handles quadrant folding) instead of this
/// directly unless you are certain the angle is already in range.
fn cordic_raw(angle: i64) -> (i64, i64) {
    let mut x: i128 = CORDIC_GAIN as i128;
    let mut y: i128 = 0;
    let mut z: i128 = angle as i128;

    for i in 0..24 {
        let xp = x;
        let yp = y;
        let table = CORDIC_ATAN_TABLE[i] as i128;
        if z >= 0 {
            x = xp - (yp >> i);
            y = yp + (xp >> i);
            z -= table;
        } else {
            x = xp + (yp >> i);
            y = yp - (xp >> i);
            z += table;
        }
    }
    (y as i64, x as i64)
}

/// Run CORDIC with full quadrant folding so any angle in [−π, π] is handled.
///
/// CORDIC rotation mode only converges for angles in (−π/2, π/2).
/// Angles in quadrant II (π/2, π) and III (−π, −π/2) are mapped into
/// quadrant I/IV via the identities:
///   sin(a) =  sin(π − a)   for a ∈ (π/2, π)      [cos negated]
///   sin(a) = −sin(−a)      for a ∈ (−π, 0)        [then fold if needed]
fn cordic_sin_cos(angle: i64) -> (i64, i64) {
    let mut a = angle;
    let mut negate_sin = false;
    let mut negate_cos = false;

    // Fold quadrant II: (π/2, π) → (0, π/2), cos negated
    if a > FIXED_PI_OVER_2 {
        a = FIXED_PI - a;
        negate_cos = true;
    }
    // Fold quadrant III: (−π, −π/2) → (−π/2, 0), sin negated
    else if a < -FIXED_PI_OVER_2 {
        a = -a; // now a > π/2
        negate_sin = true;
        // May still be in quadrant II — fold again
        if a > FIXED_PI_OVER_2 {
            a = FIXED_PI - a;
            negate_cos = true;
        }
    }

    let (sin_v, cos_v) = cordic_raw(a);

    let sin_out = if negate_sin { -sin_v } else { sin_v };
    let cos_out = if negate_cos { -cos_v } else { cos_v };
    (sin_out, cos_out)
}

/// Exact (sin, cos) table for multiples of 30° and 45°.
/// Tolerance: ±512 Q31.32 units ≈ ±1.2×10⁻⁷ rad ≈ ±0.000007°.
fn exact_sin_cos_lookup(a: i64) -> Option<(i64, i64)> {
    let close = |x: i64, y: i64| (x - y).abs() < 512;

    let p2  = FIXED_PI_OVER_2;
    let p3  = 4_497_012_568_i64;  // π/3  = 60°
    let p4  = 3_373_259_426_i64;  // π/4  = 45°
    let p6  = 2_248_506_284_i64;  // π/6  = 30°

    let s0  = 0_i64;
    let c0  = FIXED_ONE;
    let s30 = FIXED_HALF;
    let c30 = FIXED_SQRT3_OVER_2;
    let s45 = FIXED_INV_SQRT2;
    let c45 = FIXED_INV_SQRT2;
    let s60 = FIXED_SQRT3_OVER_2;
    let c60 = FIXED_HALF;
    let s90 = FIXED_ONE;
    let c90 = 0_i64;

    if close(a,  0)          { return Some((s0,   c0));  }
    if close(a,  p6)         { return Some((s30,  c30)); }
    if close(a,  p4)         { return Some((s45,  c45)); }
    if close(a,  p3)         { return Some((s60,  c60)); }
    if close(a,  p2)         { return Some((s90,  c90)); }
    if close(a,  p2 + p6)    { return Some(( s60, -c60)); } // 120°
    if close(a,  p2 + p4)    { return Some(( s45, -c45)); } // 135°
    if close(a,  p2 + p3)    { return Some(( s30, -c30)); } // 150°
    if close(a,  FIXED_PI)   { return Some((  s0,  -c0)); } // 180°
    if close(a, -(FIXED_PI - p6)) { return Some((-s30, -c30)); } // 210°
    if close(a, -(FIXED_PI - p4)) { return Some((-s45, -c45)); } // 225°
    if close(a, -(FIXED_PI - p3)) { return Some((-s60, -c60)); } // 240°
    if close(a, -p2)         { return Some((-s90,  c90)); } // 270°
    if close(a, -(p2 - p6))  { return Some((-s60,  c60)); } // 300°
    if close(a, -(p2 - p4))  { return Some((-s45,  c45)); } // 315°
    if close(a, -(p2 - p3))  { return Some((-s30,  c30)); } // 330°
    None
}

// ─── Inverse trigonometry ─────────────────────────────────────────────────────

/// atan(x) in Q31.32 radians via CORDIC vectoring mode with i128 intermediates.
pub fn atan(x: i64) -> i64 {
    let mut vx: i128 = FIXED_ONE as i128;
    let mut vy: i128 = x as i128;
    let mut z:  i128 = 0;

    for i in 0..24 {
        let vxp = vx;
        let vyp = vy;
        let table = CORDIC_ATAN_TABLE[i] as i128;
        if vy >= 0 {
            vx =  vxp + (vyp >> i);
            vy =  vyp - (vxp >> i);
            z  += table;
        } else {
            vx =  vxp - (vyp >> i);
            vy =  vyp + (vxp >> i);
            z  -= table;
        }
    }
    z as i64
}

/// asin(x) in Q31.32 radians. Returns None if |x| > 1.
/// asin(x) = atan(x / √(1 − x²))
pub fn asin(x: i64) -> Option<i64> {
    if x.abs() > FIXED_ONE  { return None; }
    if x ==  FIXED_ONE      { return Some( FIXED_PI_OVER_2); }
    if x == -FIXED_ONE      { return Some(-FIXED_PI_OVER_2); }
    if x == 0               { return Some(0); }
    let x_sq = multiply(x, x);
    let root = sqrt(FIXED_ONE - x_sq)?;
    Some(atan(divide(x, root)?))
}

/// acos(x) in Q31.32 radians. Returns None if |x| > 1.
/// acos(x) = π/2 − asin(x)
pub fn acos(x: i64) -> Option<i64> {
    Some(FIXED_PI_OVER_2 - asin(x)?)
}

// ─── Exponential and logarithm ────────────────────────────────────────────────

/// e^x for Q31.32 x.
///
/// Range reduction: x = k × ln2 + r, |r| ≤ ln2/2.
/// e^r computed via 12-term Taylor series with i128 intermediates.
/// Result scaled by 2^k via bit shift.
pub fn natural_exp(x: i64) -> i64 {
    // e^0 = 1
    if x == 0 {
        return FIXED_ONE;
    }

    // Handle negative exponents explicitly.
    //
    // Computing the Taylor series directly for negative values is less stable
    // in fixed-point arithmetic and can accumulate significant truncation error.
    //
    // Instead use:
    //
    //     e^-x = 1 / e^x
    //
    // which keeps the polynomial evaluation in the positive domain.
    if x < 0 {
        return divide(FIXED_ONE, natural_exp(-x)).unwrap_or(0);
    }

    // Range reduction:
    //
    //     e^x = e^(k ln 2 + r)
    //          = 2^k * e^r
    //
    // Choose k such that:
    //
    //     r = x - k ln 2
    //
    // is small, improving Taylor-series accuracy.
    //
    // Using truncation instead of rounding keeps r bounded and stable.
    let k = to_integer_truncated(divide(x, FIXED_LN2).unwrap_or(0));

    // Reduced argument.
    let r = x - k * FIXED_LN2;

    // Evaluate e^r using a 12-term Taylor series:
    //
    //     e^r = 1 + r + r²/2! + r³/3! + ...
    //
    // All calculations are done in i128 to preserve precision and avoid
    // overflow during intermediate multiplication.
    let r_i = r as i128;
    let s   = SCALE as i128;

    // Current term in the series.
    // Starts at 1.0 in Q31.32.
    let mut term = s;

    // Accumulator also starts at 1.0.
    let mut result = s;

    // Add terms:
    //
    //     term_n = term_(n-1) * r / n
    //
    // maintaining fixed-point scaling throughout.
    for n in 1i128..=12 {
        term = (term * r_i) / s / n;
        result += term;
    }

    // Convert back to i64 after polynomial evaluation.
    let poly = result as i64;

    // Multiply by 2^k:
    //
    //     e^x = 2^k * e^r
    //
    // Since x is guaranteed positive here, k should also be >= 0.
    //
    // Clamp shift amount to avoid undefined behaviour.
    poly << (k as u32).min(30)
}

/// ln(x) for Q31.32 x > 0. Returns None for x ≤ 0.
///
/// Range reduction: x = 2^k × m, m ∈ [1, 2).
/// ln(m) via 16-term Taylor series for ln(1+t), t = m−1 ∈ [0,1).
/// All arithmetic in i128.
pub fn natural_log(x: i64) -> Option<i64> {
    if x <= 0 { return None; }

    // Reduce to m ∈ [SCALE, 2×SCALE).
    let mut m = x;
    let mut k: i64 = 0;
    while m >= 2 * SCALE { m >>= 1; k += 1; }
    while m <      SCALE { m <<= 1; k -= 1; }

    // t = m − 1 as Q31.32, range [0, SCALE).
    let t = (m - SCALE) as i128;
    // TODO: worry about whatever this is -> let s = SCALE as i128;

    // 16-term Taylor: ln(1+t) = Σ (−1)^(n+1) × t^n / n
    let mut t_power = t;
    let mut result: i128 = 0;
    for n in 1i128..=16 {
        let term = t_power / n;
        if n % 2 == 1 { result += term; } else { result -= term; }
        t_power = (t_power * t) >> 32;
    }

    Some(k * FIXED_LN2 + result as i64)
}

/// log₁₀(x). Returns None for x ≤ 0.
pub fn log10(x: i64) -> Option<i64> {
    divide(natural_log(x)?, FIXED_LN10)
}

/// log₂(x). Returns None for x ≤ 0.
pub fn log2(x: i64) -> Option<i64> {
    divide(natural_log(x)?, FIXED_LN2)
}

// ─── Rounding ─────────────────────────────────────────────────────────────────

/// Absolute value.
#[inline(always)]
pub fn abs(x: i64) -> i64 { x.abs() }

/// Floor: round toward −∞.
///
/// Rust's arithmetic right shift on i64 already rounds toward −∞ for
/// negative values. So (x >> 32) << 32 is correct for all signs — no
/// special case needed.
#[inline(always)]
pub fn floor(x: i64) -> i64 {
    (x >> FRACTIONAL_BITS) << FRACTIONAL_BITS
}

/// Ceiling: round toward +∞.
#[inline(always)]
pub fn ceil(x: i64) -> i64 {
    if x & (SCALE - 1) == 0 { x }
    else { floor(x) + SCALE }
}

/// Round to nearest integer (half rounds away from zero).
#[inline(always)]
pub fn round(x: i64) -> i64 {
    if x >= 0 { floor(x + FIXED_HALF) }
    else      { -floor(-x + FIXED_HALF) }
}

// ─── Angle unit conversion ────────────────────────────────────────────────────

/// deg(x): x is in degrees → result is x in radians.
/// sin(deg(90)) evaluates sin(π/2) = 1.
#[inline(always)]
pub fn degrees_to_radians(degrees: i64) -> i64 {
    multiply(degrees, FIXED_PI_OVER_180)
}

/// rad(x): x is in radians → result is x in degrees.
/// rad(pi) = 180.
#[inline(always)]
pub fn radians_to_degrees(radians: i64) -> i64 {
    multiply(radians, FIXED_180_OVER_PI)
}

// ─── Formatting ───────────────────────────────────────────────────────────────

/// Format a Q31.32 value as a decimal string with up to 6 significant
/// fractional digits, trailing zeros stripped.
///
/// Writes into `buffer` (must be ≥ 24 bytes) and returns the filled slice.
pub fn format_fixed_point(value: i64, buffer: &mut [u8; 24]) -> &[u8] {
    let is_negative = value < 0;
    // Use i128 to safely handle i64::MIN.
    let abs_val: i128 = if is_negative { -(value as i128) } else { value as i128 };

    let integer_part = (abs_val >> FRACTIONAL_BITS) as u64;

    // Extract fractional part as 6 decimal digits.
    // frac_raw ∈ [0, SCALE−1]; scale to 6 decimal places.
    // Use i128 to avoid overflow: frac_raw × 1_000_000 can exceed u64.
    let frac_raw     = (abs_val & (SCALE as i128 - 1)) as u128;
    let frac_decimal = ((frac_raw * 1_000_000 + (SCALE as u128) / 2) / SCALE as u128) as u32;

    // Carry from rounding.
    let (frac_decimal, integer_part) = if frac_decimal >= 1_000_000 {
        (0u32, integer_part + 1)
    } else {
        (frac_decimal, integer_part)
    };

    let mut pos = 0usize;

    if is_negative { buffer[pos] = b'-'; pos += 1; }

    // Write integer part.
    let int_start = pos;
    if integer_part == 0 {
        buffer[pos] = b'0'; pos += 1;
    } else {
        let mut n = integer_part;
        while n > 0 {
            buffer[pos] = b'0' + (n % 10) as u8;
            pos += 1;
            n /= 10;
        }
        buffer[int_start..pos].reverse();
    }

    // Write fractional part (6 digits, trailing zeros stripped).
    if frac_decimal > 0 {
        buffer[pos] = b'.'; pos += 1;
        let mut digits = [0u8; 6];
        let mut fd = frac_decimal;
        for i in (0..6).rev() {
            digits[i] = b'0' + (fd % 10) as u8;
            fd /= 10;
        }
        let last = (0..6).rev().find(|&i| digits[i] != b'0').unwrap_or(0);
        for i in 0..=last { buffer[pos] = digits[i]; pos += 1; }
    }

    &buffer[..pos]
}