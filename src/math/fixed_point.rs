//! # Fixed-Point Arithmetic — Q20.12
//!
//! All numbers in the math engine are represented as Q20.12 fixed-point values:
//!   - Stored as a signed 32-bit integer
//!   - The bottom 12 bits are the fractional part (precision = 1/4096 ≈ 0.000244)
//!   - The top 20 bits are the integer part (range: −524288 to +524287)
//!
//! ## Why Q20.12 and not floats?
//!   The Cortex-M3 has no FPU. Software floats work but cost ~4 KB of code.
//!   Q20.12 gives ~6 significant decimal digits, covers the full calculator
//!   range, and costs zero extra code size beyond basic integer arithmetic.
//!
//! ## Scale factor
//!   Every stored value equals its true mathematical value × 4096.
//!   Example: the number 1.5 is stored as 1.5 × 4096 = 6144.
//!
//! ## Operations
//!   Add/subtract: trivial (same as i32)
//!   Multiply: (a × b) / 4096  — need i64 intermediate to avoid overflow
//!   Divide:   (a × 4096) / b  — need i64 intermediate

/// The number of fractional bits. Changing this constant changes the entire
/// numeric format — all derived constants below update automatically.
pub const FRACTIONAL_BITS: u32 = 12;

/// The scale factor: 2^FRACTIONAL_BITS = 4096.
pub const SCALE: i32 = 1 << FRACTIONAL_BITS;

/// Q20.12 representation of the constant π.
pub const FIXED_PI: i32 = 12868; // 3.14159265 × 4096 ≈ 12868

/// Q20.12 representation of π/2.
pub const FIXED_PI_OVER_2: i32 = 6434; // 1.5707963 × 4096 ≈ 6434

/// Q20.12 representation of 2π.
pub const FIXED_TWO_PI: i32 = 25736; // 6.2831853 × 4096 ≈ 25736

/// Q20.12 representation of Euler's number e.
pub const FIXED_E: i32 = 11134; // 2.71828182 × 4096 ≈ 11134

/// Q20.12 representation of ln(2), used in log/exp computations.
pub const FIXED_LN2: i32 = 2839; // 0.69314718 × 4096 ≈ 2839

/// Q20.12 representation of 1.0.
pub const FIXED_ONE: i32 = SCALE; // 4096

/// Q20.12 representation of 0.5.
pub const FIXED_HALF: i32 = SCALE / 2; // 2048

/// Q20.12 representation of 180/π, used for degree↔radian conversion.
pub const FIXED_180_OVER_PI: i32 = (FIXED_ONE * 180) / FIXED_PI; // 57.29577951 × 4096 ≈ 235099

/// Q20.12 representation of π/180.
pub const FIXED_PI_OVER_180: i32 = FIXED_PI / 180; // 0.01745329 × 4096 ≈ 71

// ─── Core arithmetic ──────────────────────────────────────────────────────────

/// Convert an integer literal into Q20.12 fixed-point.
#[inline(always)]
pub fn from_integer(n: i32) -> i32 {
    n * SCALE
}

/// Convert a Q20.12 fixed-point value to the nearest integer (truncates).
#[inline(always)]
pub fn to_integer_truncated(fp: i32) -> i32 {
    fp >> FRACTIONAL_BITS
}

/// Convert a Q20.12 fixed-point value to the nearest integer (rounds).
#[inline(always)]
pub fn to_integer_rounded(fp: i32) -> i32 {
    (fp + FIXED_HALF) >> FRACTIONAL_BITS
}

/// Multiply two Q20.12 values. Uses i64 intermediate to prevent overflow.
///
/// Result: (a × b) >> 12
#[inline(always)]
pub fn multiply(a: i32, b: i32) -> i32 {
    (((a as i64) * (b as i64)) >> FRACTIONAL_BITS) as i32
}

/// Divide two Q20.12 values. Returns None if divisor is zero.
///
/// Result: (a << 12) / b
#[inline(always)]
pub fn divide(a: i32, b: i32) -> Option<i32> {
    if b == 0 { return None; }
    Some((((a as i64) << FRACTIONAL_BITS) / (b as i64)) as i32)
}

/// Raise a Q20.12 base to a Q20.12 exponent power.
/// Uses exp(exponent × ln(base)) for non-integer exponents.
/// Falls back to integer exponentiation for integer exponents.
pub fn power(base: i32, exponent: i32) -> Option<i32> {
    // If exponent is an integer, use fast integer path.
    if exponent & (SCALE - 1) == 0 {
        let int_exp = to_integer_truncated(exponent);
        return integer_power(base, int_exp);
    }
    // General case: e^(exp × ln(base))
    // base must be positive for ln to be defined.
    if base <= 0 { return None; }
    let ln_base = natural_log(base)?;
    let product = multiply(exponent, ln_base);
    Some(natural_exp(product))
}

/// Integer exponentiation: base^exp where exp is a plain i32 (not fixed-point).
pub fn integer_power(base: i32, exp: i32) -> Option<i32> {
    if exp < 0 {
        // base^(-n) = 1 / base^n
        let positive_result = integer_power(base, -exp)?;
        return divide(FIXED_ONE, positive_result);
    }
    let mut result = FIXED_ONE;
    let mut remaining = exp;
    let mut current_base = base;
    // Fast exponentiation by squaring.
    while remaining > 0 {
        if remaining & 1 == 1 {
            result = multiply(result, current_base);
        }
        current_base = multiply(current_base, current_base);
        remaining >>= 1;
    }
    Some(result)
}

// ─── Square root (Newton-Raphson) ─────────────────────────────────────────────

/// Compute √x for a Q20.12 fixed-point value x.
///
/// Uses Newton-Raphson iteration: x_{n+1} = (x_n + S/x_n) / 2
/// Converges in ~8 iterations for values in the calculator's working range.
/// Returns None if x is negative.
pub fn sqrt(x: i32) -> Option<i32> {
    if x < 0 { return None; }
    if x == 0 { return Some(0); }

    // Initial guess: shift right by half the fractional bits for a reasonable start.
    // For Q20.12, scale up to Q20.24 then integer sqrt, then scale back.
    let scaled = (x as i64) << FRACTIONAL_BITS; // promote to Q20.24
    let mut guess = integer_sqrt_i64(scaled) as i32;

    // Newton-Raphson refinement (5 iterations is enough for Q20.12 precision).
    for _ in 0..5 {
        if guess == 0 { break; }
        // guess = (guess + x/guess) / 2
        let quotient = (((x as i64) << FRACTIONAL_BITS) / (guess as i64)) as i32;
        guess = (guess + quotient) / 2;
    }
    Some(guess)
}

/// Integer square root of a non-negative i64, returning the floor.
fn integer_sqrt_i64(n: i64) -> i64 {
    if n <= 0 { return 0; }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

// ─── Trigonometry (CORDIC) ────────────────────────────────────────────────────
//
// CORDIC (COordinate Rotation DIgital Computer) computes sin/cos using only
// shifts and additions — ideal for a processor with no FPU and no division.
//
// We use the iterative CORDIC rotation mode in Q20.12 format.
// The algorithm rotates a unit vector toward angle θ, accumulating the result.

/// CORDIC arctangent table in Q20.12 format.
/// Entry i = atan(2^-i) in radians × 4096, for i = 0..15.
const CORDIC_ATAN_TABLE: [i32; 20] = [
    // atan(2^0)  = atan(1)       = 0.7853981634 × 4096 = 3217
    3217,
    // atan(2^-1) = atan(0.5)     = 0.4636476090 × 4096 = 1899
    1899,
    // atan(2^-2) = atan(0.25)    = 0.2449786631 × 4096 = 1004
    1004,
    // atan(2^-3) = atan(0.125)   = 0.1243549945 × 4096 = 509
    509,
    // atan(2^-4)                 = 0.0624188100 × 4096 = 256
    256,
    // atan(2^-5)                 = 0.0312398334 × 4096 = 128
    128,
    // atan(2^-6)                 = 0.0156237286 × 4096 = 64
    64,
    // atan(2^-7)                 = 0.0078123766 × 4096 = 32
    32,
    // atan(2^-8)                 = 0.0039062301 × 4096 = 16
    16,
    // atan(2^-9)                 = 0.0019531226 × 4096 = 8
    8,
    // atan(2^-10)                = 0.0009765622 × 4096 = 4
    4,
    // atan(2^-11)                = 0.0004882812 × 4096 = 2
    2,
    // atan(2^-12..15)            ≈ 1 each
    1, 1, 1, 1, 1, 1, 1, 1,
];

/// CORDIC gain factor K = ∏ cos(atan(2^-i)) ≈ 0.6072529350 in Q20.12 = 2487.
const CORDIC_GAIN: i32 = 2487;

/// Compute (sin(angle), cos(angle)) for angle in Q20.12 radians.
///
/// Input angle must be in [−π, π]. Angles outside this range should be
/// reduced with `reduce_angle_to_principal` before calling.
pub fn sin_cos(angle: i32) -> (i32, i32) {
    // CORDIC initialisation: unit vector pointing along x-axis, scaled by 1/K
    // so that after the CORDIC gain the output is correctly scaled.
    let mut x: i32 = CORDIC_GAIN;
    let mut y: i32 = 0;
    let mut z: i32 = angle; // Remaining angle to rotate

    for i in 0..20 {
        let x_prev = x;
        let y_prev = y;

        if z >= 0 {
            // Rotate counter-clockwise
            x = x_prev - (y_prev >> i);
            y = y_prev + (x_prev >> i);
            z -= CORDIC_ATAN_TABLE[i];
        } else {
            // Rotate clockwise
            x = x_prev + (y_prev >> i);
            y = y_prev - (x_prev >> i);
            z += CORDIC_ATAN_TABLE[i];
        }
    }
    // y = sin(angle), x = cos(angle), both in Q20.12
    (y, x)
}

/// Compute sin(angle) in Q20.12. Angle in Q20.12 radians.
pub fn sin(angle: i32) -> i32 {
    let reduced = reduce_angle_to_principal(angle);
    sin_cos(reduced).0
}

/// Compute cos(angle) in Q20.12. Angle in Q20.12 radians.
pub fn cos(angle: i32) -> i32 {
    let reduced = reduce_angle_to_principal(angle);
    sin_cos(reduced).1
}

/// Compute tan(angle) in Q20.12. Returns None near ±π/2 (cos ≈ 0).
pub fn tan(angle: i32) -> Option<i32> {
    let reduced = reduce_angle_to_principal(angle);
    let (s, c) = sin_cos(reduced);
    // Avoid division by near-zero cosine (threshold: |cos| < 0.001 ≈ 4 in Q20.12)
    if c.abs() < 4 { return None; }
    divide(s, c)
}

/// Reduce an angle (in Q20.12 radians) to the principal range [−π, π].
pub fn reduce_angle_to_principal(angle: i32) -> i32 {
    let mut a = angle % FIXED_TWO_PI;

    if a > FIXED_PI {
        a -= FIXED_TWO_PI;
    }
    if a < -FIXED_PI {
        a += FIXED_TWO_PI;
    }

    a
}

// ─── Inverse trig (asin, acos, atan) ─────────────────────────────────────────

/// Compute atan(x) in Q20.12 radians using the CORDIC vectoring mode.
///
/// Input x is a Q20.12 ratio. Output is in [−π/2, π/2].
pub fn atan(x: i32) -> i32 {
    // CORDIC vectoring: rotate until y → 0, accumulate angle in z.
    let mut vx: i32 = FIXED_ONE;
    let mut vy: i32 = x;
    let mut z:  i32 = 0;

    for i in 0..16 {
        let vx_prev = vx;
        let vy_prev = vy;
        if vy >= 0 {
            vx =  vx_prev + (vy_prev >> i);
            vy =  vy_prev - (vx_prev >> i);
            z  += CORDIC_ATAN_TABLE[i];
        } else {
            vx =  vx_prev - (vy_prev >> i);
            vy =  vy_prev + (vx_prev >> i);
            z  -= CORDIC_ATAN_TABLE[i];
        }
    }
    z
}

/// Compute atan2(y, x) in Q20.12 radians. Output in [−π, π].
pub fn atan2(y: i32, x: i32) -> i32 {
    if x > 0 {
        atan(divide(y, x).unwrap_or(0))
    } else if x < 0 && y >= 0 {
        atan(divide(y, x).unwrap_or(0)) + FIXED_PI
    } else if x < 0 && y < 0 {
        atan(divide(y, x).unwrap_or(0)) - FIXED_PI
    } else if x == 0 && y > 0 {
        FIXED_PI_OVER_2
    } else if x == 0 && y < 0 {
        -FIXED_PI_OVER_2
    } else {
        0 // atan2(0, 0) is undefined — return 0
    }
}

/// Compute asin(x) in Q20.12 radians. Returns None if |x| > 1.
///
/// asin(x) = atan(x / sqrt(1 - x²))
pub fn asin(x: i32) -> Option<i32> {
    // |x| must be ≤ 1 (in Q20.12: ≤ 4096)
    if x.abs() > FIXED_ONE { return None; }
    if x == FIXED_ONE  { return Some(FIXED_PI_OVER_2); }
    if x == -FIXED_ONE { return Some(-FIXED_PI_OVER_2); }

    // 1 - x²
    let x_squared = multiply(x, x);
    let one_minus_x_sq = FIXED_ONE - x_squared;
    let root = sqrt(one_minus_x_sq)?;
    let ratio = divide(x, root)?;
    Some(atan(ratio))
}

/// Compute acos(x) in Q20.12 radians. Returns None if |x| > 1.
///
/// acos(x) = π/2 − asin(x)
pub fn acos(x: i32) -> Option<i32> {
    let asin_val = asin(x)?;
    Some(FIXED_PI_OVER_2 - asin_val)
}

// ─── Logarithms and exponentials ─────────────────────────────────────────────

/// Compute the natural exponential e^x for Q20.12 x.
///
/// Uses the identity: e^x = 2^(x / ln2) = 2^k × 2^(fraction)
/// The fractional part is computed with a degree-5 minimax polynomial.
///
/// Valid range: x in roughly [−20, +20] in Q20.12 (i.e. stored as ±81920).
pub fn natural_exp(x: i32) -> i32 {
    // Split x into integer and fractional parts: x = k*ln2 + r, |r| < ln2/2
    // k = round(x / ln2)
    let k = to_integer_rounded(divide(x, FIXED_LN2).unwrap_or(0));

    // r = x - k * ln2  (remaining fractional argument)
    let r = x - k * FIXED_LN2;

    // Polynomial approximation of e^r for |r| < ln2/2 ≈ 0.347
    // P(r) = 1 + r + r²/2 + r³/6 + r⁴/24 + r⁵/120  (Taylor series)
    // All arithmetic in Q20.12.
    let r2 = multiply(r, r);
    let r3 = multiply(r2, r);
    let r4 = multiply(r3, r);
    let r5 = multiply(r4, r);

    let poly = FIXED_ONE
        + r
        + divide(r2, from_integer(2)).unwrap_or(0)
        + divide(r3, from_integer(6)).unwrap_or(0)
        + divide(r4, from_integer(24)).unwrap_or(0)
        + divide(r5, from_integer(120)).unwrap_or(0);

    // Scale by 2^k: shift left for positive k, right for negative k.
    if k >= 0 {
        poly << (k as u32).min(20) // cap shift to prevent panic
    } else {
        poly >> ((-k) as u32).min(31)
    }
}

/// Compute ln(x) for Q20.12 x > 0. Returns None for x ≤ 0.
///
/// Uses: ln(x) = ln(2^k × m) = k×ln2 + ln(m)   where m ∈ [1, 2)
/// Then ln(m) is approximated by a Taylor series around m=1.
pub fn natural_log(x: i32) -> Option<i32> {
    if x <= 0 { return None; }

    // Find integer k such that x = 2^k × m, m ∈ [1, 2) in Q20.12.
    // In Q20.12, 1.0 = 4096 = 2^12. So we're looking for the leading bit.
    let mut mantissa = x;
    let mut k: i32 = 0;

    // Normalise mantissa into [SCALE, 2*SCALE) i.e. [1.0, 2.0) in Q20.12.
    while mantissa >= 2 * SCALE {
        mantissa >>= 1;
        k += 1;
    }
    while mantissa < SCALE {
        mantissa <<= 1;
        k -= 1;
    }

    // Now compute ln(mantissa/SCALE) = ln(1 + t) where t = (mantissa-SCALE)/SCALE
    // Using Taylor: ln(1+t) = t - t²/2 + t³/3 - t⁴/4 + t⁵/5, |t| < 1
    let t = divide(mantissa - SCALE, SCALE).unwrap_or(0); // t in Q20.12, range [0, SCALE)

    // All operations in Q20.12
    let t2 = multiply(t, t);
    let t3 = multiply(t2, t);
    let t4 = multiply(t3, t);
    let t5 = multiply(t4, t);

    let ln_mantissa = t
        - divide(t2, from_integer(2)).unwrap_or(0)
        + divide(t3, from_integer(3)).unwrap_or(0)
        - divide(t4, from_integer(4)).unwrap_or(0)
        + divide(t5, from_integer(5)).unwrap_or(0);

    // ln(x) = k × ln2 + ln(mantissa)
    Some(k * FIXED_LN2 + ln_mantissa)
}

/// Compute log base 10 of x. Returns None for x ≤ 0.
///
/// log10(x) = ln(x) / ln(10)
pub fn log10(x: i32) -> Option<i32> {
    // ln(10) in Q20.12 = 2.302585 × 4096 = 9434
    const FIXED_LN10: i32 = 9434;
    let ln_x = natural_log(x)?;
    divide(ln_x, FIXED_LN10)
}

/// Compute log base 2 of x. Returns None for x ≤ 0.
pub fn log2(x: i32) -> Option<i32> {
    let ln_x = natural_log(x)?;
    divide(ln_x, FIXED_LN2)
}

// ─── Rounding and absolute value ─────────────────────────────────────────────

/// Absolute value of a Q20.12 value.
#[inline(always)]
pub fn abs(x: i32) -> i32 {
    if x < 0 { -x } else { x }
}

/// Floor: round toward negative infinity.
#[inline(always)]
pub fn floor(x: i32) -> i32 {
    // Mask off the fractional bits. For negative numbers with a fractional
    // part, this naturally rounds toward −∞.
    (x >> FRACTIONAL_BITS) << FRACTIONAL_BITS
}

/// Ceiling: round toward positive infinity.
#[inline(always)]
pub fn ceil(x: i32) -> i32 {
    // If there are any fractional bits set, add 1 to the integer part.
    if x & (SCALE - 1) != 0 {
        floor(x) + SCALE
    } else {
        x
    }
}

/// Round to nearest integer (half-way cases round away from zero).
#[inline(always)]
pub fn round(x: i32) -> i32 {
    floor(x + FIXED_HALF)
}

/// Convert degrees to radians.
#[inline(always)]
pub fn degrees_to_radians(degrees: i32) -> i32 {
    multiply(degrees, FIXED_PI_OVER_180)
}

/// Convert radians to degrees.
#[inline(always)]
pub fn radians_to_degrees(radians: i32) -> i32 {
    multiply(radians, FIXED_180_OVER_PI)
}

// ─── Formatting ───────────────────────────────────────────────────────────────

/// Format a Q20.12 fixed-point value into a human-readable decimal string.
///
/// Produces up to 4 decimal places, trailing zeros stripped.
/// Output goes into `buffer` (must be ≥ 20 bytes). Returns the filled slice.
///
/// Examples:
///   4096  (= 1.0)    → b"1"
///   6144  (= 1.5)    → b"1.5"
///   12868 (= π)      → b"3.1415"
///  -4096  (= -1.0)   → b"-1"
pub fn format_fixed_point(value: i32, buffer: &mut [u8; 20]) -> &[u8] {
    let is_negative = value < 0;
    // Safely get absolute value — use i64 to handle i32::MIN.
    let abs_value = if is_negative { -(value as i64) } else { value as i64 };

    let integer_part = (abs_value >> FRACTIONAL_BITS) as u32;
    // Extract fractional bits and convert to decimal digits (4 places).
    // frac_raw / SCALE = fractional value; multiply by 10000 for 4 decimal places.
    let frac_raw = (abs_value & (SCALE as i64 - 1)) as u32;
    let frac_decimal = (frac_raw * 10000) / (SCALE as u32); // 0..9999

    let mut pos = 0usize;

    // Write sign.
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
        // Write digits right-to-left into a temp area, then reverse.
        let mut n = integer_part;
        while n > 0 {
            buffer[pos] = b'0' + (n % 10) as u8;
            pos += 1;
            n /= 10;
        }
        buffer[int_start..pos].reverse();
    }

    // Write fractional part (up to 4 digits, trailing zeros stripped).
    if frac_decimal > 0 {
        buffer[pos] = b'.';
        pos += 1;

        // Produce exactly 4 decimal digits then strip trailing zeros.
        let mut frac_digits = [0u8; 4];
        let mut fd = frac_decimal;
        for i in (0..4).rev() {
            frac_digits[i] = b'0' + (fd % 10) as u8;
            fd /= 10;
        }
        // Find last non-zero digit.
        let mut last_nonzero = 0usize;
        for i in 0..4 {
            if frac_digits[i] != b'0' { last_nonzero = i; }
        }
        for i in 0..=last_nonzero {
            buffer[pos] = frac_digits[i];
            pos += 1;
        }
    }

    &buffer[..pos]
}
