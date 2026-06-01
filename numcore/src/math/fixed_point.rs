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
//! 22 iterations + linear Taylor correction for rotation mode (sin/cos),
//! 22 iterations for vectoring mode (atan) — sufficient for 1e-6 error.
//!
//! ## deg(x) / rad(x) semantics
//!   `deg(x)` — x is in degrees → converts to radians  (sin(deg(90)) = 1)
//!   `rad(x)` — x is in radians → converts to degrees  (rad(pi) = 180)

// ─── Scale and precision ──────────────────────────────────────────────────────

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
/// 22 entries — 22 CORDIC iterations + linear Taylor correction for residual.
const CORDIC_ATAN_TABLE: [i64; 22] = [
    3_373_259_426, // atan(2^0)  = 0.78539816 rad
    1_991_351_318, // atan(2^-1) = 0.46364761 rad
    1_052_175_346, // atan(2^-2) = 0.24497866 rad
    534_100_635,   // atan(2^-3) = 0.12435499 rad
    268_086_748,   // atan(2^-4) = 0.06241881 rad
    134_174_063,   // atan(2^-5) = 0.03123983 rad
    67_103_403,    // atan(2^-6) = 0.01562373 rad
    33_553_749,    // atan(2^-7) = 0.00781234 rad
    16_777_131,    // atan(2^-8)
    8_388_597,     // atan(2^-9)
    4_194_303,     // atan(2^-10)
    2_097_152,     // atan(2^-11)
    1_048_576,     // atan(2^-12)
    524_288,       // atan(2^-13)
    262_144,       // atan(2^-14)
    131_072,       // atan(2^-15)
    65_536,        // atan(2^-16)
    32_768,        // atan(2^-17)
    16_384,        // atan(2^-18)
    8_192,         // atan(2^-19)
    4_096,         // atan(2^-20)
    2_048,         // atan(2^-21)
];

// ─── Rational minimax constants for atan (Ganssle-Homer form) ────────────────
//
// atan(r) = r * P(r^2) / Q(r^2),  r ∈ [0, 1]
// P(t) = p0 + p2·t + p4·t^2 + p6·t^3 + p8·t^4
// Q(t) = 1 + q2·t + q4·t^2 + q6·t^3 + q8·t^4
//
// Remez-optimised for [0, 1]; max error < 1.6e-10 after Q31.32 quantisation.

const ATAN_P0: i64 = SCALE;                     //  1.0000000000
const ATAN_P2: i64 = 8_660_121_455;             //  2.0163416525
const ATAN_P4: i64 = 5_210_237_323;             //  1.2131029095
const ATAN_P6: i64 = 928_627_796;               //  0.2162130075
const ATAN_P8: i64 = 22_578_749;                //  0.0052570247

const ATAN_Q2: i64 = 10_091_777_336;            //  2.3496750128
const ATAN_Q4: i64 = 7_715_167_325;             //  1.7963273742
const ATAN_Q6: i64 = 2_095_582_640;             //  0.4879158549
const ATAN_Q8: i64 = 142_430_693;               //  0.0331622299

// ─── Core arithmetic ──────────────────────────────────────────────────────────

/// Convert an integer to Q31.32.
#[inline(always)]
pub fn from_integer(n: i64) -> i64 {
    n * SCALE
}

/// Truncate a Q31.32 value to its integer part (round toward zero).
#[inline(always)]
pub fn to_integer_truncated(fp: i64) -> i64 {
    fp >> FRACTIONAL_BITS
}

/// Round a Q31.32 value to the nearest integer.
#[inline(always)]
pub fn to_integer_rounded(fp: i64) -> i64 {
    (fp + FIXED_HALF) >> FRACTIONAL_BITS
}

/// Multiply two Q31.32 values.  Returns `None` on overflow.
///
/// Result = (a × b) >> 32, computed in i128 to capture the full
/// Q31.64 product.  Symmetric rounding is applied before truncation.
/// If the Q31.32 result does not fit in i64, `None` is returned.
pub fn multiply(a: i64, b: i64) -> Option<i64> {
    let product = (a as i128) * (b as i128);
    let offset = 1i128 << (FRACTIONAL_BITS - 1);

    // Symmetric rounding (half away from zero) via absolute-value
    // arithmetic.  For positive products, (product + offset) >> 32 gives
    // truncation-toward-zero of the rounded result.  For negative we
    // negate to the positive domain, apply the same rounding, then
    // negate back — this avoids Rust's arithmetic right shift (>>)
    // which truncates toward -∞ and would give the wrong direction
    // when combined with a subtracted offset.
    let result = if product >= 0 {
        (product + offset) >> FRACTIONAL_BITS
    } else {
        let abs = product.wrapping_neg();
        (abs + offset) >> FRACTIONAL_BITS
    };

    if result > i64::MAX as i128 {
        return None;
    }
    if product >= 0 {
        Some(result as i64)
    } else {
        Some(-(result as i64))
    }
}

/// Divide two Q31.32 values. Returns None if divisor is zero.
/// Result = (a << 32) / b, computed in i128 to prevent overflow.
pub fn divide(a: i64, b: i64) -> Option<i64> {
    if b == 0 {
        return None;
    }
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
    if base <= 0 {
        return None;
    }
    let ln_base = natural_log(base)?;
    let result = natural_exp(multiply(exponent, ln_base)?)?;

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
        if e & 1 == 1 {
            result = multiply(result, b)?;
        }
        b = multiply(b, b)?;
        e >>= 1;
    }
    Some(result)
}

// ─── Reciprocal sqrt (CLZ initial guess + Newton–Raphson) ─────────────────────

/// 32-entry LUT for 1/√m where m ∈ [1, 2), midpoint of each 1/32 interval.
/// Precomputed via Python with numpy; max error ~2%.
const RSQRT_INIT_TABLE: [i64; 32] = [
    4_261_801_029, // 1/sqrt(1.015625)
    4_197_710_145, // 1/sqrt(1.046875)
    4_136_426_415, // 1/sqrt(1.078125)
    4_077_750_728, // 1/sqrt(1.109375)
    4_021_503_196, // 1/sqrt(1.140625)
    3_967_520_839, // 1/sqrt(1.171875)
    3_915_655_591, // 1/sqrt(1.203125)
    3_865_772_592, // 1/sqrt(1.234375)
    3_817_748_708, // 1/sqrt(1.265625)
    3_771_471_255, // 1/sqrt(1.296875)
    3_726_836_887, // 1/sqrt(1.328125)
    3_683_750_620, // 1/sqrt(1.359375)
    3_642_124_983, // 1/sqrt(1.390625)
    3_601_879_272, // 1/sqrt(1.421875)
    3_562_938_893, // 1/sqrt(1.453125)
    3_525_234_775, // 1/sqrt(1.484375)
    3_488_702_859, // 1/sqrt(1.515625)
    3_453_283_638, // 1/sqrt(1.546875)
    3_418_921_752, // 1/sqrt(1.578125)
    3_385_565_620, // 1/sqrt(1.609375)
    3_353_167_118, // 1/sqrt(1.640625)
    3_321_681_283, // 1/sqrt(1.671875)
    3_291_066_056, // 1/sqrt(1.703125)
    3_261_282_040, // 1/sqrt(1.734375)
    3_232_292_291, // 1/sqrt(1.765625)
    3_204_062_124, // 1/sqrt(1.796875)
    3_176_558_936, // 1/sqrt(1.828125)
    3_149_752_052, // 1/sqrt(1.859375)
    3_123_612_579, // 1/sqrt(1.890625)
    3_098_113_274, // 1/sqrt(1.921875)
    3_073_228_427, // 1/sqrt(1.953125)
    3_048_933_750, // 1/sqrt(1.984375)
];

/// CLZ-based initial approximation of 1/√x for Q31.32 x > 0.
///
/// Normalises x to m ∈ [1, 2), indexes the 32-entry LUT, then applies
/// exponent scaling.  Error < 2% across the full Q31.32 domain.
fn rsqrt_clz_initial(x: i64) -> i64 {
    let x_u = x as u64;
    let leading = x_u.leading_zeros();
    let p = 63 - leading;
    let e = p as i32 - 32;

    let m_i64 = if p >= 32 {
        x_u >> (p - 32)
    } else {
        x_u << (32 - p)
    };
    let idx = ((m_i64 >> 27) & 0x1F) as usize;
    let lut_val = RSQRT_INIT_TABLE[idx];

    if e >= 0 {
        if e & 1 == 0 {
            lut_val >> (e / 2)
        } else {
            multiply(lut_val, FIXED_INV_SQRT2).unwrap() >> ((e - 1) / 2)
        }
    } else {
        let neg_e = -e;
        if neg_e & 1 == 0 {
            lut_val << (neg_e / 2)
        } else {
            multiply(lut_val, FIXED_SQRT2).unwrap() << ((neg_e - 1) / 2)
        }
    }
}

/// Compute √x for Q31.32 x ≥ 0. Returns None for negative input.
///
/// Uses CLZ-based initial guess for 1/√x, 3 Newton–Raphson iterations
/// on the reciprocal square root, then one final Newton step refining
/// √x directly.
///
///     y_0     ← CLZ + 32-entry LUT (error < 2%)
///     y_{n+1} ← y_n · (3 − x·y_n²) / 2    (3 iterations)
///     s       ← x · y_3
///     s       ← (s + x / s) / 2            (final Newton refinement)
///
/// Error < 1e-6 relative (~3.7e-8 worst observed), ~250 cycles.
pub fn sqrt(x: i64) -> Option<i64> {
    if x < 0 {
        return None;
    }
    if x == 0 {
        return Some(0);
    }

    let three = 3 * SCALE;

    // CLZ initial guess for 1/√x.
    let mut y = rsqrt_clz_initial(x);

    // Three Newton–Raphson iterations on 1/√x.
    for _ in 0..3 {
        let y_sq = multiply(y, y)?;
        let x_y_sq = multiply(x, y_sq)?;
        let three_minus = three.checked_sub(x_y_sq)?;
        y = multiply(y, three_minus)? >> 1;
    }

    // s = x · y  (our first approximation of √x).
    let mut s = multiply(x, y)?;

    // One final Newton step refining sqrt directly:
    //   s ← (s + x / s) / 2
    s = (s.checked_add(divide(x, s)?)?) >> 1;

    Some(s)
}

/// Compute the integer nth root of x via Newton's method.
///
/// Uses the iteration:
///
///     x_{k+1} = ((n−1)·x_k + x / x_k^(n−1)) / n
///
/// which converges quadratically to x^(1/n).  n must be ≥ 2 and a
/// plain integer (not Q31.32).  x must be ≥ 0.
fn newton_nthroot(x: i64, n: i64) -> Option<i64> {
    // Initial guess: x / 2 in Q31.32, same as sqrt.
    let mut guess = x >> 1;
    if guess == 0 {
        guess = FIXED_ONE;
    }
    let n_fp = from_integer(n);
    let n_minus_1 = from_integer(n - 1);

    for _ in 0..12 {
        // x_k^(n−1) via integer fast squaring.
        let pow = integer_power(guess, n - 1)?;
        let quotient = divide(x, pow)?;
        // ((n−1)·x_k + x / x_k^(n−1)) / n
        let sum = multiply(n_minus_1, guess)?.checked_add(quotient)?;
        guess = divide(sum, n_fp)?;
    }
    Some(guess)
}

pub fn nthroot(x: i64, n: i64) -> Option<i64> {
    if n == 0 {
        return None;
    }

    // Handle negative n: x^(1/n) = 1 / x^(1/|n|).
    let n_neg = n < 0;
    if n_neg {
        let pos = nthroot(x, -n)?;
        return divide(FIXED_ONE, pos);
    }

    let n_is_integer = (n & (SCALE - 1)) == 0;
    let x_neg = x < 0;
    let x_zero = x == 0;

    if x_zero {
        return Some(0);
    }

    if n_is_integer {
        let n_int = to_integer_truncated(n);

        if n_int == 2 {
            // Square root: use the dedicated Newton implementation.
            if x_neg {
                return None;
            }
            return sqrt(x);
        }

        if n_int >= 3 {
            // Higher integer root: use general Newton iteration.
            // Newton converges faster and more accurately than the
            // exp(ln(x)/n) path used by power().
            let n_odd = (n_int & 1) != 0;

            if !n_odd && x_neg {
                return None; // even root of negative → domain error
            }
            if n_odd && x_neg {
                return newton_nthroot(-x, n_int).map(|v| -v);
            }
            return newton_nthroot(x, n_int);
        }

        // n_int == 1:  x^(1/1) = x
        Some(x)
    } else {
        // Non-integer root: must use exp(ln(x)/n).
        if x_neg || x_zero {
            return None;
        }
        power(x, divide(FIXED_ONE, n)?)
    }
}

// ─── Trigonometry (CORDIC Q31.32) ────────────────────────────────────────────

/// Compute (sin, cos) for a Q31.32 radian angle. Results are Q31.32.
///
/// First normalises the angle to the principal range [−π, π] via
/// `reduce_angle_to_principal`, then delegates to `cordic_sin_cos` which runs
/// CORDIC with i128 intermediates.
pub fn sin_cos(angle: i64) -> (i64, i64) {
    let a = reduce_angle_to_principal(angle);
    cordic_sin_cos(a)
}

/// sin of a Q31.32 radian angle.
pub fn sin(angle: i64) -> i64 {
    sin_cos(angle).0
}

/// cos of a Q31.32 radian angle.
pub fn cos(angle: i64) -> i64 {
    sin_cos(angle).1
}

/// |cos| below which tan(x) = sin/cos is rejected as too close to a pole.
///
/// 0.0001 in Q31.32 = round(0.0001 × 2^32) = 429_497.
///
/// At |cos| = 0.0001 the true tan magnitude is ~10 000.  The nearest
/// representable Q31.32 integer part is 2^31 − 1 ≈ 2.147 × 10^9, so
/// there is headroom — but the CORDIC sin/cos error (~1−2 ULP at 22
/// iterations + Taylor) means the computed cos could be slightly smaller or
/// even zero for inputs extremely close to ±π/2.  Picking 0.0001
/// gives a comfortable safety margin while still covering angles up
/// to ~89.994°.
const TAN_COS_MIN: i64 = 429_497;

/// tan of a Q31.32 radian angle. Returns None near ±π/2.
pub fn tan(angle: i64) -> Option<i64> {
    let (s, c) = sin_cos(angle);
    if c.abs() < TAN_COS_MIN {
        return None;
    }
    divide(s, c)
}

/// sinh of a Q31.32 value.  Returns `None` on overflow.
pub fn sinh(angle: i64) -> Option<i64> {
    let exp_pos = natural_exp(angle)?;
    let exp_neg = natural_exp(0 - angle)?;
    Some((exp_pos - exp_neg) >> 1)
}

/// cosh of a Q31.32 value.  Returns `None` on overflow.
pub fn cosh(angle: i64) -> Option<i64> {
    let exp_pos = natural_exp(angle)?;
    let exp_neg = natural_exp(0 - angle)?;
    Some((exp_pos + exp_neg) >> 1)
}

/// |x| beyond which tanh(x) rounds to ±1 in Q31.32 precision.
///
/// tanh(x) = 1 - 2/(e^(2x) + 1).  For |x| ≥ 12 the correction
/// term is < 2^-33 — below one Q31.32 ULP — so the result is
/// indistinguishable from ±1.0.  Bypassing the exponential avoids
/// spurious overflow for large arguments.
const TANH_SATURATION: i64 = 12 * SCALE;

/// tanh of a Q31.32 value. Returns None if overflow/undefined.
pub fn tanh(angle: i64) -> Option<i64> {
    // Saturation fast-path: tanh(≥12) = 1, tanh(≤-12) = -1 within Q31.32 ULP.
    if angle >= TANH_SATURATION {
        return Some(FIXED_ONE);
    }
    if angle <= -TANH_SATURATION {
        return Some(-FIXED_ONE);
    }

    // tanh(x) = sinh(x) / cosh(x)
    let exp_pos = natural_exp(angle)?;
    let exp_neg = natural_exp(0 - angle)?;

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
    let x_sq = multiply(x, x)?;
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
    let x_sq = multiply(x, x)?;
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

/// Reduce angle to [−π, π] in Q31.32 using modular arithmetic.
///
/// The while-loop approach (iteratively subtracting/adding TWO_PI)
/// requires O(|angle| / TWO_PI) iterations — for a 1e9 radian input
/// that is ~1.6e8 iterations, hanging a 12 MHz Cortex-M3 for seconds.
///
/// Instead we use the `%` operator which gives the remainder in
/// (−TWO_PI, TWO_PI) in a single division.  At most one conditional
/// add/subtract is then needed to bring the result into [−π, π].
pub fn reduce_angle_to_principal(angle: i64) -> i64 {
    let mut a = angle % FIXED_TWO_PI;
    if a > FIXED_PI {
        a -= FIXED_TWO_PI;
    } else if a < -FIXED_PI {
        a += FIXED_TWO_PI;
    }
    a
}

/// Run raw CORDIC rotation mode in Q31.32 with i128 intermediates.
///
/// IMPORTANT: input angle must be in (−π/2, π/2) for convergence.
/// Call `cordic_sin_cos` (which handles quadrant folding) instead of this
/// directly unless you are certain the angle is already in range.
///
/// Uses 22 CORDIC iterations (i = 0..21) plus a first-order Taylor correction
/// on the residual angle `z`.  The linear approximation
///   cos(θ) ≈ cos(θ₀) − sin(θ₀)·δ,   sin(θ) ≈ sin(θ₀) + cos(θ₀)·δ
/// with δ = z and (x, y) = (K·cos(θ₀), K·sin(θ₀)) reduces the worst-case error
/// from |δ| < 2⁻²¹ ≈ 4.77e-7 to O(δ²) < 2.3e-13 — well below 1e-6.
fn cordic_raw(angle: i64) -> (i64, i64) {
    if angle == 0 {
        return (0, FIXED_ONE);
    }
    let mut x: i128 = CORDIC_GAIN as i128;
    let mut y: i128 = 0;
    let mut z: i128 = angle as i128;

    for i in 0..22 {
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

    // First-order Taylor correction for residual angle δ = z.
    // cos(θ₀ + δ) ≈ cos(θ₀) − δ·sin(θ₀)
    // sin(θ₀ + δ) ≈ sin(θ₀) + δ·cos(θ₀)
    // In Q31.32:  x_final ≈ x − y·δ,  y_final ≈ y + x·δ
    let delta = z;
    let ty = (y * delta) >> FRACTIONAL_BITS;
    let tx = (x * delta) >> FRACTIONAL_BITS;
    let x_out = x - ty;
    let y_out = y + tx;

    (y_out as i64, x_out as i64)
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

// ─── Inverse trigonometry ─────────────────────────────────────────────────────

/// atan(x) in Q31.32 radians via rational minimax (Ganssle-Homer form).
///
/// For |x| ≤ 1 evaluates the rational polynomial directly; for |x| > 1 uses
/// the identity  atan(x) = π/2 − atan(1/x).  Max error < 1.6e-10 rad.
pub fn atan(x: i64) -> i64 {
    if x == 0 {
        return 0;
    }

    let negative = x < 0;
    let ax = if negative { x.wrapping_neg() } else { x };

    // For |x| > 1, reduce via atan(x) = π/2 − atan(1/x).
    let (r, inverted) = if ax > FIXED_ONE {
        (divide(FIXED_ONE, ax).unwrap_or(0), true)
    } else {
        (ax, false)
    };

    // t = r²   (both in [0, 1] in Q31.32)
    let t = multiply(r, r).unwrap();

    // Horner's method:  P(t) = (((p8·t + p6)·t + p4)·t + p2)·t + p0
    let p = multiply(
        multiply(
            multiply(
                multiply(ATAN_P8, t).unwrap() + ATAN_P6, t,
            ).unwrap() + ATAN_P4, t,
        ).unwrap() + ATAN_P2, t,
    ).unwrap() + ATAN_P0;

    // Q(t) = (((q8·t + q6)·t + q4)·t + q2)·t + 1
    let q = multiply(
        multiply(
            multiply(
                multiply(ATAN_Q8, t).unwrap() + ATAN_Q6, t,
            ).unwrap() + ATAN_Q4, t,
        ).unwrap() + ATAN_Q2, t,
    ).unwrap() + FIXED_ONE;

    // atan(r) = r · P / Q
    let frac = divide(p, q).unwrap();
    let mut angle = multiply(r, frac).unwrap();

    if inverted {
        angle = FIXED_PI_OVER_2 - angle;
    }

    if negative { -angle } else { angle }
}

/// Four-quadrant arctangent atan2(y, x) in Q31.32 radians.
/// Returns angle in [-pi, pi].
pub fn atan2(y: i64, x: i64) -> i64 {
    if x == 0 {
        if y > 0 {
            return FIXED_PI_OVER_2;
        }
        if y < 0 {
            return -FIXED_PI_OVER_2;
        }
        return 0;
    }
    // Use the smaller of |y/x| or |x/y| to avoid divide overflow on steep slopes.
    let (small, large) = if y.abs() > x.abs() { (x, y) } else { (y, x) };
    let ratio = divide(small, large).unwrap_or(0);
    let mut angle = atan(ratio);
    if y.abs() > x.abs() {
        // |y| > |x|: atan2(y,x) = sign(y/x) * π/2 - atan(x/y)
        // sign(y/x) = sign(y) * sign(x) determines the base formula.
        if (y > 0) == (x > 0) {
            angle = FIXED_PI_OVER_2 - angle;
        } else {
            angle = -FIXED_PI_OVER_2 - angle;
        }
        if x < 0 {
            if y >= 0 {
                angle += FIXED_PI;
            } else {
                angle -= FIXED_PI;
            }
        }
    } else {
        if x < 0 {
            if y >= 0 {
                angle += FIXED_PI;
            } else {
                angle -= FIXED_PI;
            }
        }
    }
    angle
}

/// asin(x) in Q31.32 radians. Returns None if |x| > 1.
/// asin(x) = atan(x / √(1 − x²))
pub fn asin(x: i64) -> Option<i64> {
    if x.abs() > FIXED_ONE {
        return None;
    }
    if x == FIXED_ONE {
        return Some(FIXED_PI_OVER_2);
    }
    if x == -FIXED_ONE {
        return Some(-FIXED_PI_OVER_2);
    }
    if x == 0 {
        return Some(0);
    }
    let x_sq = multiply(x, x)?;
    let root = sqrt(FIXED_ONE - x_sq)?;
    Some(atan(divide(x, root)?))
}

/// acos(x) in Q31.32 radians. Returns None if |x| > 1.
/// acos(x) = π/2 − asin(x)
pub fn acos(x: i64) -> Option<i64> {
    Some(FIXED_PI_OVER_2 - asin(x)?)
}

// ─── Exponential and logarithm ────────────────────────────────────────────────

/// Maximum safe k = floor(log2(i64::MAX / SCALE)) = 30.
///
/// For k > 30 the result 2^k × e^r exceeds the maximum representable
/// Q31.32 value (~2.147 × 10⁹).  Return `None` instead of truncating
/// the shift amount and producing a silently wrong result.
const MAX_EXP_SHIFT: i64 = 30;

/// Largest positive x such that exp(x) does not overflow Q31.32.
///
/// k = trunc(x / ln2).  For k ≤ 30 we need x < 31 × ln2 ≈ 21.49.
/// Any argument larger than this will be caught by the k ≤ MAX_EXP_SHIFT
/// check, but failing early here avoids unnecessary recursion (and an
/// expensive divide) in the negative-x path.
const MAX_POS_EXP_ARG: i64 = (MAX_EXP_SHIFT + 1) * FIXED_LN2 - 1;

// ─── Minimax polynomial coefficients ──────────────────────────────────────────

/// Degree-7 minimax polynomial for e^r on r ∈ [0, ln2) (truncation range).
/// Chebyshev approximation; max error ~5.95×10⁻¹¹ (< 10⁻⁶).
const EXP_C0: i64 = 4294967296; //  1.0000000000
const EXP_C1: i64 = 4294967340; //  1.0000000102
const EXP_C2: i64 = 2147482330; //  0.4999996930
const EXP_C3: i64 = 715843011;  //  0.1666701890
const EXP_C4: i64 = 178872053;  //  0.0416468953
const EXP_C5: i64 = 36048604;   //  0.0083932197
const EXP_C6: i64 = 5538864;    //  0.0012896172
const EXP_C7: i64 = 1209186;    //  0.0002815356

/// Degree-10 minimax polynomial for ln(1+t) on t ∈ [√½−1, √2−1].
/// Computed as t × p(t) where p(t) ≈ ln(1+t)/t (degree 9).
/// Forces exact zero at t=0 (ln(1) = 0).
/// Max error ~1.62×10⁻⁹ (< 10⁻⁶).
const LOG_C0: i64 = 0;           //  0.0000000000
const LOG_C1: i64 = 4294967293;  //  0.9999999994
const LOG_C2: i64 = -2147483154; // -0.4999998850
const LOG_C3: i64 = 1431656281;  //  0.3333334535
const LOG_C4: i64 = -1073809646; // -0.2500157909
const LOG_C5: i64 = 859033808;   //  0.2000093944
const LOG_C6: i64 = -713324035;  // -0.1660836941
const LOG_C7: i64 = 609870329;   //  0.1419965013
const LOG_C8: i64 = -569771710;  // -0.1326603139
const LOG_C9: i64 = 550039721;   //  0.1280661024
const LOG_C10: i64 = -320025997; // -0.0745118588

/// e^x for Q31.32 x.
///
/// Range reduction: x = k × ln2 + r, |r| ≤ ln2/2.
/// e^r computed via degree-6 minimax polynomial in Horner form.
/// Result scaled by 2^k via bit shift.
///
/// Returns `None` if the result overflows Q31.32 (x > ~21.5).
/// Returns `Some(0)` for underflow (x < -21.5).
pub fn natural_exp(x: i64) -> Option<i64> {
    if x == 0 {
        return Some(FIXED_ONE);
    }

    if x < 0 {
        if x < -MAX_POS_EXP_ARG {
            return Some(0);
        }
        let pos = natural_exp(-x)?;
        return divide(FIXED_ONE, pos);
    }

    let k = to_integer_truncated(divide(x, FIXED_LN2).unwrap_or(0));

    if k < 0 || k > MAX_EXP_SHIFT {
        return None;
    }

    let r = x - k * FIXED_LN2;

    // Evaluate e^r via Horner with degree-7 minimax polynomial.
    let r_i = r as i128;
    let s = SCALE as i128;

    let mut result = EXP_C7 as i128;
    result = EXP_C6 as i128 + result * r_i / s;
    result = EXP_C5 as i128 + result * r_i / s;
    result = EXP_C4 as i128 + result * r_i / s;
    result = EXP_C3 as i128 + result * r_i / s;
    result = EXP_C2 as i128 + result * r_i / s;
    result = EXP_C1 as i128 + result * r_i / s;
    result = EXP_C0 as i128 + result * r_i / s;
    let poly = result as i64;

    Some(poly << k)
}

/// ln(x) for Q31.32 x > 0. Returns None for x ≤ 0.
///
/// Range reduction: x = 2^k × m, m ∈ [1/√2, √2) so that
/// t = m−1 is bounded to |t| ≤ √2−1 ≈ 0.414, guaranteeing fast
/// polynomial convergence.
/// ln(m) via degree-10 minimax polynomial in Horner form.
/// All arithmetic in i128.
pub fn natural_log(x: i64) -> Option<i64> {
    if x <= 0 {
        return None;
    }

    let mut m = x;
    let mut k: i64 = 0;
    while m >= 2 * SCALE {
        m >>= 1;
        k += 1;
    }
    while m < SCALE {
        m <<= 1;
        k -= 1;
    }

    if m > FIXED_SQRT2 {
        m >>= 1;
        k += 1;
    }

    let t = (m - SCALE) as i128;
    let s = SCALE as i128;

    // Evaluate ln(1+t) via Horner with degree-10 minimax polynomial.
    let mut result = LOG_C10 as i128;
    result = LOG_C9 as i128 + result * t / s;
    result = LOG_C8 as i128 + result * t / s;
    result = LOG_C7 as i128 + result * t / s;
    result = LOG_C6 as i128 + result * t / s;
    result = LOG_C5 as i128 + result * t / s;
    result = LOG_C4 as i128 + result * t / s;
    result = LOG_C3 as i128 + result * t / s;
    result = LOG_C2 as i128 + result * t / s;
    result = LOG_C1 as i128 + result * t / s;
    result = LOG_C0 as i128 + result * t / s;
    let poly = result as i64;

    Some(k * FIXED_LN2 + poly)
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
pub fn abs(x: i64) -> i64 {
    x.abs()
}

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
    if x & (SCALE - 1) == 0 {
        x
    } else {
        floor(x) + SCALE
    }
}

/// Round to nearest integer (half rounds away from zero).
#[inline(always)]
pub fn round(x: i64) -> i64 {
    if x >= 0 {
        floor(x + FIXED_HALF)
    } else {
        -floor(-x + FIXED_HALF)
    }
}

// ─── Angle unit conversion ────────────────────────────────────────────────────

/// deg(x): x is in degrees → result is x in radians.
/// sin(deg(90)) evaluates sin(π/2) = 1.
#[inline(always)]
pub fn degrees_to_radians(degrees: i64) -> Option<i64> {
    multiply(degrees, FIXED_PI_OVER_180)
}

/// rad(x): x is in radians → result is x in degrees.
/// rad(pi) = 180.
#[inline(always)]
pub fn radians_to_degrees(radians: i64) -> Option<i64> {
    multiply(radians, FIXED_180_OVER_PI)
}

// ─── Formatting ───────────────────────────────────────────────────────────────

/// Format a Q31.32 value as a decimal string with up to 6 significant
/// fractional digits, trailing zeros stripped.
///
/// Writes into `buffer` (must be ≥ 24 bytes) and returns the filled slice.
pub fn format_fixed_point(value: i64, buffer: &mut [u8]) -> &[u8] {
    let is_negative = value < 0;
    // Use i128 to safely handle i64::MIN.
    let abs_val: i128 = if is_negative {
        -(value as i128)
    } else {
        value as i128
    };

    let integer_part = (abs_val >> FRACTIONAL_BITS) as u64;

    // Extract fractional part as 6 decimal digits.
    // frac_raw ∈ [0, SCALE−1]; scale to 6 decimal places.
    // Use i128 to avoid overflow: frac_raw × 1_000_000 can exceed u64.
    let frac_raw = (abs_val & (SCALE as i128 - 1)) as u128;
    let frac_decimal = ((frac_raw * 1_000_000 + (SCALE as u128) / 2) / SCALE as u128) as u32;

    // Carry from rounding.
    let (frac_decimal, integer_part) = if frac_decimal >= 1_000_000 {
        (0u32, integer_part + 1)
    } else {
        (frac_decimal, integer_part)
    };

    // If the value rounds to exactly zero, suppress the negative sign.
    let is_negative = is_negative && !(integer_part == 0 && frac_decimal == 0);

    let mut pos = 0usize;

    if is_negative {
        buffer[pos] = b'-';
        pos += 1;
    }

    // Write integer part.
    let int_start = pos;
    if integer_part == 0 {
        buffer[pos] = b'0';
        pos += 1;
    } else {
        let mut n = integer_part;
        while n > 0 {
            buffer[pos] = b'0' + (n % 10) as u8;
            pos += 1;
            n /= 10;
        }
        buffer[int_start..pos].reverse();
    }

    // Write fractional part — up to 6 digits, trailing zeros stripped.
    if frac_decimal > 0 {
        buffer[pos] = b'.';
        pos += 1;
        let mut digits = [0u8; 6];
        let mut fd = frac_decimal;
        for i in (0..6).rev() {
            digits[i] = b'0' + (fd % 10) as u8;
            fd /= 10;
        }
        let last = (0..6).rev().find(|&i| digits[i] != b'0').unwrap_or(0);
        for i in 0..=last {
            buffer[pos] = digits[i];
            pos += 1;
        }
    }

    &buffer[..pos]
}
