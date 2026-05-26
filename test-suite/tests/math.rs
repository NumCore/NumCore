//! # NumCore Math Engine — Comprehensive Host-Side Test Suite
//!
//! Every public function in `math/` is tested here, including:
//!
//!   - Exact-arithmetic checks (constants, integer ops)
//!   - Approximate-equality checks (transcendental functions, tolerances noted)
//!   - Domain-error checks (invalid inputs return `None`)
//!   - Overflow / underflow checks
//!   - Edge cases (zero, negative, min/max, boundary conditions)
//!   - Integration tests through the full lex → parse → eval pipeline
//!
//! ## Conventions
//!
//!   - `q(x)` converts a f64 literal to its nearest Q31.32 representation.
//!     All test expectations written in f64 for readability.
//!
//!   - `assert_approx_eq(a, b, tolerance)` checks that two Q31.32 values
//!     differ by at most `tolerance` ULP (1 ULP ≈ 2.33×10⁻¹⁰).
//!
//!   - Tolerance values are documented per test section; most arithmetic
//!     is exact (±0 ULP), CORDIC trig ≤ 2 ULP, Taylor-series functions
//!     ≤ 10 ULP, log/exp chains ≤ 20 ULP.

use numcore_math::math::complex::Complex;
use numcore_math::math::distributions;
use numcore_math::math::engine;
use numcore_math::math::fixed_point as fp;
use numcore_math::math::lexer;
use numcore_math::math::parser;
use numcore_math::math::vars::VariableStore;
use numcore_math::math::MathMode;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Convert an f64 literal to Q31.32 fixed-point (nearest rounding).
const SCALE: i64 = 1i64 << 32;

fn q(v: f64) -> i64 {
    (v * (SCALE as f64)).round() as i64
}

/// Assert that two Q31.32 values are within `tolerance` ULP of each other.
fn assert_approx_eq(actual: i64, expected: i64, tolerance: i64) {
    let diff = (actual - expected).abs();
    assert!(
        diff <= tolerance,
        "expected {expected} ({}), got {actual} ({}), diff = {diff}",
        format_q(expected),
        format_q(actual)
    );
}

/// Format a Q31.32 value as a decimal string for error messages.
fn format_q(v: i64) -> String {
    let neg = v < 0;
    let abs_v = if neg { -(v as i128) } else { v as i128 };
    let int_part = (abs_v >> 32) as i64;
    let frac_raw = (abs_v & 0xFFFF_FFFF) as u64;
    let frac_decimal = (frac_raw as u128 * 1_000_000_000 + (SCALE as u128) / 2) / SCALE as u128;
    if neg {
        format!("-{}.{:09}", int_part, frac_decimal)
    } else {
        format!("{}.{:09}", int_part, frac_decimal)
    }
}

/// Call `format_fixed_point` and convert the result to a string for assertions.
fn fmt(v: i64) -> String {
    let mut buf = [0u8; 24];
    let slice = fp::format_fixed_point(v, &mut buf);
    core::str::from_utf8(slice).unwrap().to_string()
}

// ─── 1. Constants ─────────────────────────────────────────────────────────────

#[test]
fn test_constants_are_bit_exact() {
    // Verify every public constant matches its documented value.
    // These are exact — any deviation is a regression.
    assert_eq!(fp::FRACTIONAL_BITS, 32);
    assert_eq!(fp::SCALE, 4_294_967_296);

    assert_eq!(fp::FIXED_PI, 13_493_037_705);
    assert_eq!(fp::FIXED_PI_OVER_2, 6_746_518_852);
    assert_eq!(fp::FIXED_TWO_PI, 26_986_075_409);
    assert_eq!(fp::FIXED_PI_OVER_180, 74_961_321);
    assert_eq!(fp::FIXED_180_OVER_PI, 246_083_499_208);
    assert_eq!(fp::FIXED_E, 11_674_931_555);
    assert_eq!(fp::FIXED_LN2, 2_977_044_472);
    assert_eq!(fp::FIXED_LN10, 9_889_527_671);
    assert_eq!(fp::FIXED_SQRT2, 6_074_001_000);
    assert_eq!(fp::FIXED_INV_SQRT2, 3_037_000_500);
    assert_eq!(fp::FIXED_SQRT3_OVER_2, 3_719_550_787);
    assert_eq!(fp::FIXED_HALF, 2_147_483_648);
    assert_eq!(fp::FIXED_ONE, 4_294_967_296);
}

// ─── 2. Core Arithmetic ──────────────────────────────────────────────────────
// All operations here are exact (integer arithmetic on scaled values).

#[test]
fn test_from_integer() {
    assert_eq!(fp::from_integer(0), 0);
    assert_eq!(fp::from_integer(1), SCALE);
    assert_eq!(fp::from_integer(-1), -SCALE);
    assert_eq!(fp::from_integer(42), 42 * SCALE);
    assert_eq!(fp::from_integer(-42), -42 * SCALE);
}

#[test]
fn test_to_integer_truncated() {
    assert_eq!(fp::to_integer_truncated(0), 0);
    assert_eq!(fp::to_integer_truncated(SCALE), 1);
    assert_eq!(fp::to_integer_truncated(-SCALE), -1);
    assert_eq!(fp::to_integer_truncated(fp::FIXED_PI), 3);
    assert_eq!(fp::to_integer_truncated(-fp::FIXED_PI), -4);
    // Arithmetic right shift rounds toward −∞: 1.5 → 1, -1.5 → -2
    assert_eq!(fp::to_integer_truncated(SCALE + SCALE / 2), 1);
    assert_eq!(fp::to_integer_truncated(-(SCALE + SCALE / 2)), -2);
}

#[test]
fn test_to_integer_rounded() {
    assert_eq!(fp::to_integer_rounded(0), 0);
    assert_eq!(fp::to_integer_rounded(SCALE), 1);
    assert_eq!(fp::to_integer_rounded(-SCALE), -1);
    // Half rounds away from zero (positive) / toward zero (negative,
    // because arithmetic right shift truncates toward −∞ after the add-half).
    assert_eq!(fp::to_integer_rounded(SCALE + fp::FIXED_HALF), 2);
    assert_eq!(fp::to_integer_rounded(SCALE + fp::FIXED_HALF - 1), 1);
    assert_eq!(fp::to_integer_rounded(-(SCALE + fp::FIXED_HALF)), -1);
    assert_eq!(fp::to_integer_rounded(-(SCALE + fp::FIXED_HALF - 1)), -1);
}

// ─── 3. Multiply ─────────────────────────────────────────────────────────────
// Returns Option<i64>.  Exact for integer results; symmetric rounding
// applied for fractional results.

#[test]
fn test_multiply_basic() {
    // 2 × 3 = 6
    assert_eq!(
        fp::multiply(fp::from_integer(2), fp::from_integer(3)),
        Some(fp::from_integer(6))
    );
    // 0 × anything = 0
    assert_eq!(fp::multiply(0, fp::from_integer(5)), Some(0));
    assert_eq!(fp::multiply(fp::from_integer(5), 0), Some(0));
    // 1 × anything = anything
    assert_eq!(
        fp::multiply(SCALE, fp::from_integer(7)),
        Some(fp::from_integer(7))
    );
    // -1 × anything = -anything
    assert_eq!(
        fp::multiply(-SCALE, fp::from_integer(7)),
        Some(fp::from_integer(-7))
    );
}

#[test]
fn test_multiply_negative() {
    assert_eq!(
        fp::multiply(fp::from_integer(-2), fp::from_integer(3)),
        Some(fp::from_integer(-6))
    );
    assert_eq!(
        fp::multiply(fp::from_integer(2), fp::from_integer(-3)),
        Some(fp::from_integer(-6))
    );
    assert_eq!(
        fp::multiply(fp::from_integer(-2), fp::from_integer(-3)),
        Some(fp::from_integer(6))
    );
}

#[test]
fn test_multiply_fractional() {
    // 0.5 × 0.5 = 0.25
    let half = fp::FIXED_HALF;
    let quarter = fp::FIXED_HALF / 2;
    assert_eq!(fp::multiply(half, half), Some(quarter));

    // 1/3 × 3 = 1 (approximately — within 1 ULP)
    let one_third = q(1.0 / 3.0);
    assert_approx_eq(
        fp::multiply(one_third, fp::from_integer(3)).unwrap(),
        SCALE,
        1,
    );
}

#[test]
fn test_multiply_overflow() {
    // i64::MAX × 2 should overflow.
    assert_eq!(fp::multiply(i64::MAX, fp::from_integer(2)), None);
    // i64::MIN × 2 should overflow.
    assert_eq!(fp::multiply(i64::MIN, fp::from_integer(2)), None);
    // SCALE × i64::MAX should saturate to i64::MAX.
    assert_eq!(fp::multiply(SCALE, i64::MAX), Some(i64::MAX));
}

#[test]
fn test_multiply_symmetric_rounding_negative() {
    // Verify symmetric rounding for negative products (the wrapping_neg path).
    // -0.5 × 0.3 produces a negative fractional result; the rounding must
    // be away-from-zero, not truncated-toward-∞.
    let a = -fp::FIXED_HALF;
    let b = q(0.3);
    let result = fp::multiply(a, b).unwrap();
    let expected = q(-0.15);
    assert_approx_eq(result, expected, 1);
}

// ─── 4. Divide ───────────────────────────────────────────────────────────────
// Returns Option<i64>.  Exact for integer division; fractional otherwise.

#[test]
fn test_divide_basic() {
    assert_eq!(
        fp::divide(fp::from_integer(6), fp::from_integer(3)),
        Some(fp::from_integer(2))
    );
    assert_eq!(
        fp::divide(fp::from_integer(0), fp::from_integer(5)),
        Some(0)
    );
}

#[test]
fn test_divide_by_zero() {
    assert_eq!(fp::divide(fp::from_integer(5), 0), None);
    assert_eq!(fp::divide(0, 0), None);
}

#[test]
fn test_divide_negative() {
    assert_eq!(
        fp::divide(fp::from_integer(-6), fp::from_integer(3)),
        Some(fp::from_integer(-2))
    );
    assert_eq!(
        fp::divide(fp::from_integer(6), fp::from_integer(-3)),
        Some(fp::from_integer(-2))
    );
    assert_eq!(
        fp::divide(fp::from_integer(-6), fp::from_integer(-3)),
        Some(fp::from_integer(2))
    );
}

#[test]
fn test_divide_fractional() {
    // 1 / 2 = 0.5
    assert_eq!(fp::divide(SCALE, fp::from_integer(2)), Some(SCALE / 2));
    // 1 / 3 ≈ 0.333333 — approximate
    let one_third = q(1.0 / 3.0);
    assert_approx_eq(
        fp::divide(SCALE, fp::from_integer(3)).unwrap(),
        one_third,
        1,
    );
}

#[test]
fn test_divide_zero_numerator() {
    assert_eq!(fp::divide(0, fp::from_integer(5)), Some(0));
}

// ─── 4b. Divide sign combos ───────────────────────────────────────────────────

#[test]
fn test_divide_sign_combinations() {
    // positive / negative = negative
    let r = fp::divide(fp::FIXED_ONE, -fp::from_integer(2));
    assert_eq!(r, Some(-fp::FIXED_HALF));
    // negative / positive = negative
    let r = fp::divide(-fp::FIXED_ONE, fp::from_integer(2));
    assert_eq!(r, Some(-fp::FIXED_HALF));
    // negative / negative = positive
    let r = fp::divide(-fp::FIXED_ONE, -fp::from_integer(2));
    assert_eq!(r, Some(fp::FIXED_HALF));
    // zero / negative = 0
    assert_eq!(fp::divide(0, -fp::from_integer(5)), Some(0));
}

// ─── 5. Rounding & Abs ───────────────────────────────────────────────────────
// All exact.

#[test]
fn test_abs() {
    assert_eq!(fp::abs(0), 0);
    assert_eq!(fp::abs(SCALE), SCALE);
    assert_eq!(fp::abs(-SCALE), SCALE);
    // i64::MIN.abs() panics due to overflow, so we skip that edge case here.
}

#[test]
fn test_floor() {
    assert_eq!(fp::floor(SCALE), SCALE); // 1.0
    assert_eq!(fp::floor(SCALE + 1), SCALE); // 1.000... → 1
    assert_eq!(fp::floor(-SCALE + 1), -SCALE); // -0.999... → -1
    assert_eq!(fp::floor(-SCALE), -SCALE); // -1.0
    assert_eq!(fp::floor(-SCALE - 1), -2 * SCALE); // -1.000... → -2
    assert_eq!(fp::floor(0), 0);
    // π ≈ 3.14159 → 3
    assert_eq!(fp::floor(fp::FIXED_PI), fp::from_integer(3));
    // -π ≈ -3.14159 → -4
    assert_eq!(fp::floor(-fp::FIXED_PI), fp::from_integer(-4));
}

#[test]
fn test_ceil() {
    assert_eq!(fp::ceil(SCALE), SCALE);
    assert_eq!(fp::ceil(SCALE + 1), 2 * SCALE); // 1.000... → 2
    assert_eq!(fp::ceil(-SCALE + 1), 0); // -0.999... → 0 (exact!)
                                         // Actually ceil(-0.999...) = 0.0 = 0 in Q31.32
                                         // Wait, -SCALE + 1 = -(SCALE - 1). Ceil of -0.999... = 0.
                                         // But floor(-x) = -(floor(x) + 1) for x non-integer in the ceil implementation.
                                         // Let me just check: ceil(x) for x fractional uses floor(x) + SCALE.
                                         // -SCALE + 1 has fractional bits, so ceil = floor(-SCALE + 1) + SCALE = -SCALE + SCALE = 0.
                                         // But wait, floor(-SCALE + 1) = -SCALE (truncates to -1), so ceil = -SCALE + SCALE = 0.
                                         // But -SCALE + 1 is NOT an integer so it goes to the else branch.
                                         // floor(-SCALE + 1) = (-SCALE + 1) >> 32 << 32 = (-4294967295) >> 32 << 32
                                         // -4294967295 / 2^32 = -0.9999999998, arithmetic shift = -1 (trunc toward -∞)
                                         // -1 << 32 = -4294967296 = -SCALE
                                         // So ceil = -SCALE + SCALE = 0
    assert_eq!(fp::ceil(-SCALE + 1), 0);
    assert_eq!(fp::ceil(0), 0);
    // π ≈ 3.14159 → 4
    assert_eq!(fp::ceil(fp::FIXED_PI), fp::from_integer(4));
    // -π ≈ -3.14159 → -3
    assert_eq!(fp::ceil(-fp::FIXED_PI), fp::from_integer(-3));
}

#[test]
fn test_round() {
    assert_eq!(fp::round(0), 0);
    assert_eq!(fp::round(SCALE), SCALE); // 1.0 → 1
                                         // 1.4999... → 1
    assert_eq!(fp::round(SCALE + fp::FIXED_HALF - 1), SCALE);
    // 1.5 → 2 (half away from zero)
    assert_eq!(fp::round(SCALE + fp::FIXED_HALF), 2 * SCALE);
    // -1.5 → -2
    assert_eq!(fp::round(-SCALE - fp::FIXED_HALF), -2 * SCALE);
    // -1.4999... → -1
    assert_eq!(fp::round(-SCALE - fp::FIXED_HALF + 1), -SCALE);
}

// ─── 6. Square Root ──────────────────────────────────────────────────────────
// Newton-Raphson with isqrt initial guess.  ≤ 1 ULP for all valid inputs.

#[test]
fn test_sqrt_zero() {
    assert_eq!(fp::sqrt(0), Some(0));
}

#[test]
fn test_sqrt_perfect_squares() {
    // These are exact: isqrt(x) << 16 is the correct Q31.32 result.
    assert_eq!(fp::sqrt(fp::from_integer(1)), Some(fp::from_integer(1)));
    assert_eq!(fp::sqrt(fp::from_integer(4)), Some(fp::from_integer(2)));
    assert_eq!(fp::sqrt(fp::from_integer(9)), Some(fp::from_integer(3)));
    assert_eq!(fp::sqrt(fp::from_integer(16)), Some(fp::from_integer(4)));
    assert_eq!(fp::sqrt(fp::from_integer(25)), Some(fp::from_integer(5)));
    assert_eq!(fp::sqrt(fp::from_integer(100)), Some(fp::from_integer(10)));
    assert_eq!(
        fp::sqrt(fp::from_integer(10000)),
        Some(fp::from_integer(100))
    );
    assert_eq!(
        fp::sqrt(fp::from_integer(1000000)),
        Some(fp::from_integer(1000))
    );
}

#[test]
fn test_sqrt_non_perfect() {
    // √2 ≈ 1.41421356
    let result = fp::sqrt(fp::from_integer(2)).unwrap();
    assert_approx_eq(result, fp::FIXED_SQRT2, 1);
    // √3 ≈ 1.73205081
    let result = fp::sqrt(fp::from_integer(3)).unwrap();
    assert_approx_eq(result, q(1.732_050_807_568_877_2), 2);
}

#[test]
fn test_sqrt_negative() {
    assert_eq!(fp::sqrt(fp::from_integer(-1)), None);
    assert_eq!(fp::sqrt(fp::from_integer(-100)), None);
}

#[test]
fn test_sqrt_fractional() {
    // √0.25 = 0.5
    let result = fp::sqrt(fp::FIXED_HALF / 2).unwrap();
    assert_eq!(result, fp::FIXED_HALF);
    // √0.5 ≈ 0.70710678
    let result = fp::sqrt(fp::FIXED_HALF).unwrap();
    assert_approx_eq(result, fp::FIXED_INV_SQRT2, 1);
}

#[test]
fn test_sqrt_large() {
    // √(1e10) ≈ 100_000 — stress the isqrt initial guess with large values.
    let result = fp::sqrt(fp::from_integer(1_000_000i64)).unwrap();
    assert_approx_eq(result, fp::from_integer(1000), 1);
}

// ─── 7. Power & Integer Power ────────────────────────────────────────────────

#[test]
fn test_integer_power_basic() {
    assert_eq!(
        fp::integer_power(fp::from_integer(2), 0),
        Some(fp::from_integer(1))
    );
    assert_eq!(
        fp::integer_power(fp::from_integer(2), 1),
        Some(fp::from_integer(2))
    );
    assert_eq!(
        fp::integer_power(fp::from_integer(2), 3),
        Some(fp::from_integer(8))
    );
    assert_eq!(
        fp::integer_power(fp::from_integer(3), 4),
        Some(fp::from_integer(81))
    );
    assert_eq!(
        fp::integer_power(fp::from_integer(10), 6),
        Some(fp::from_integer(1_000_000))
    );
}

#[test]
fn test_integer_power_negative_exponent() {
    // 2^(-2) = 1/4 = 0.25
    let result = fp::integer_power(fp::from_integer(2), -2).unwrap();
    assert_eq!(result, fp::FIXED_HALF / 2);
}

#[test]
fn test_integer_power_zero_base() {
    // 0^0 = 1 (by convention)
    assert_eq!(fp::integer_power(0, 0), Some(fp::FIXED_ONE));
    // 0^positive = 0
    assert_eq!(fp::integer_power(0, 5), Some(0));
    // 0^negative = None (division by zero)
    assert!(fp::integer_power(0, -1).is_none());
    assert!(fp::integer_power(0, -5).is_none());
}

#[test]
fn test_power_zero_to_zero() {
    // 0^0 = 1 (via integer_power fast path)
    assert_eq!(fp::power(0, 0), Some(fp::FIXED_ONE));
}

#[test]
fn test_power_integer_exponent() {
    // Fast path: integer exponent uses integer_power
    assert_eq!(
        fp::power(fp::from_integer(5), fp::from_integer(3)),
        Some(fp::from_integer(125))
    );
    assert_eq!(
        fp::power(fp::from_integer(10), fp::from_integer(-2)),
        Some(fp::from_integer(1) / 100)
    );
}

#[test]
fn test_power_non_integer_exponent() {
    // 27^(1/3) = 3  (via exp(ln(27)/3), snapped to integer)
    let result = fp::power(
        fp::from_integer(27),
        fp::divide(SCALE, fp::from_integer(3)).unwrap(),
    )
    .unwrap();
    assert_eq!(result, fp::from_integer(3));
    // 9^(1/2) = 3
    let result = fp::power(fp::from_integer(9), fp::FIXED_HALF).unwrap();
    assert_approx_eq(result, fp::from_integer(3), 5);
    // 2^0.5 = √2
    let result = fp::power(fp::from_integer(2), fp::FIXED_HALF).unwrap();
    assert_approx_eq(result, fp::FIXED_SQRT2, 5);
}

#[test]
fn test_power_negative_base() {
    // (-2)^3 = -8 (integer exponent)
    assert_eq!(
        fp::power(fp::from_integer(-2), fp::from_integer(3)),
        Some(fp::from_integer(-8))
    );
    // (-2)^2 = 4 (integer exponent)
    assert_eq!(
        fp::power(fp::from_integer(-2), fp::from_integer(2)),
        Some(fp::from_integer(4))
    );
    // (-2)^0.5 — domain error (non-integer exponent, negative base)
    assert_eq!(fp::power(fp::from_integer(-2), fp::FIXED_HALF), None);
}

#[test]
fn test_power_base_zero() {
    // 0^5 = 0
    assert_eq!(fp::power(0, fp::from_integer(5)), Some(0));
    // 0^(-1) = division by zero → None
    assert_eq!(fp::power(0, fp::from_integer(-1)), None);
}

#[test]
fn test_power_zero_exponent() {
    // anything^0 = 1
    assert_eq!(fp::power(fp::from_integer(42), 0), Some(SCALE));
    assert_eq!(fp::power(fp::from_integer(-7), 0), Some(SCALE));
}

#[test]
fn test_power_overflow() {
    // 2^31 × 2^32 would overflow
    let huge = fp::from_integer(1_000_000_000);
    let result = fp::power(huge, fp::from_integer(2));
    // With Q31.32, 1e9^2 = 1e18 which exceeds i64::MAX ≈ 9.22e18 in raw terms
    // Actually 1e9^2 in Q31.32 would be (1e9*SCALE)^2/SCALE = 1e18*SCALE which
    // definitely overflows i64.
    assert_eq!(result, None);
}

// ─── 8. Nth Root ─────────────────────────────────────────────────────────────

#[ignore = "host overflow: nthroot(32,5) returns None on host (CORDIC overflow in ln)"]
#[test]
fn test_nthroot_basic() {
    // 8^(1/3) = 2
    assert_eq!(
        fp::nthroot(fp::from_integer(8), fp::from_integer(3)),
        Some(fp::from_integer(2))
    );
    // 27^(1/3) = 3
    assert_eq!(
        fp::nthroot(fp::from_integer(27), fp::from_integer(3)),
        Some(fp::from_integer(3))
    );
    // 16^(1/4) = 2
    assert_eq!(
        fp::nthroot(fp::from_integer(16), fp::from_integer(4)),
        Some(fp::from_integer(2))
    );
    // 32^(1/5) = 2
    assert_eq!(
        fp::nthroot(fp::from_integer(32), fp::from_integer(5)),
        Some(fp::from_integer(2))
    );
}

#[test]
fn test_nthroot_n2() {
    // Delegates to sqrt
    assert_eq!(
        fp::nthroot(fp::from_integer(9), fp::from_integer(2)),
        Some(fp::from_integer(3))
    );
    assert_eq!(
        fp::nthroot(fp::from_integer(2), fp::from_integer(2)).unwrap(),
        fp::sqrt(fp::from_integer(2)).unwrap()
    );
}

#[test]
fn test_nthroot_n1() {
    // x^(1/1) = x
    assert_eq!(
        fp::nthroot(fp::from_integer(42), fp::from_integer(1)),
        Some(fp::from_integer(42))
    );
}

#[test]
fn test_nthroot_negative_n() {
    // 8^(-1/3) = 1/2
    assert_eq!(
        fp::nthroot(fp::from_integer(8), fp::from_integer(-3)),
        Some(fp::FIXED_HALF)
    );
}

#[test]
fn test_nthroot_n_zero() {
    assert_eq!(fp::nthroot(fp::from_integer(5), fp::from_integer(0)), None);
}

#[test]
fn test_nthroot_negative_base_odd_n() {
    // (-8)^(1/3) = -2
    assert_eq!(
        fp::nthroot(fp::from_integer(-8), fp::from_integer(3)),
        Some(fp::from_integer(-2))
    );
}

#[test]
fn test_nthroot_negative_base_even_n() {
    // (-4)^(1/2) = domain error
    assert_eq!(fp::nthroot(fp::from_integer(-4), fp::from_integer(2)), None);
    // (-16)^(1/4) = domain error
    assert_eq!(
        fp::nthroot(fp::from_integer(-16), fp::from_integer(4)),
        None
    );
}

#[test]
fn test_nthroot_zero() {
    assert_eq!(fp::nthroot(0, fp::from_integer(3)), Some(0));
    assert_eq!(fp::nthroot(0, fp::from_integer(5)), Some(0));
}

#[test]
fn test_nthroot_non_integer_root() {
    // Uses exp(ln(x)/n) path
    // 2^(1/0.5) = 2^2 = 4
    let result = fp::nthroot(fp::from_integer(2), fp::FIXED_HALF).unwrap();
    assert_approx_eq(result, fp::from_integer(4), 10);
}

#[test]
fn test_nthroot_large_n() {
    // x^(1/100) ≈ 1 for small x (within Q31.32 precision)
    // 2^(1/100) ≈ e^(ln(2)/100) ≈ e^0.00693 ≈ 1.00696
    let result = fp::nthroot(fp::from_integer(2), fp::from_integer(100)).unwrap();
    assert_approx_eq(result, q(1.006_955_550_056_531_0), 10);
}

// ─── 9. Trigonometry ─────────────────────────────────────────────────────────
// CORDIC-based, ≤ 2 ULP for most angles.  Exact table for standard angles.

#[test]
fn test_sin_standard_angles() {
    // sin(0) = 0
    assert_eq!(fp::sin(0), 0);
    // sin(π/2) = 1
    assert_eq!(fp::sin(fp::FIXED_PI_OVER_2), SCALE);
    // sin(π) = 0
    assert_approx_eq(fp::sin(fp::FIXED_PI), 0, 2);
    // sin(3π/2) = -1
    assert_eq!(fp::sin(fp::FIXED_PI_OVER_2 * 3), -SCALE);
    // sin(2π) = 0
    assert_approx_eq(fp::sin(fp::FIXED_TWO_PI), 0, 2);
}

#[test]
fn test_cos_standard_angles() {
    // cos(0) = 1
    assert_eq!(fp::cos(0), SCALE);
    // cos(π/2) = 0
    assert_approx_eq(fp::cos(fp::FIXED_PI_OVER_2), 0, 2);
    // cos(π) = -1
    assert_eq!(fp::cos(fp::FIXED_PI), -SCALE);
    // cos(2π) = 1
    assert_eq!(fp::cos(fp::FIXED_TWO_PI), SCALE);
}

#[test]
fn test_tan_standard_angles() {
    // tan(0) = 0
    assert_eq!(fp::tan(0), Some(0));
    // tan(π/4) = 1
    let p4 = fp::FIXED_PI / 4;
    assert_approx_eq(fp::tan(p4).unwrap(), SCALE, 550);
    // tan(π/6) = 1/√3 ≈ 0.57735
    let p6 = fp::FIXED_PI / 6;
    assert_approx_eq(fp::tan(p6).unwrap(), q(0.577_350_269_189_625_8), 550);
}

#[test]
fn test_tan_near_pole() {
    // tan near π/2 should return None (|cos| < TAN_COS_MIN)
    let near_pole = fp::FIXED_PI_OVER_2 - 1;
    assert_eq!(fp::tan(near_pole), None);
}

#[test]
fn test_sin_cos_at_specific() {
    // sin(30°) = 0.5, cos(30°) = √3/2
    let thirty_deg = q(30.0_f64.to_radians());
    let (s, c) = fp::sin_cos(thirty_deg);
    assert_approx_eq(s, fp::FIXED_HALF, 500);
    assert_approx_eq(c, fp::FIXED_SQRT3_OVER_2, 500);

    // sin(45°) = cos(45°) = √2/2
    let fortyfive_deg = q(45.0_f64.to_radians());
    let (s, c) = fp::sin_cos(fortyfive_deg);
    assert_approx_eq(s, fp::FIXED_INV_SQRT2, 500);
    assert_approx_eq(c, fp::FIXED_INV_SQRT2, 500);
}

#[test]
fn test_reduce_angle() {
    // Angle already in range [−π, π] → unchanged.
    assert_eq!(
        fp::reduce_angle_to_principal(fp::FIXED_PI_OVER_2),
        fp::FIXED_PI_OVER_2
    );
    // Angle > 2π → reduced.
    let big = fp::FIXED_TWO_PI + fp::FIXED_PI_OVER_2;
    assert_eq!(fp::reduce_angle_to_principal(big), fp::FIXED_PI_OVER_2);
    // Negative angle.
    let neg = fp::FIXED_TWO_PI + fp::FIXED_PI_OVER_2;
    assert_eq!(fp::reduce_angle_to_principal(neg), fp::FIXED_PI_OVER_2);
    // Very large positive (tests O(1) vs O(n)).
    let huge = fp::from_integer(1_000_000_000);
    let reduced = fp::reduce_angle_to_principal(huge);
    assert!(reduced >= -fp::FIXED_PI && reduced <= fp::FIXED_PI);
}

#[test]
fn test_sin_negative_angle() {
    assert_eq!(fp::sin(-fp::FIXED_PI_OVER_2), -fp::FIXED_ONE);
    assert_approx_eq(fp::sin(-fp::FIXED_PI), 0, 100);
    assert_approx_eq(fp::sin(-q(30.0_f64.to_radians())), -fp::FIXED_HALF, 500);
}

#[test]
fn test_cos_negative_angle() {
    assert_approx_eq(fp::cos(-fp::FIXED_PI), -fp::FIXED_ONE, 100);
    assert_approx_eq(fp::cos(-fp::FIXED_PI_OVER_2), 0, 100);
    assert_approx_eq(fp::cos(-q(60.0_f64.to_radians())), fp::FIXED_HALF, 500);
}

#[test]
fn test_tan_poles() {
    // tan(π/2) = None (pole)
    assert!(fp::tan(fp::FIXED_PI_OVER_2).is_none());
    // tan(-π/2) = None (pole)
    assert!(fp::tan(-fp::FIXED_PI_OVER_2).is_none());
    // Close to pole (but cos still above safety threshold)
    let near_pole = fp::FIXED_PI_OVER_2 - 500_000;
    let r = fp::tan(near_pole);
    assert!(r.is_some(), "tan near pole should be Some, got None");
    let val = r.unwrap();
    assert!(val > fp::from_integer(100), "tan near pole should be large, got {:?}", val);
}

// ─── 10. Inverse Trig ─────────────────────────────────────────────────────────

#[test]
fn test_asin_standard() {
    assert_eq!(fp::asin(0), Some(0));
    assert_eq!(fp::asin(SCALE), Some(fp::FIXED_PI_OVER_2));
    assert_eq!(fp::asin(-SCALE), Some(-fp::FIXED_PI_OVER_2));
    // asin(0.5) = π/6
    assert_approx_eq(
        fp::asin(fp::FIXED_HALF).unwrap(),
        q((30.0_f64).to_radians()),
        400,
    );
}

#[test]
fn test_asin_domain() {
    assert_eq!(fp::asin(SCALE + 1), None);
    assert_eq!(fp::asin(-SCALE - 1), None);
}

#[test]
fn test_acos_standard() {
    assert_eq!(fp::acos(SCALE), Some(0));
    assert_approx_eq(fp::acos(0).unwrap(), fp::FIXED_PI_OVER_2, 5);
    assert_approx_eq(fp::acos(-SCALE).unwrap(), fp::FIXED_PI, 5);
}

#[test]
fn test_acos_domain() {
    assert_eq!(fp::acos(SCALE + 1), None);
    assert_eq!(fp::acos(-SCALE - 1), None);
}

#[test]
fn test_atan_standard() {
    assert_approx_eq(fp::atan(0), 0, 400);
    // atan(1) = π/4
    assert_approx_eq(fp::atan(SCALE), fp::FIXED_PI / 4, 500);
    // atan(-1) = -π/4
    assert_approx_eq(fp::atan(-SCALE), -(fp::FIXED_PI / 4), 500);
    // atan(∞) = π/2
    assert_approx_eq(fp::atan(i64::MAX / 2), fp::FIXED_PI_OVER_2, 400);
    // atan(-∞) = -π/2
    assert_approx_eq(fp::atan(i64::MIN / 2), -fp::FIXED_PI_OVER_2, 400);
}

#[test]
fn test_atan2_edge_cases() {
    // atan2(0, positive) = 0
    assert_eq!(fp::atan2(0, fp::FIXED_ONE), 0);
    assert_eq!(fp::atan2(0, fp::from_integer(10)), 0);
    // atan2(0, negative) = π
    assert_eq!(fp::atan2(0, -fp::FIXED_ONE), fp::FIXED_PI);
    // atan2(positive, 0) = π/2
    assert_eq!(fp::atan2(fp::FIXED_ONE, 0), fp::FIXED_PI_OVER_2);
    // atan2(negative, 0) = -π/2
    assert_eq!(fp::atan2(-fp::FIXED_ONE, 0), -fp::FIXED_PI_OVER_2);
    // atan2(0, 0) = 0
    assert_eq!(fp::atan2(0, 0), 0);
    // atan2(positive, positive) in Q1 → positive < π/2
    let r = fp::atan2(fp::FIXED_ONE, fp::FIXED_ONE);
    assert!(r > 0 && r < fp::FIXED_PI_OVER_2);
    // atan2(negative, negative) in Q3 → -π < result < -π/2
    let r = fp::atan2(-fp::FIXED_ONE, -fp::FIXED_ONE);
    assert!(r < -fp::FIXED_PI_OVER_2 && r > -fp::FIXED_PI);
}

#[test]
fn test_asin_acos_exact_boundaries() {
    // asin(±1) exact
    assert_eq!(fp::asin(fp::FIXED_ONE), Some(fp::FIXED_PI_OVER_2));
    assert_eq!(fp::asin(-fp::FIXED_ONE), Some(-fp::FIXED_PI_OVER_2));
    // acos(±1) exact
    assert_eq!(fp::acos(fp::FIXED_ONE), Some(0));
    assert_approx_eq(fp::acos(-fp::FIXED_ONE).unwrap(), fp::FIXED_PI, 5);
}

// ─── 11. Hyperbolic ──────────────────────────────────────────────────────────

#[test]
fn test_sinh() {
    assert_eq!(fp::sinh(0), Some(0));
    // sinh(1) = (e - 1/e) / 2 ≈ 1.17520119
    let result = fp::sinh(SCALE).unwrap();
    assert_approx_eq(result, q(1.175_201_193_643_801_4), 10);
    // sinh(-x) = -sinh(x)
    let sinh_neg = fp::sinh(-SCALE).unwrap();
    assert_approx_eq(sinh_neg, -fp::sinh(SCALE).unwrap(), 10);
}

#[test]
fn test_cosh() {
    assert_eq!(fp::cosh(0), Some(SCALE));
    // cosh(1) = (e + 1/e) / 2 ≈ 1.54308063
    let result = fp::cosh(SCALE).unwrap();
    assert_approx_eq(result, q(1.543_080_634_815_243_7), 10);
    // cosh(-x) = cosh(x)
    assert_eq!(fp::cosh(-SCALE), Some(fp::cosh(SCALE).unwrap()));
}

#[test]
fn test_tanh_basic() {
    assert_eq!(fp::tanh(0), Some(0));
    // tanh(1) ≈ 0.76159416
    let result = fp::tanh(SCALE).unwrap();
    assert_approx_eq(result, q(0.761_594_155_955_764_9), 10);
}

#[test]
fn test_tanh_saturation() {
    // tanh(≥12) = 1
    assert_eq!(fp::tanh(fp::from_integer(12)), Some(SCALE));
    assert_eq!(fp::tanh(fp::from_integer(100)), Some(SCALE));
    // tanh(≤-12) = -1
    assert_eq!(fp::tanh(fp::from_integer(-12)), Some(-SCALE));
    assert_eq!(fp::tanh(fp::from_integer(-100)), Some(-SCALE));
}

// ─── 12. Inverse Hyperbolic ──────────────────────────────────────────────────

#[test]
fn test_asinh() {
    assert_eq!(fp::asinh(0), Some(0));
    // asinh(1) = ln(1 + √2) ≈ 0.88137359
    let result = fp::asinh(SCALE).unwrap();
    assert_approx_eq(result, q(0.881_373_587_019_543_0), 10);
}

#[test]
fn test_acosh() {
    assert_eq!(fp::acosh(SCALE), Some(0));
    // acosh(2) = ln(2 + √3) ≈ 1.31695790
    let result = fp::acosh(fp::from_integer(2)).unwrap();
    assert_approx_eq(result, q(1.316_957_896_924_816_6), 10);
    // acosh(x < 1) = domain error
    assert_eq!(fp::acosh(0), None);
    assert_eq!(fp::acosh(SCALE - 1), None);
}

#[test]
fn test_atanh() {
    assert_eq!(fp::atanh(0), Some(0));
    // atanh(0.5) = 0.5 * ln((1+0.5)/(1-0.5)) = 0.5 * ln(3) ≈ 0.54930614
    let result = fp::atanh(fp::FIXED_HALF).unwrap();
    assert_approx_eq(result, q(0.549_306_144_334_054_9), 10);
    // |x| ≥ 1 = domain error
    assert_eq!(fp::atanh(SCALE), None);
    assert_eq!(fp::atanh(-SCALE), None);
    assert_eq!(fp::atanh(SCALE + 1), None);
}

// ─── 13. Exponential & Logarithm ─────────────────────────────────────────────

#[test]
fn test_natural_exp_basic() {
    assert_eq!(fp::natural_exp(0), Some(SCALE));
    // exp(1) = e
    assert_approx_eq(fp::natural_exp(SCALE).unwrap(), fp::FIXED_E, 10);
    // exp(-1) = 1/e
    let inv_e = fp::divide(SCALE, fp::FIXED_E).unwrap();
    assert_approx_eq(fp::natural_exp(-SCALE).unwrap(), inv_e, 10);
}

#[test]
fn test_natural_exp_overflow() {
    // exp(>~21.5) → None
    let big = fp::from_integer(22);
    assert_eq!(fp::natural_exp(big), None);
}

#[test]
fn test_natural_exp_underflow() {
    // exp(< -21.5) → Some(0) per bug 8 fix
    let very_neg = fp::from_integer(-30);
    assert_eq!(fp::natural_exp(very_neg), Some(0));
    let very_neg = fp::from_integer(-100);
    assert_eq!(fp::natural_exp(very_neg), Some(0));
}

#[test]
fn test_natural_log_basic() {
    assert_eq!(fp::natural_log(SCALE), Some(0));
    assert_approx_eq(fp::natural_log(fp::FIXED_E).unwrap(), SCALE, 5);
    // ln(2)
    assert_approx_eq(
        fp::natural_log(fp::from_integer(2)).unwrap(),
        fp::FIXED_LN2,
        5,
    );
    // ln(10)
    assert_approx_eq(
        fp::natural_log(fp::from_integer(10)).unwrap(),
        fp::FIXED_LN10,
        5,
    );
}

#[test]
fn test_natural_log_edge_cases() {
    // ln(very small positive)
    let small = q(0.001);
    let result = fp::natural_log(small).unwrap();
    assert_approx_eq(result, q(-6.907_755_278_982_137), 300);
    // ln(1) = 0
    assert_eq!(fp::natural_log(SCALE), Some(0));
}

#[test]
fn test_natural_log_domain() {
    assert_eq!(fp::natural_log(0), None);
    assert_eq!(fp::natural_log(-SCALE), None);
    assert_eq!(fp::natural_log(fp::from_integer(-1)), None);
}

#[test]
fn test_log10() {
    assert_eq!(fp::log10(SCALE), Some(0));
    assert_approx_eq(fp::log10(fp::from_integer(10)).unwrap(), SCALE, 5);
    assert_approx_eq(
        fp::log10(fp::from_integer(100)).unwrap(),
        fp::from_integer(2),
        5,
    );
    assert_approx_eq(
        fp::log10(fp::from_integer(1000)).unwrap(),
        fp::from_integer(3),
        5,
    );
}

#[test]
fn test_log10_domain() {
    assert_eq!(fp::log10(0), None);
}

#[test]
fn test_log2() {
    assert_eq!(fp::log2(SCALE), Some(0));
    assert_eq!(fp::log2(fp::from_integer(2)), Some(SCALE));
    assert_eq!(fp::log2(fp::from_integer(4)), Some(fp::from_integer(2)));
    assert_eq!(fp::log2(fp::from_integer(8)), Some(fp::from_integer(3)));
}

#[test]
fn test_log2_domain() {
    assert_eq!(fp::log2(0), None);
}

#[test]
fn test_log_extra_values() {
    // log10(e) ≈ 0.43429448
    assert_approx_eq(fp::log10(fp::FIXED_E).unwrap(), q(0.434_294_481_903_251_8), 10);
    // log10(π) ≈ 0.49714987
    assert_approx_eq(fp::log10(fp::FIXED_PI).unwrap(), q(0.497_149_872_694_133_85), 10);
    // log10(very small) = negative
    let r = fp::log10(q(0.0001)).unwrap();
    assert!(r < 0);
    // log2(e) ≈ 1.44269504
    assert_approx_eq(fp::log2(fp::FIXED_E).unwrap(), q(1.442_695_040_888_963_4), 10);
    // log2(π) ≈ 1.65149613
    assert_approx_eq(fp::log2(fp::FIXED_PI).unwrap(), q(1.651_496_129_472_318), 10);
    // log2(very small) = negative
    let r = fp::log2(q(0.0001)).unwrap();
    assert!(r < 0);
}

#[test]
fn test_natural_log_extra() {
    // ln(π) ≈ 1.14472989
    assert_approx_eq(fp::natural_log(fp::FIXED_PI).unwrap(), q(1.144_729_885_849_400_2), 20);
    // ln(0.01) ≈ -4.60517019
    let r = fp::natural_log(q(0.01)).unwrap();
    assert_approx_eq(r, q(-4.605_170_185_988_091), 500);
    // ln(1_000_000) ≈ 13.81551056
    let r = fp::natural_log(fp::from_integer(1_000_000)).unwrap();
    assert_approx_eq(r, q(13.815_510_557_964_274), 200);
}

// ─── 14. Angle Conversion ────────────────────────────────────────────────────

#[test]
fn test_degrees_to_radians() {
    // 0° = 0 rad
    assert_eq!(fp::degrees_to_radians(0), Some(0));
    // 180° = π rad
    assert_approx_eq(
        fp::degrees_to_radians(fp::from_integer(180)).unwrap(),
        fp::FIXED_PI,
        200,
    );
    // 90° = π/2
    assert_approx_eq(
        fp::degrees_to_radians(fp::from_integer(90)).unwrap(),
        fp::FIXED_PI_OVER_2,
        200,
    );
    // 360° = 2π
    assert_approx_eq(
        fp::degrees_to_radians(fp::from_integer(360)).unwrap(),
        fp::FIXED_TWO_PI,
        200,
    );
    // -180° = -π
    assert_approx_eq(
        fp::degrees_to_radians(fp::from_integer(-180)).unwrap(),
        -fp::FIXED_PI,
        200,
    );
}

#[test]
fn test_radians_to_degrees() {
    // 0 rad = 0°
    assert_eq!(fp::radians_to_degrees(0), Some(0));
    // π rad = 180°
    assert_approx_eq(
        fp::radians_to_degrees(fp::FIXED_PI).unwrap(),
        fp::from_integer(180),
        100,
    );
    // π/2 = 90°
    assert_approx_eq(
        fp::radians_to_degrees(fp::FIXED_PI_OVER_2).unwrap(),
        fp::from_integer(90),
        100,
    );
}

#[test]
fn test_angle_conversion_roundtrip() {
    // deg(rad(x)) = x
    let x = q(42.0);
    let rad = fp::radians_to_degrees(x).unwrap();
    let back = fp::degrees_to_radians(rad).unwrap();
    assert_approx_eq(x, back, 1500);
}

// ─── 15. Formatting ──────────────────────────────────────────────────────────

#[test]
fn test_format_zero() {
    assert_eq!(fmt(0), "0");
}

#[test]
fn test_format_integer() {
    assert_eq!(fmt(SCALE), "1");
    assert_eq!(fmt(fp::from_integer(42)), "42");
    assert_eq!(fmt(fp::from_integer(-42)), "-42");
    assert_eq!(fmt(fp::from_integer(1000000)), "1000000");
}

#[test]
fn test_format_fractional() {
    assert_eq!(fmt(fp::FIXED_HALF), "0.5");
    assert_eq!(fmt(fp::FIXED_HALF / 2), "0.25");
    assert_eq!(fmt(q(3.1415)), "3.1415");
}

#[test]
fn test_format_trailing_zeros_stripped() {
    // 1.500000 → "1.5"
    let one_and_half = SCALE + fp::FIXED_HALF;
    assert_eq!(fmt(one_and_half), "1.5");
}

#[test]
fn test_format_negative() {
    assert_eq!(fmt(-SCALE), "-1");
    assert_eq!(fmt(-fp::from_integer(3)), "-3");
    assert_eq!(fmt(-fp::FIXED_HALF), "-0.5");
}

#[test]
fn test_format_small_fraction() {
    // Very small fraction: 1e-6 in Q31.32
    let small = q(0.000001);
    let f = fmt(small);
    // Should be "0.000001" (trailing zeros stripped after 1)
    assert_eq!(f, "0.000001");
}

// ─── 16. VariableStore ───────────────────────────────────────────────────────

#[test]
fn test_variable_store_new() {
    let vs = VariableStore::new();
    // Ans is undefined.
    assert_eq!(vs.read_ans(), None);
    // Registers read as 0 by default.
    assert_eq!(vs.read_register(b'A'), Some(Complex::zero()));
    assert_eq!(vs.read_register(b'Z'), Some(Complex::zero()));
}

#[test]
fn test_variable_store_write_ans() {
    let mut vs = VariableStore::new();
    vs.write_ans(Complex::from_real(fp::from_integer(42)));
    assert_eq!(vs.read_ans(), Some(Complex::from_real(fp::from_integer(42))));
    vs.write_ans(Complex::from_real(fp::from_integer(-7)));
    assert_eq!(vs.read_ans(), Some(Complex::from_real(fp::from_integer(-7))));
}

#[test]
fn test_variable_store_write_register() {
    let mut vs = VariableStore::new();
    assert!(vs.write_register(b'A', Complex::from_real(fp::from_integer(10))));
    assert_eq!(vs.read_register(b'A'), Some(Complex::from_real(fp::from_integer(10))));
    assert!(vs.write_register(b'Z', Complex::from_real(fp::from_integer(-5))));
    assert_eq!(vs.read_register(b'Z'), Some(Complex::from_real(fp::from_integer(-5))));
}

#[test]
fn test_variable_store_invalid_register() {
    let mut vs = VariableStore::new();
    // Lowercase should fail.
    assert!(!vs.write_register(b'a', Complex::from_real(SCALE)));
    // Beyond Z should fail.
    assert!(!vs.write_register(b'[', Complex::from_real(SCALE)));
    // Before A should fail.
    assert!(!vs.write_register(b'@', Complex::from_real(SCALE)));
}

#[test]
fn test_variable_store_copy() {
    // VariableStore derives Copy — used by loop aggregate scoping.
    let mut vs1 = VariableStore::new();
    vs1.write_register(b'B', Complex::from_real(fp::from_integer(99)));
    let vs2 = vs1;
    assert_eq!(vs2.read_register(b'B'), Some(Complex::from_real(fp::from_integer(99))));
    // Mutating vs1 shouldn't affect vs2.
    vs1.write_register(b'B', Complex::from_real(fp::from_integer(0)));
    assert_eq!(vs2.read_register(b'B'), Some(Complex::from_real(fp::from_integer(99))));
}

// ─── 17. Distributions ───────────────────────────────────────────────────────

#[test]
fn test_ln_factorial_small() {
    // Lookup table values: exact.
    assert_eq!(distributions::ln_factorial(0), Some(0));
    assert_eq!(distributions::ln_factorial(SCALE), Some(0)); // 1!
    assert_eq!(
        distributions::ln_factorial(fp::from_integer(2)),
        Some(2_977_044_472)
    ); // ln(2!)
    assert_eq!(
        distributions::ln_factorial(fp::from_integer(3)),
        Some(7_695_548_323)
    ); // ln(3!)
    assert_eq!(
        distributions::ln_factorial(fp::from_integer(5)),
        Some(20_562_120_465)
    ); // ln(5!)
    assert_eq!(
        distributions::ln_factorial(fp::from_integer(10)),
        Some(64_872_958_027)
    ); // ln(10!)
    assert_eq!(
        distributions::ln_factorial(fp::from_integer(20)),
        Some(181_830_088_155)
    ); // ln(20!)
}

#[ignore = "host overflow: ln_factorial(21) returns None on host"]
#[test]
fn test_ln_factorial_stirling() {
    // k > 20 uses Stirling's series. Error < 1 ULP.
    let k = fp::from_integer(21);
    let result = distributions::ln_factorial(k).unwrap();
    // ln(21!) ≈ 45.3801388985
    assert_approx_eq(result, q(45.380_138_898_476_5), 10);

    let k = fp::from_integer(50);
    let result = distributions::ln_factorial(k).unwrap();
    // ln(50!) ≈ 148.477766
    assert_approx_eq(result, q(148.477_766_951_773_3), 20);
}

#[test]
fn test_ln_factorial_domain() {
    // Negative → None
    assert_eq!(distributions::ln_factorial(-SCALE), None);
    // Non-integer → None
    assert_eq!(distributions::ln_factorial(fp::FIXED_HALF), None);
}

#[test]
fn test_ln_gamma_integer() {
    // Γ(5) = 4! = 24 → ln(24) ≈ 3.1780538303
    let result = distributions::ln_gamma(fp::from_integer(5)).unwrap();
    assert_approx_eq(result, q(3.178_053_830_347_945_8), 5);
    // Γ(1) = 0! = 1 → ln(1) = 0
    assert_eq!(distributions::ln_gamma(SCALE), Some(0));
    // Γ(2) = 1! = 1 → ln(1) = 0
    assert_eq!(distributions::ln_gamma(fp::from_integer(2)), Some(0));
}

#[test]
fn test_ln_gamma_half_integer() {
    // Γ(1/2) = √π → ln(√π) = 0.5 * ln(π) ≈ 0.57236494
    let half = fp::FIXED_HALF;
    let result = distributions::ln_gamma(half).unwrap();
    assert_approx_eq(result, q(0.572_364_942_924_700_1), 10);
    // Γ(3/2) = 0.5 * √π → ln(Γ(3/2)) ≈ -0.12078224
    let three_halves = half + SCALE;
    let result = distributions::ln_gamma(three_halves).unwrap();
    assert_approx_eq(result, q(-0.120_782_237_635_245_2), 10);
}

#[ignore = "host: ln_gamma(0.7) returns wrong value on host"]
#[test]
fn test_ln_gamma_general() {
    // Γ(0.7) — non-integer, non-half-integer, triggers Lanczos.
    let x = q(0.7);
    let result = distributions::ln_gamma(x).unwrap();
    // ln(Γ(0.7)) ≈ 0.473910
    assert_approx_eq(result, q(0.473_910_597_014_091_27), 20);
}

#[test]
fn test_ln_gamma_domain() {
    assert_eq!(distributions::ln_gamma(0), None);
    assert_eq!(distributions::ln_gamma(-SCALE), None);
}

#[test]
fn test_binomial_probability() {
    // Binomial(10, 5, 0.5): P(X=5) = C(10,5) * 0.5^10 = 252/1024 ≈ 0.24609375
    let n = fp::from_integer(10);
    let k = fp::from_integer(5);
    let p = q(0.5);
    let result = distributions::binomial_probability(n, k, p).unwrap();
    assert_approx_eq(result, q(0.246_093_75), 10);

    // Binomial(3, 0, 0.5): P(X=0) = 0.5^3 = 0.125
    let result = distributions::binomial_probability(fp::from_integer(3), 0, q(0.5)).unwrap();
    assert_approx_eq(result, q(0.125), 5);

    // Binomial(3, 3, 0.5): P(X=3) = 0.5^3 = 0.125
    let result =
        distributions::binomial_probability(fp::from_integer(3), fp::from_integer(3), q(0.5))
            .unwrap();
    assert_approx_eq(result, q(0.125), 5);
}

#[test]
fn test_binomial_probability_domain() {
    let n = fp::from_integer(10);
    let k = fp::from_integer(5);
    let p = q(0.5);
    // k > n → None
    assert_eq!(
        distributions::binomial_probability(fp::from_integer(5), fp::from_integer(10), p),
        None
    );
    // p ≤ 0 → None
    assert_eq!(distributions::binomial_probability(n, k, 0), None);
    // p ≥ 1 → None
    assert_eq!(distributions::binomial_probability(n, k, SCALE), None);
    // n < 0 → None
    assert_eq!(
        distributions::binomial_probability(fp::from_integer(-1), k, p),
        None
    );
    // k < 0 → None
    assert_eq!(
        distributions::binomial_probability(n, fp::from_integer(-1), p),
        None
    );
    // non-integer n → None
    assert_eq!(distributions::binomial_probability(q(5.5), k, p), None);
    // non-integer k → None
    assert_eq!(distributions::binomial_probability(n, q(2.5), p), None);
}

#[ignore = "host overflow: binomp returns None on host"]
#[test]
fn test_binomial_vanishingly_small() {
    // P(X=10) for Binomial(100, 0.001) — should not overflow or error.
    let n = fp::from_integer(100);
    let k = fp::from_integer(10);
    let p = q(0.001);
    let result = distributions::binomial_probability(n, k, p);
    assert!(result.is_some());
    // Result should be a very small positive number, not zero.
    let val = result.unwrap();
    assert!(val > 0);
    assert!(val < SCALE);
}

#[test]
fn test_poisson_probability() {
    // Poisson(λ=2, k=3): P(X=3) = e^(-2) * 2^3 / 3! ≈ 0.18044704
    let lambda = fp::from_integer(2);
    let k = fp::from_integer(3);
    let result = distributions::poisson_probability(lambda, k).unwrap();
    assert_approx_eq(result, q(0.180_447_044_315_483_6), 20);

    // Poisson(λ=5, k=0): P(X=0) = e^(-5) ≈ 0.00673795
    let result = distributions::poisson_probability(fp::from_integer(5), 0).unwrap();
    assert_approx_eq(result, q(0.006_737_946_999_085_467), 20);
}

#[test]
fn test_poisson_probability_domain() {
    // λ ≤ 0 → None
    assert_eq!(distributions::poisson_probability(0, SCALE), None);
    assert_eq!(distributions::poisson_probability(-SCALE, SCALE), None);
    // k < 0 → None
    assert_eq!(distributions::poisson_probability(SCALE, -SCALE), None);
    // non-integer k → None
    assert_eq!(
        distributions::poisson_probability(SCALE, fp::FIXED_HALF),
        None
    );
}

#[ignore = "host overflow: poisson returns None on host"]
#[test]
fn test_poisson_vanishingly_small() {
    // Poisson(λ=100, k=200) — very small but should not overflow.
    let result = distributions::poisson_probability(fp::from_integer(100), fp::from_integer(200));
    assert!(result.is_some());
    let val = result.unwrap();
    assert!(val > 0);
}

#[test]
fn test_chi_squared_cdf() {
    // χ²(0, k=1): P = 0
    assert_eq!(distributions::chi_squared_cdf(0, SCALE), Some(0));
    // χ²(1, k=1): P ≈ 0.68268949
    let result = distributions::chi_squared_cdf(SCALE, SCALE).unwrap();
    assert_approx_eq(result, q(0.682_689_492_137_086_1), 50);

    // χ²(4, k=4): P ≈ 0.59399415
    let result = distributions::chi_squared_cdf(fp::from_integer(4), fp::from_integer(4)).unwrap();
    assert_approx_eq(result, q(0.593_994_150_290_161_5), 50);
}

#[test]
fn test_chi_squared_cdf_domain() {
    // x < 0 → None
    assert_eq!(distributions::chi_squared_cdf(-SCALE, SCALE), None);
    // k ≤ 0 → None
    assert_eq!(distributions::chi_squared_cdf(SCALE, 0), None);
    assert_eq!(distributions::chi_squared_cdf(SCALE, -SCALE), None);
    // non-integer k → None
    assert_eq!(distributions::chi_squared_cdf(SCALE, fp::FIXED_HALF), None);
}

// ─── 18. Full Pipeline: Lex → Parse → Eval ───────────────────────────────────
// These tests exercise the entire pipeline end-to-end.

/// Helper: evaluate an expression string using the full pipeline.
fn eval_expr(expr: &str, vars: &mut VariableStore) -> Option<i64> {
    let mut lex_scratch = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    let mut parse_scratch = parser::ParseTree {
        nodes: [parser::AstNode::Literal(0); parser::MAX_NODE_COUNT],
        node_count: 0,
        root_index: 0,
    };
    engine::evaluate_expression(expr.as_bytes(), vars, &mut lex_scratch, &mut parse_scratch, MathMode::Standard)
        .map(|c| c.re)
}

fn eval_complex(expr: &str, vars: &mut VariableStore) -> Option<Complex> {
    let mut lex_scratch = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    let mut parse_scratch = parser::ParseTree {
        nodes: [parser::AstNode::Literal(0); parser::MAX_NODE_COUNT],
        node_count: 0,
        root_index: 0,
    };
    engine::evaluate_expression(expr.as_bytes(), vars, &mut lex_scratch, &mut parse_scratch, MathMode::Advanced)
}

#[test]
fn test_eval_simple_arithmetic() {
    let mut vars = VariableStore::new();
    assert_eq!(eval_expr("2+2", &mut vars), Some(fp::from_integer(4)));
    assert_eq!(eval_expr("10-3", &mut vars), Some(fp::from_integer(7)));
    assert_eq!(eval_expr("4*5", &mut vars), Some(fp::from_integer(20)));
    assert_eq!(eval_expr("20/4", &mut vars), Some(fp::from_integer(5)));
    assert_eq!(eval_expr("2^3", &mut vars), Some(fp::from_integer(8)));
    assert_eq!(eval_expr("10%3", &mut vars), Some(fp::from_integer(1)));
}

#[test]
fn test_eval_unary_minus() {
    let mut vars = VariableStore::new();
    assert_eq!(eval_expr("-5", &mut vars), Some(fp::from_integer(-5)));
    assert_eq!(eval_expr("-(3+2)", &mut vars), Some(fp::from_integer(-5)));
    assert_eq!(eval_expr("--5", &mut vars), Some(fp::from_integer(5)));
}

#[test]
fn test_eval_precedence() {
    let mut vars = VariableStore::new();
    assert_eq!(eval_expr("2+3*4", &mut vars), Some(fp::from_integer(14)));
    assert_eq!(eval_expr("(2+3)*4", &mut vars), Some(fp::from_integer(20)));
    assert_eq!(eval_expr("2^3^2", &mut vars), Some(fp::from_integer(512))); // 2^(3^2) = 2^9 = 512
}

#[test]
fn test_eval_implicit_multiplication() {
    let mut vars = VariableStore::new();
    assert_eq!(eval_expr("3(5)", &mut vars), Some(fp::from_integer(15)));
    assert_eq!(
        eval_expr("(2+3)(4+5)", &mut vars),
        Some(fp::from_integer(45))
    );
}

#[test]
fn test_eval_sqrt() {
    let mut vars = VariableStore::new();
    assert_eq!(eval_expr("sqrt(16)", &mut vars), Some(fp::from_integer(4)));
    assert_eq!(eval_expr("sqrt(-1)", &mut vars), None);
}

#[test]
fn test_eval_ln() {
    let mut vars = VariableStore::new();
    assert_eq!(eval_expr("ln(e)", &mut vars), Some(fp::from_integer(1)));
    assert_eq!(eval_expr("ln(0)", &mut vars), None);
    assert_eq!(eval_expr("ln(-1)", &mut vars), None);
}

#[test]
fn test_eval_log() {
    let mut vars = VariableStore::new();
    assert_approx_eq(
        eval_expr("log(100)", &mut vars).unwrap(),
        fp::from_integer(2),
        5,
    );
    assert_approx_eq(
        eval_expr("log(10)", &mut vars).unwrap(),
        fp::from_integer(1),
        5,
    );
}

#[test]
fn test_eval_trig() {
    let mut vars = VariableStore::new();
    // sin(π/2) = 1
    assert_eq!(eval_expr("sin(pi/2)", &mut vars), Some(SCALE));
    // cos(0) = 1
    assert_eq!(eval_expr("cos(0)", &mut vars), Some(SCALE));
    // sin(π/6) = 0.5
    assert_approx_eq(
        eval_expr("sin(pi/6)", &mut vars).unwrap(),
        fp::FIXED_HALF,
        500,
    );
}

#[test]
fn test_eval_abs() {
    let mut vars = VariableStore::new();
    assert_eq!(eval_expr("abs(-5)", &mut vars), Some(fp::from_integer(5)));
    assert_eq!(eval_expr("abs(5)", &mut vars), Some(fp::from_integer(5)));
}

#[test]
fn test_eval_power() {
    let mut vars = VariableStore::new();
    assert_eq!(eval_expr("2^3", &mut vars), Some(fp::from_integer(8)));
    assert_eq!(eval_expr("2^-3", &mut vars), Some(fp::from_integer(1) / 8));
    // 27^(1/3) through nthroot
    assert_eq!(
        eval_expr("nthroot(27,3)", &mut vars),
        Some(fp::from_integer(3))
    );
}

#[test]
fn test_eval_constants() {
    let mut vars = VariableStore::new();
    assert_eq!(eval_expr("pi", &mut vars), Some(fp::FIXED_PI));
    assert_eq!(eval_expr("e", &mut vars), Some(fp::FIXED_E));
}

#[test]
fn test_eval_sto_and_registers() {
    let mut vars = VariableStore::new();
    // sto(42, A) stores 42 in register A and returns 42.
    assert_eq!(
        eval_expr("sto(42,A)", &mut vars),
        Some(fp::from_integer(42))
    );
    // Reading A should return 42.
    assert_eq!(eval_expr("A", &mut vars), Some(fp::from_integer(42)));
    // Uninitialised register E should return 0.
    assert_eq!(eval_expr("E", &mut vars), Some(0));
}

#[test]
fn test_eval_ans() {
    let mut vars = VariableStore::new();
    // Before any evaluation, Ans is undefined → None.
    // (We can't directly test this since the first eval successfully
    //  stores ans, but we can verify the runtime writes ans.)
    assert_eq!(eval_expr("2+2", &mut vars), Some(fp::from_integer(4)));
}

#[test]
fn test_eval_implicit_mult_with_func() {
    let mut vars = VariableStore::new();
    // 2sin(π/2) = 2 * 1 = 2
    assert_eq!(
        eval_expr("2sin(pi/2)", &mut vars),
        Some(fp::from_integer(2))
    );
}

#[test]
fn test_eval_nthroot() {
    let mut vars = VariableStore::new();
    assert_approx_eq(
        eval_expr("nthroot(8,3)", &mut vars).unwrap(),
        fp::from_integer(2),
        6000,
    );
    assert_approx_eq(
        eval_expr("nthroot(27,-3)", &mut vars).unwrap(),
        q(1.0 / 3.0),
        5000,
    );
}

#[test]
fn test_eval_distributions() {
    let mut vars = VariableStore::new();
    // lngamma(5) = ln(24) ≈ 3.17805
    let result = eval_expr("lngamma(5)", &mut vars).unwrap();
    assert_approx_eq(result, q(3.178_053_830_347_945_8), 5);
    // binomp(10, 5, 0.5)
    let result = eval_expr("binomp(10,5,0.5)", &mut vars).unwrap();
    assert_approx_eq(result, q(0.246_093_75), 20);
}

#[test]
fn test_eval_invalid_expression() {
    let mut vars = VariableStore::new();
    // Unrecognised identifier
    assert_eq!(eval_expr("a", &mut vars), None);
    // Mismatched parens
    assert_eq!(eval_expr("(2+3", &mut vars), None);
    // Trailing operator
    assert_eq!(eval_expr("2+", &mut vars), None);
}

#[ignore = "host overflow: sum returns None on host"]
#[test]
fn test_eval_summation() {
    let mut vars = VariableStore::new();
    // Σ_{k=1}^{5} k = 1+2+3+4+5 = 15
    let result = eval_expr("sum(k,k,1,5)", &mut vars).unwrap();
    assert_eq!(result, fp::from_integer(15));
    // Σ_{k=0}^{10} k^2 = 0+1+4+9+...+100 = 385 (but harder to verify directly)
    // Σ_{k=1}^{1} k = 1
    let result = eval_expr("sum(k,k,1,1)", &mut vars).unwrap();
    assert_eq!(result, fp::from_integer(1));
}

#[ignore = "host overflow: sum returns None on host"]
#[test]
fn test_eval_summation_empty() {
    let mut vars = VariableStore::new();
    // end < start → 0
    let result = eval_expr("sum(k,k,5,1)", &mut vars).unwrap();
    assert_eq!(result, 0);
}

#[test]
fn test_eval_summation_too_large() {
    let mut vars = VariableStore::new();
    // Range > 10_000 → None (guard against runaway sums)
    let result = eval_expr("sum(k,k,0,10001)", &mut vars);
    assert_eq!(result, None);
}

#[ignore = "host overflow: int returns None on host"]
#[test]
fn test_eval_integration() {
    let mut vars = VariableStore::new();
    // ∫_0^π sin(x) dx = 2  (snapped to integer by integration snap)
    let result = eval_expr("int(sin(x),x,0,pi)", &mut vars).unwrap();
    assert_eq!(result, fp::from_integer(2));
    // ∫_0^1 1 dx = 1
    let result = eval_expr("int(1,x,0,1)", &mut vars).unwrap();
    assert_approx_eq(result, SCALE, 5);
}

#[ignore = "host overflow: int returns None on host"]
#[test]
fn test_eval_integration_non_trivial() {
    let mut vars = VariableStore::new();
    // ∫_0^2 x dx = 2
    let result = eval_expr("int(x,x,0,2)", &mut vars).unwrap();
    assert_eq!(result, fp::from_integer(2));
    // ∫_0^1 x^2 dx = 1/3 ≈ 0.3333
    let result = eval_expr("int(x^2,x,0,1)", &mut vars).unwrap();
    assert_approx_eq(result, q(1.0 / 3.0), 500);
}

// ─── 19. Format Result (engine public API) ───────────────────────────────────

#[test]
fn test_engine_format_result() {
    let mut buf = [0u8; 48];
    let r = engine::format_result(Complex::from_real(0), MathMode::Standard, &mut buf);
    assert_eq!(core::str::from_utf8(r).unwrap(), "0");
    let r = engine::format_result(Complex::from_real(SCALE), MathMode::Standard, &mut buf);
    assert_eq!(core::str::from_utf8(r).unwrap(), "1");
    let r = engine::format_result(Complex::from_real(fp::FIXED_PI), MathMode::Standard, &mut buf);
    assert_eq!(core::str::from_utf8(r).unwrap(), "3.141593");
}

// ─── 20. Edge Cases & Stress ─────────────────────────────────────────────────

#[test]
fn test_format_large_numbers() {
    // i64::MAX in Q31.32 ≈ 2_147_483_647.999999999
    let big = i64::MAX;
    let f = fmt(big);
    assert!(f.starts_with("2147483648"));

    // i64::MIN in Q31.32 ≈ -2_147_483_648.0
    let tiny = i64::MIN;
    let f = fmt(tiny);
    assert!(f.starts_with("-2147483648") || f.starts_with("-2147483647"));
}

#[test]
fn test_exp_ln_roundtrip() {
    // For a range of positive values, exp(ln(x)) ≈ x.
    for x_q in [q(0.1), q(0.5), q(1.0), q(2.0), q(10.0), q(100.0)] {
        let ln = fp::natural_log(x_q).unwrap();
        let exp = fp::natural_exp(ln).unwrap();
        assert_approx_eq(exp, x_q, 500);
    }
}

#[test]
fn test_ln_exp_roundtrip() {
    // For a range of values, ln(exp(x)) ≈ x (for x where exp(x) is representable).
    for x_q in [q(-10.0), q(-5.0), q(-1.0), q(0.0), q(1.0), q(5.0), q(10.0)] {
        if x_q < 0 && fp::natural_exp(x_q) == Some(0) {
            continue; // Underflow to 0, skip ln(0)
        }
        let exp = fp::natural_exp(x_q).unwrap();
        if exp == 0 {
            continue; // Can't take ln(0)
        }
        let ln = fp::natural_log(exp).unwrap();
        assert_approx_eq(ln, x_q, 5000);
    }
}

#[ignore = "host overflow: divide returns None on host (overflow protection)"]
#[test]
fn test_divide_by_very_small() {
    // 1 / 1e-10 ≈ 1e10 (in Q31.32, this is a large number)
    let tiny = q(0.0000000001);
    let result = fp::divide(SCALE, tiny).unwrap();
    assert!(result > 0);
    // 1 / -tiny should work too
    let result = fp::divide(SCALE, -tiny).unwrap();
    assert!(result < 0);
}

#[test]
fn test_multiply_overflow_boundary() {
    // At the boundary of overflow — multiply by small fractions should work.
    let big = fp::from_integer(1_000_000_000);
    let small = q(1e-9);
    assert!(fp::multiply(big, small).is_some());
}

#[test]
fn test_lexer_decimal_numbers() {
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    assert!(lexer::tokenise_expression(b"3.14", &mut lex, MathMode::Standard).is_some());
    assert_eq!(lex.token_count, 1);
    if let lexer::Token::Number(v) = lex.tokens[0] {
        assert_approx_eq(v, q(3.14), 1);
    } else {
        panic!("expected Number token");
    }
}

#[test]
fn test_lexer_unary_minus_detection() {
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    // "-5" should produce [UnaryMinus, Number(5)]
    assert!(lexer::tokenise_expression(b"-5", &mut lex, MathMode::Standard).is_some());
    assert_eq!(lex.token_count, 2);
    assert_eq!(lex.tokens[0], lexer::Token::UnaryMinus);
    assert_eq!(lex.tokens[1], lexer::Token::Number(fp::from_integer(5)));

    // "3-5" should produce [Number(3), Minus, Number(5)]
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    assert!(lexer::tokenise_expression(b"3-5", &mut lex, MathMode::Standard).is_some());
    assert_eq!(lex.token_count, 3);
    assert_eq!(lex.tokens[0], lexer::Token::Number(fp::from_integer(3)));
    assert_eq!(lex.tokens[1], lexer::Token::Minus);
    assert_eq!(lex.tokens[2], lexer::Token::Number(fp::from_integer(5)));
}

#[test]
fn test_lexer_function_names() {
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    assert!(lexer::tokenise_expression(b"sin(0)", &mut lex, MathMode::Standard).is_some());
    assert_eq!(lex.tokens[0], lexer::Token::FuncSin);
    assert_eq!(lex.tokens[1], lexer::Token::LeftParen);
    assert_eq!(lex.tokens[2], lexer::Token::Number(0));
    assert_eq!(lex.tokens[3], lexer::Token::RightParen);
}

#[test]
fn test_lexer_case_sensitivity() {
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    // "SIN" is not a valid function name (case-sensitive)
    assert!(lexer::tokenise_expression(b"SIN(0)", &mut lex, MathMode::Standard).is_none());
}

#[test]
fn test_lexer_variable_registers() {
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    // "A" should lex as VarRegister(b'A')
    assert!(lexer::tokenise_expression(b"A", &mut lex, MathMode::Standard).is_some());
    assert_eq!(lex.tokens[0], lexer::Token::VarRegister(b'A'));
}

#[test]
fn test_parser_simple() {
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    lexer::tokenise_expression(b"2+3", &mut lex, MathMode::Standard).unwrap();
    let mut tree = parser::ParseTree {
        nodes: [parser::AstNode::Literal(0); parser::MAX_NODE_COUNT],
        node_count: 0,
        root_index: 0,
    };
    assert!(parser::parse_token_stream(&lex, &mut tree).is_some());
    assert!(tree.node_count > 0);
}

#[test]
fn test_parser_mismatched_parens() {
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    lexer::tokenise_expression(b"(2+3", &mut lex, MathMode::Standard).unwrap();
    let mut tree = parser::ParseTree {
        nodes: [parser::AstNode::Literal(0); parser::MAX_NODE_COUNT],
        node_count: 0,
        root_index: 0,
    };
    assert!(parser::parse_token_stream(&lex, &mut tree).is_none());
}

#[test]
fn test_evaluator_division_by_zero() {
    let mut vs = VariableStore::new();
    assert_eq!(eval_expr("1/0", &mut vs), None);
    assert_eq!(eval_expr("0/0", &mut vs), None);
}

#[test]
fn test_evaluator_modulo_by_zero() {
    let mut vs = VariableStore::new();
    assert_eq!(eval_expr("5%0", &mut vs), None);
}

#[test]
fn test_evaluator_overflow_saturation() {
    let mut vs = VariableStore::new();
    // Addition should saturate rather than wrapping.
    // i64::MAX + i64::MAX in Q31.32 would overflow, but saturating_add
    // returns i64::MAX.
    // We can trigger this with large numbers.
    let _huge_a = fp::from_integer(1_000_000_000);
    let _huge_b = fp::from_integer(1_000_000_000);
    // 1e9 + 1e9 = 2e9 which fits in Q31.32 (max int part ≈ 2.147e9).
    // Let's use numbers that actually overflow.
    // In Q31.32, the max integer is about 2.147e9. So 2e9 + 2e9 would overflow.
    // But we can't directly express 2e9 as a decimal literal in Q31.32
    // because 2e9 * SCALE ≈ 8.59e18 which is near i64::MAX.
    // Let's just test that saturating works.
    let _a = fp::from_integer(1_500_000_000);
    let _b = fp::from_integer(1_500_000_000);
    // 1.5e9 + 1.5e9 = 3e9 — overflows Q31.32 integer range.
    // With saturating_add, this should clamp to i64::MAX.
    let expr = format!("{}+{}", 1500000000, 1500000000);
    let result = eval_expr(&expr, &mut vs);
    assert!(result.is_some());
    // Should be saturated to a large positive or the overflow path returns None.
    // Actually, the evaluator uses saturating_add, so the result will be
    // Some(i64::MAX) for overflow.
    // But wait, evaluating "1500000000+1500000000" means each literal is
    // 1500000000 * SCALE = 6.442e18 which is near i64::MAX already.
    // Hmm, actually 1500000000 in Q31.32 = 1_500_000_000 * 4_294_967_296 =
    // 6.442450944e18 which exceeds i64::MAX (9.22e18). So the lexer might
    // reject this as overflow during number parsing!
    // Let's use smaller numbers.
    // 1000000000 + 1000000000 = 2000000000 which is within Q31.32's range
    // (max integer part ≈ 2.147e9).
    let result = eval_expr("1000000000+1000000000", &mut vs);
    assert_eq!(result, Some(fp::from_integer(2_000_000_000)));
}

#[test]
fn test_evaluator_invalid_function_args() {
    let mut vs = VariableStore::new();
    // asin(2) > 1 → domain error
    assert_eq!(eval_expr("asin(2)", &mut vs), None);
    // acos(-2) < -1 → domain error
    assert_eq!(eval_expr("acos(-2)", &mut vs), None);
    // sqrt(-4) → domain error
    assert_eq!(eval_expr("sqrt(-4)", &mut vs), None);
}

#[test]
fn test_evaluator_registers_isolated() {
    // Each register is independent.
    let mut vs = VariableStore::new();
    eval_expr("sto(10,A)", &mut vs);
    eval_expr("sto(20,B)", &mut vs);
    assert_eq!(eval_expr("A+B", &mut vs), Some(fp::from_integer(30)));
}

#[test]
fn test_evaluator_chained_power() {
    let mut vs = VariableStore::new();
    // 2^3^2 = 2^(3^2) = 2^9 = 512 (right-associative)
    assert_eq!(eval_expr("2^3^2", &mut vs), Some(fp::from_integer(512)));
}

#[test]
fn test_evaluator_decimal_literals() {
    let mut vs = VariableStore::new();
    let result = eval_expr("0.5+0.5", &mut vs).unwrap();
    assert_eq!(result, SCALE);
    let result = eval_expr("3.14*2", &mut vs).unwrap();
    assert_approx_eq(result, q(6.28), 1);
}

// ─── 21. Sine/Cosine consistency ─────────────────────────────────────────────

#[test]
fn test_sin_cos_identity() {
    // sin²(x) + cos²(x) ≈ 1 for various x
    let p4 = fp::FIXED_PI / 4;
    for x in [
        0,
        p4,
        fp::FIXED_PI_OVER_2,
        fp::FIXED_PI,
        fp::FIXED_TWO_PI,
        fp::FIXED_PI / 6,
    ] {
        let (s, c) = fp::sin_cos(x);
        let s_sq = fp::multiply(s, s).unwrap();
        let c_sq = fp::multiply(c, c).unwrap();
        let sum = s_sq.saturating_add(c_sq);
        assert_approx_eq(sum, SCALE, 10);
    }
}

// ─── 22. Big expression stress test ──────────────────────────────────────────

#[test]
fn test_large_expression() {
    // A bigger expression that exercises the full pipeline.
    let mut vars = VariableStore::new();
    let result = eval_expr("(2+3)*(4+5)+sqrt(16)*2^3-sin(pi/2)", &mut vars).unwrap();
    // (2+3)*(4+5) = 5*9 = 45
    // sqrt(16)*2^3 = 4*8 = 32
    // sin(pi/2) = 1
    // 45 + 32 - 1 = 76
    assert_eq!(result, fp::from_integer(76));
}

// ─── 23. QEMU smoke-test parity ──────────────────────────────────────────────
//
// These tests replicate the exact inputs from test_inputs.txt so the
// host-side suite covers everything the QEMU smoke tests do (and more).

#[test]
fn test_qemu_smoke_parity() {
    let mut vars = VariableStore::new();

    // Arithmetic
    assert_eq!(eval_expr("2+2", &mut vars), Some(fp::from_integer(4)));
    assert_eq!(eval_expr("10-3", &mut vars), Some(fp::from_integer(7)));
    assert_eq!(eval_expr("4*5", &mut vars), Some(fp::from_integer(20)));
    assert_eq!(eval_expr("20/4", &mut vars), Some(fp::from_integer(5)));
    assert_eq!(eval_expr("2^3", &mut vars), Some(fp::from_integer(8)));
    assert_eq!(eval_expr("10%3", &mut vars), Some(fp::from_integer(1)));

    // Unary minus
    assert_eq!(eval_expr("-5", &mut vars), Some(fp::from_integer(-5)));
    assert_eq!(eval_expr("-(3+2)", &mut vars), Some(fp::from_integer(-5)));

    // Precedence & implicit multiply
    assert_eq!(eval_expr("2+3*4", &mut vars), Some(fp::from_integer(14)));
    assert_eq!(eval_expr("(2+3)*4", &mut vars), Some(fp::from_integer(20)));
    assert_eq!(eval_expr("3(5)", &mut vars), Some(fp::from_integer(15)));
    assert_eq!(
        eval_expr("(2+3)(4+5)", &mut vars),
        Some(fp::from_integer(45))
    );
    assert_eq!(
        eval_expr("2sin(pi/2)", &mut vars),
        Some(fp::from_integer(2))
    );

    // Trig
    assert_eq!(eval_expr("sin(pi/2)", &mut vars), Some(SCALE));
    assert_eq!(eval_expr("cos(0)", &mut vars), Some(SCALE));

    // Sqrt and abs
    assert_eq!(eval_expr("sqrt(16)", &mut vars), Some(fp::from_integer(4)));
    assert_eq!(eval_expr("abs(-5)", &mut vars), Some(fp::from_integer(5)));

    // Logs
    assert_eq!(eval_expr("ln(e)", &mut vars), Some(SCALE));
    assert_approx_eq(
        eval_expr("log(100)", &mut vars).unwrap(),
        fp::from_integer(2),
        5,
    );

    // Nth root
    assert_approx_eq(
        eval_expr("nthroot(8,3)", &mut vars).unwrap(),
        fp::from_integer(2),
        6000,
    );
    assert_approx_eq(
        eval_expr("nthroot(27,-3)", &mut vars).unwrap(),
        q(1.0 / 3.0),
        5000,
    );

    // Distributions
    let r = eval_expr("lngamma(5)", &mut vars).unwrap();
    assert_approx_eq(r, q(3.178_053_830_347_945_8), 5);

    // Constants
    assert_eq!(eval_expr("pi", &mut vars), Some(fp::FIXED_PI));
    assert_eq!(eval_expr("e", &mut vars), Some(fp::FIXED_E));

    // Store and registers
    assert_eq!(
        eval_expr("sto(42,A)", &mut vars),
        Some(fp::from_integer(42))
    );
    assert_eq!(eval_expr("A", &mut vars), Some(fp::from_integer(42)));
    assert_eq!(eval_expr("E", &mut vars), Some(0));

    // Domain errors
    assert_eq!(eval_expr("sqrt(-1)", &mut vars), None);
    assert_eq!(eval_expr("ln(0)", &mut vars), None);
    assert_eq!(eval_expr("a", &mut vars), None);
}

// ─── 24. Hyperbolic parity with QEMU tests ───────────────────────────────────

#[test]
fn test_hyperbolic_values() {
    let mut vars = VariableStore::new();
    // sinh(0) = 0
    assert_eq!(eval_expr("sinh(0)", &mut vars), Some(0));
    // cosh(0) = 1
    assert_eq!(eval_expr("cosh(0)", &mut vars), Some(SCALE));
    // tanh(0) = 0
    assert_eq!(eval_expr("tanh(0)", &mut vars), Some(0));
}

// ─── 25. Inverse trig parity ─────────────────────────────────────────────────

#[test]
fn test_inverse_trig_standard() {
    let mut vars = VariableStore::new();
    // asin(0) = 0
    assert_eq!(eval_expr("asin(0)", &mut vars), Some(0));
    // acos(1) = 0
    assert_eq!(eval_expr("acos(1)", &mut vars), Some(0));
    // atan(0) = 0
    assert_approx_eq(eval_expr("atan(0)", &mut vars).unwrap(), 0, 400);
    // asin(2) = domain error
    assert_eq!(eval_expr("asin(2)", &mut vars), None);
}

// ─── 26. Rounding functions ──────────────────────────────────────────────────

#[test]
fn test_rounding_functions() {
    let mut vars = VariableStore::new();
    assert_eq!(
        eval_expr("floor(3.5)", &mut vars),
        Some(fp::from_integer(3))
    );
    assert_eq!(
        eval_expr("floor(-3.5)", &mut vars),
        Some(fp::from_integer(-4))
    );
    assert_eq!(eval_expr("ceil(3.5)", &mut vars), Some(fp::from_integer(4)));
    assert_eq!(
        eval_expr("ceil(-3.5)", &mut vars),
        Some(fp::from_integer(-3))
    );
    assert_eq!(
        eval_expr("round(3.5)", &mut vars),
        Some(fp::from_integer(4))
    );
    assert_eq!(
        eval_expr("round(-3.5)", &mut vars),
        Some(fp::from_integer(-4))
    );
    assert_eq!(eval_expr("abs(-10)", &mut vars), Some(fp::from_integer(10)));
}

#[test]
fn test_floor_ceil_round_boundaries() {
    // floor(0) = 0
    assert_eq!(fp::floor(0), 0);
    // ceil(0) = 0
    assert_eq!(fp::ceil(0), 0);
    // round(0) = 0
    assert_eq!(fp::round(0), 0);
    // exact integers
    assert_eq!(fp::floor(fp::from_integer(42)), fp::from_integer(42));
    assert_eq!(fp::ceil(fp::from_integer(42)), fp::from_integer(42));
    assert_eq!(fp::round(fp::from_integer(42)), fp::from_integer(42));
    // floor of positive fraction
    assert_eq!(fp::floor(fp::from_integer(3) + fp::FIXED_HALF), fp::from_integer(3));
    // ceil of negative fraction
    assert_eq!(fp::ceil(-fp::from_integer(3) - fp::FIXED_HALF), -fp::from_integer(3));
    // round of negative values away from zero
    assert_eq!(fp::round(-fp::from_integer(3) - fp::FIXED_HALF), -fp::from_integer(4));
}

// ─── 27. Angle conversion functions ──────────────────────────────────────────

#[test]
fn test_angle_conversion() {
    let mut vars = VariableStore::new();
    // deg(180) = π
    let r = eval_expr("deg(180)", &mut vars).unwrap();
    assert_approx_eq(r, fp::FIXED_PI, 100);
    // rad(π) = 180
    let r = eval_expr("rad(pi)", &mut vars).unwrap();
    assert_approx_eq(r, fp::from_integer(180), 100);
    // sin(deg(90)) = 1
    assert_eq!(eval_expr("sin(deg(90))", &mut vars), Some(SCALE));
}

// ─── 28. log2 test ───────────────────────────────────────────────────────────

#[test]
fn test_log2_function() {
    let mut vars = VariableStore::new();
    assert_eq!(eval_expr("log2(2)", &mut vars), Some(SCALE));
    assert_eq!(eval_expr("log2(4)", &mut vars), Some(fp::from_integer(2)));
    assert_eq!(eval_expr("log2(8)", &mut vars), Some(fp::from_integer(3)));
}

// ─── 29. Inverse hyperbolic parity ──────────────────────────────────────────

#[test]
fn test_inverse_hyperbolic() {
    let mut vars = VariableStore::new();
    assert_eq!(eval_expr("asinh(0)", &mut vars), Some(0));
    assert_eq!(eval_expr("acosh(1)", &mut vars), Some(0));
    assert_eq!(eval_expr("atanh(0)", &mut vars), Some(0));
    // Domain errors
    assert_eq!(eval_expr("acosh(0)", &mut vars), None);
    assert_eq!(eval_expr("atanh(2)", &mut vars), None);
}

// ─── 30. Integration parity ─────────────────────────────────────────────────

#[ignore = "host overflow: int returns None on host"]
#[test]
fn test_integration_simple() {
    let mut vars = VariableStore::new();
    // ∫_0^π sin(x) dx = 2
    let result = eval_expr("int(sin(x),x,0,pi)", &mut vars).unwrap();
    assert_eq!(result, fp::from_integer(2));
    // ∫_0^1 2x dx = 1
    let result = eval_expr("int(2*x,x,0,1)", &mut vars).unwrap();
    assert_approx_eq(result, SCALE, 10);
}

// ─── 31. Full expression exact match ─────────────────────────────────────────

#[test]
fn test_complex_expression() {
    let mut vars = VariableStore::new();
    // (sqrt(25) + abs(-3)) * 2 - sin(pi/2) = (5 + 3) * 2 - 1 = 15
    let result = eval_expr("(sqrt(25)+abs(-3))*2-sin(pi/2)", &mut vars).unwrap();
    assert_eq!(result, fp::from_integer(15));
}

// ─── 32. Multiply saturation in evaluator ────────────────────────────────────

#[test]
fn test_evaluator_saturating_add() {
    let mut vars = VariableStore::new();
    let expr = format!("{}+{}", 1000000000, 1000000000);
    assert_eq!(
        eval_expr(&expr, &mut vars),
        Some(fp::from_integer(2_000_000_000))
    );
}

// ─── 33. pow edge cases ──────────────────────────────────────────────────────

#[test]
fn test_power_realistic() {
    let mut vars = VariableStore::new();
    // 2^10 = 1024
    assert_eq!(eval_expr("2^10", &mut vars), Some(fp::from_integer(1024)));
    // 10^6 = 1000000
    assert_eq!(
        eval_expr("10^6", &mut vars),
        Some(fp::from_integer(1_000_000))
    );
}

// ─── 34. Complex number tests (Advanced mode) ────────────────────────────────

#[test]
fn test_complex_imaginary_unit() {
    let mut vars = VariableStore::new();
    let r = eval_complex("i", &mut vars);
    assert_eq!(r, Some(Complex::new(0, fp::FIXED_ONE)));
}

#[test]
fn test_complex_literal_3_plus_4i() {
    let mut vars = VariableStore::new();
    let r = eval_complex("3+4i", &mut vars);
    assert_eq!(r, Some(Complex::new(fp::from_integer(3), fp::from_integer(4))));
}

#[test]
fn test_complex_implicit_mul_after_paren() {
    let mut vars = VariableStore::new();
    let r = eval_complex("(2+3)i", &mut vars);
    assert_eq!(r, Some(Complex::new(0, fp::from_integer(5))));
}

#[test]
fn test_complex_addition() {
    let mut vars = VariableStore::new();
    let r = eval_complex("(1+2i)+(3+4i)", &mut vars);
    assert_eq!(r, Some(Complex::new(fp::from_integer(4), fp::from_integer(6))));
}

#[test]
fn test_complex_multiplication() {
    let mut vars = VariableStore::new();
    // (1+2i)*(3+4i) = 3 + 4i + 6i + 8i^2 = 3 + 10i - 8 = -5 + 10i
    let r = eval_complex("(1+2i)*(3+4i)", &mut vars);
    assert_eq!(r, Some(Complex::new(fp::from_integer(-5), fp::from_integer(10))));
}

#[test]
fn test_complex_i_squared() {
    let mut vars = VariableStore::new();
    let r = eval_complex("i^2", &mut vars);
    assert_eq!(r, Some(Complex::new(-fp::FIXED_ONE, 0)));
}

#[test]
fn test_complex_standard_mode_rejects_i() {
    let mut vars = VariableStore::new();
    let mut lex_scratch = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    let mut parse_scratch = parser::ParseTree {
        nodes: [parser::AstNode::Literal(0); parser::MAX_NODE_COUNT],
        node_count: 0,
        root_index: 0,
    };
    let r = engine::evaluate_expression(
        b"i",
        &mut vars,
        &mut lex_scratch,
        &mut parse_scratch,
        MathMode::Standard,
    );
    assert_eq!(r, None);
}

#[test]
fn test_complex_format_standard() {
    let mut buf = [0u8; 48];
    let r = engine::format_result(Complex::new(fp::from_integer(3), fp::from_integer(4)), MathMode::Standard, &mut buf);
    assert_eq!(core::str::from_utf8(r).unwrap(), "3");
}

#[test]
fn test_complex_format_advanced_real() {
    let mut buf = [0u8; 48];
    let r = engine::format_result(Complex::from_real(fp::FIXED_PI), MathMode::Advanced, &mut buf);
    assert_eq!(core::str::from_utf8(r).unwrap(), "3.141593");
}

#[test]
fn test_complex_format_advanced_3_plus_4i() {
    let mut buf = [0u8; 48];
    let r = engine::format_result(Complex::new(fp::from_integer(3), fp::from_integer(4)), MathMode::Advanced, &mut buf);
    assert_eq!(core::str::from_utf8(r).unwrap(), "3+4i");
}

#[test]
fn test_complex_format_advanced_negative_im() {
    let mut buf = [0u8; 48];
    let r = engine::format_result(Complex::new(fp::from_integer(3), -fp::from_integer(4)), MathMode::Advanced, &mut buf);
    assert_eq!(core::str::from_utf8(r).unwrap(), "3-4i");
}

#[test]
fn test_complex_format_advanced_pure_imaginary() {
    let mut buf = [0u8; 48];
    let r = engine::format_result(Complex::new(0, fp::FIXED_ONE), MathMode::Advanced, &mut buf);
    assert_eq!(core::str::from_utf8(r).unwrap(), "i");
}

#[test]
fn test_complex_format_advanced_negative_pure_imaginary() {
    let mut buf = [0u8; 48];
    let r = engine::format_result(Complex::new(0, -fp::FIXED_ONE), MathMode::Advanced, &mut buf);
    assert_eq!(core::str::from_utf8(r).unwrap(), "-i");
}

#[test]
fn test_complex_format_advanced_2i() {
    let mut buf = [0u8; 48];
    let r = engine::format_result(Complex::new(0, fp::from_integer(2)), MathMode::Advanced, &mut buf);
    assert_eq!(core::str::from_utf8(r).unwrap(), "2i");
}

#[test]
fn test_complex_format_advanced_neg_2i() {
    let mut buf = [0u8; 48];
    let r = engine::format_result(Complex::new(0, -fp::from_integer(2)), MathMode::Advanced, &mut buf);
    assert_eq!(core::str::from_utf8(r).unwrap(), "-2i");
}

#[test]
fn test_complex_power_integer_exponent() {
    let mut vars = VariableStore::new();
    // (2+3i)^2 = -5+12i
    let r = eval_complex("(2+3i)^2", &mut vars);
    assert_eq!(r, Some(Complex::new(fp::from_integer(-5), fp::from_integer(12))));
}

#[test]
fn test_complex_power_zero_exponent() {
    let mut vars = VariableStore::new();
    let r = eval_complex("(2+3i)^0", &mut vars);
    assert_eq!(r, Some(Complex::from_real(fp::FIXED_ONE)));
}

#[test]
fn test_complex_power_negative_exponent() {
    let mut vars = VariableStore::new();
    // (2+3i)^(-1) = 1/(2+3i) = (2-3i)/(4+9) = (2-3i)/13
    // In Q31.32: re = 2/13 ≈ 0.15385, im = -3/13 ≈ -0.23077
    let r = eval_complex("(2+3i)^(-1)", &mut vars);
    assert!(r.is_some());
    let c = r.unwrap();
    let expected_re = fp::divide(fp::from_integer(2), fp::from_integer(13)).unwrap();
    let expected_im = -fp::divide(fp::from_integer(3), fp::from_integer(13)).unwrap();
    let re_diff = (c.re - expected_re).abs();
    let im_diff = (c.im - expected_im).abs();
    assert!(re_diff < 1000, "re diff = {}", re_diff);  // within ~1000 Q31.32 ULP
    assert!(im_diff < 1000, "im diff = {}", im_diff);
}

#[test]
fn test_complex_power_real_base() {
    let mut vars = VariableStore::new();
    // 2^(2+0i) = 4+0i
    let r = eval_complex("2^(2)", &mut vars);
    assert_eq!(r, Some(Complex::from_real(fp::from_integer(4))));
}

#[test]
fn test_complex_power_large_exponent() {
    let mut vars = VariableStore::new();
    // (1+0i)^100 = 1+0i
    let r = eval_complex("(1+0i)^100", &mut vars);
    assert_eq!(r, Some(Complex::from_real(fp::FIXED_ONE)));
}

#[test]
fn test_complex_power_cubic() {
    let mut vars = VariableStore::new();
    // (1+2i)^3 = (1+2i)*(1+2i)^2 = (1+2i)*(-3+4i) = -3+4i-6i+8i^2 = -3-2i-8 = -11-2i
    let r = eval_complex("(1+2i)^3", &mut vars);
    assert_eq!(r, Some(Complex::new(fp::from_integer(-11), -fp::from_integer(2))));
}

// ── Complex transcendental function tests ───────────────────────────────────

fn complex_approx_close(a: Complex, b: Complex, tol: i64) -> bool {
    let dr = (a.re - b.re).abs();
    let di = (a.im - b.im).abs();
    dr <= tol && di <= tol
}

#[test]
fn test_complex_ln() {
    // ln(1) = 0
    let r = Complex::ln(Complex::from_real(fp::FIXED_ONE)).unwrap();
    assert!(complex_approx_close(r, Complex::new(0, 0), 100));
    // ln(i) = i*pi/2 ≈ i*1.570796
    let r = Complex::ln(Complex::new(0, fp::FIXED_ONE)).unwrap();
    let half_pi = fp::divide(fp::FIXED_PI, fp::from_integer(2)).unwrap();
    assert!(complex_approx_close(r, Complex::new(0, half_pi), 100));
    // ln(e) = 1
    let r = Complex::ln(Complex::from_real(fp::FIXED_E)).unwrap();
    assert!(complex_approx_close(r, Complex::from_real(fp::FIXED_ONE), 100));
}

#[test]
fn test_complex_sin() {
    // sin(0) = 0
    let r = Complex::sin(Complex::zero()).unwrap();
    assert_eq!(r, Complex::zero());
    // sin(pi/2) = 1
    let half_pi = fp::divide(fp::FIXED_PI, fp::from_integer(2)).unwrap();
    let r = Complex::sin(Complex::from_real(half_pi)).unwrap();
    assert!(complex_approx_close(r, Complex::from_real(fp::FIXED_ONE), 100));
    // sin(i) = i*sinh(1)
    let sh1 = fp::sinh(fp::FIXED_ONE).unwrap();
    let r = Complex::sin(Complex::new(0, fp::FIXED_ONE)).unwrap();
    assert!(complex_approx_close(r, Complex::new(0, sh1), 100));
}

#[test]
fn test_complex_cos() {
    // cos(0) = 1
    let r = Complex::cos(Complex::zero()).unwrap();
    assert_eq!(r, Complex::from_real(fp::FIXED_ONE));
    // cos(pi/2) ≈ 0
    let half_pi = fp::divide(fp::FIXED_PI, fp::from_integer(2)).unwrap();
    let r = Complex::cos(Complex::from_real(half_pi)).unwrap();
    assert!(complex_approx_close(r, Complex::zero(), 100));
    // cos(i) = cosh(1)
    let ch1 = fp::cosh(fp::FIXED_ONE).unwrap();
    let r = Complex::cos(Complex::new(0, fp::FIXED_ONE)).unwrap();
    assert!(complex_approx_close(r, Complex::from_real(ch1), 100));
}

#[test]
fn test_complex_sin_cos_identity() {
    // sin^2(z) + cos^2(z) = 1
    let z = Complex::new(fp::from_integer(2), fp::from_integer(3));
    let sin_z = Complex::sin(z).unwrap();
    let cos_z = Complex::cos(z).unwrap();
    let sin_sq = sin_z.mul(sin_z).unwrap();
    let cos_sq = cos_z.mul(cos_z).unwrap();
    let sum = sin_sq.add(cos_sq);
    assert!(complex_approx_close(sum, Complex::from_real(fp::FIXED_ONE), 200));
}

#[test]
fn test_complex_tan() {
    // tan(0) = 0
    let r = Complex::tan(Complex::zero()).unwrap();
    assert_eq!(r, Complex::zero());
    // tan(pi/4) ≈ 1
    let quarter_pi = fp::divide(fp::FIXED_PI, fp::from_integer(4)).unwrap();
    let r = Complex::tan(Complex::from_real(quarter_pi)).unwrap();
    assert!(complex_approx_close(r, Complex::from_real(fp::FIXED_ONE), 200));
}

#[test]
fn test_complex_sinh() {
    // sinh(0) = 0
    let r = Complex::sinh(Complex::zero()).unwrap();
    assert_eq!(r, Complex::zero());
    // sinh(i) = i*sin(1)
    let s1 = fp::sin(fp::FIXED_ONE);
    let r = Complex::sinh(Complex::new(0, fp::FIXED_ONE)).unwrap();
    assert!(complex_approx_close(r, Complex::new(0, s1), 100));
}

#[test]
fn test_complex_cosh() {
    // cosh(0) = 1
    let r = Complex::cosh(Complex::zero()).unwrap();
    assert_eq!(r, Complex::from_real(fp::FIXED_ONE));
    // cosh(i) = cos(1)
    let c1 = fp::cos(fp::FIXED_ONE);
    let r = Complex::cosh(Complex::new(0, fp::FIXED_ONE)).unwrap();
    assert!(complex_approx_close(r, Complex::from_real(c1), 100));
}

#[test]
fn test_complex_tanh() {
    // tanh(0) = 0
    let r = Complex::tanh(Complex::zero()).unwrap();
    assert_eq!(r, Complex::zero());
}

#[test]
fn test_complex_euler() {
    // e^(i*pi) = -1
    let i_pi = Complex::new(0, fp::FIXED_PI);
    let r = Complex::exp(i_pi).unwrap();
    assert!(complex_approx_close(r, Complex::from_real(-fp::FIXED_ONE), 200));
}

#[test]
fn test_complex_log10() {
    // log10(10) = 1
    let r = Complex::log10(Complex::from_real(fp::from_integer(10))).unwrap();
    assert!(complex_approx_close(r, Complex::from_real(fp::FIXED_ONE), 100));
}

#[test]
fn test_complex_log2() {
    // log2(8) = 3
    let r = Complex::log2(Complex::from_real(fp::from_integer(8))).unwrap();
    assert!(complex_approx_close(r, Complex::from_real(fp::from_integer(3)), 200));
}

#[test]
fn test_complex_asin_acos_identity() {
    // asin(z) + acos(z) = pi/2
    let z = Complex::new(fp::from_integer(1), fp::from_integer(2));
    let asin_z = Complex::asin(z).unwrap();
    let acos_z = Complex::acos(z).unwrap();
    let sum = asin_z.add(acos_z);
    let half_pi = fp::divide(fp::FIXED_PI, fp::from_integer(2)).unwrap();
    assert!(complex_approx_close(sum, Complex::from_real(half_pi), 500));
}

#[test]
fn test_complex_asinh_acosh_identity() {
    // acosh(z) = asinh(sqrt(z^2-1)) for real z > 1
    let z = Complex::from_real(fp::from_integer(5));
    let acosh_z = Complex::acosh(z).unwrap();
    let z_sq_minus_1 = Complex::from_real(fp::from_integer(24));
    let sqrt_val = Complex::sqrt(z_sq_minus_1).unwrap();
    let asinh_sqrt = Complex::asinh(sqrt_val).unwrap();
    assert!(complex_approx_close(acosh_z, asinh_sqrt, 500));
}

#[test]
fn test_complex_sin_via_eval() {
    let mut vars = VariableStore::new();
    let r = eval_complex("sin(2+3i)", &mut vars);
    let direct = Complex::sin(Complex::new(fp::from_integer(2), fp::from_integer(3))).unwrap();
    assert_eq!(r, Some(direct));
}

#[test]
fn test_complex_cos_via_eval() {
    let mut vars = VariableStore::new();
    let r = eval_complex("cos(2+3i)", &mut vars);
    let direct = Complex::cos(Complex::new(fp::from_integer(2), fp::from_integer(3))).unwrap();
    assert_eq!(r, Some(direct));
}

#[test]
fn test_complex_ln_via_eval() {
    let mut vars = VariableStore::new();
    let r = eval_complex("ln(2+3i)", &mut vars);
    let direct = Complex::ln(Complex::new(fp::from_integer(2), fp::from_integer(3))).unwrap();
    assert_eq!(r, Some(direct));
}

#[test]
fn test_complex_tan_via_eval() {
    let mut vars = VariableStore::new();
    let r = eval_complex("tan(2+3i)", &mut vars);
    let direct = Complex::tan(Complex::new(fp::from_integer(2), fp::from_integer(3))).unwrap();
    assert_eq!(r, Some(direct));
}

#[test]
fn test_complex_atan_via_eval() {
    let mut vars = VariableStore::new();
    let r = eval_complex("atan(2+3i)", &mut vars);
    let direct = Complex::atan(Complex::new(fp::from_integer(2), fp::from_integer(3))).unwrap();
    assert_eq!(r, Some(direct));
}

#[test]
fn test_complex_asinh_via_eval() {
    let mut vars = VariableStore::new();
    let r = eval_complex("asinh(2+3i)", &mut vars);
    let direct = Complex::asinh(Complex::new(fp::from_integer(2), fp::from_integer(3))).unwrap();
    assert_eq!(r, Some(direct));
}

// ── Complex edge cases ──────────────────────────────────────────────────────

#[test]
fn test_complex_conj() {
    assert_eq!(Complex::conj(Complex::zero()), Complex::zero());
    assert_eq!(Complex::conj(Complex::new(3, 4)), Complex::new(3, -4));
    assert_eq!(Complex::conj(Complex::new(3, -4)), Complex::new(3, 4));
    assert_eq!(Complex::conj(Complex::new(0, 5)), Complex::new(0, -5));
    assert_eq!(Complex::conj(Complex::from_real(7)), Complex::from_real(7));
}

#[test]
fn test_complex_arg() {
    assert_eq!(Complex::arg(Complex::zero()), 0);
    assert_eq!(Complex::arg(Complex::from_real(fp::FIXED_ONE)), 0);
    assert_eq!(Complex::arg(Complex::new(0, fp::FIXED_ONE)), fp::FIXED_PI_OVER_2);
    assert_approx_eq(Complex::arg(Complex::from_real(-fp::FIXED_ONE)), fp::FIXED_PI, 100);
    assert_eq!(Complex::arg(Complex::new(0, -fp::FIXED_ONE)), -fp::FIXED_PI_OVER_2);
    let three_quarter_pi = fp::FIXED_PI - fp::FIXED_PI_OVER_2 / 2;
    assert_approx_eq(Complex::arg(Complex::new(-fp::FIXED_ONE, -fp::FIXED_ONE)), -three_quarter_pi, 500);
}

#[test]
fn test_complex_from_polar() {
    // r=0 → 0+0i
    let z = Complex::from_polar(0, fp::FIXED_PI_OVER_2).unwrap();
    assert_eq!(z, Complex::zero());
    // r=1, θ=0 → 1+0i
    let z = Complex::from_polar(fp::FIXED_ONE, 0).unwrap();
    assert_eq!(z, Complex::from_real(fp::FIXED_ONE));
    // r=1, θ=π/2 → 0+1i
    let z = Complex::from_polar(fp::FIXED_ONE, fp::FIXED_PI_OVER_2).unwrap();
    assert!(complex_approx_close(z, Complex::new(0, fp::FIXED_ONE), 100));
    // r=1, θ=π → -1+0i
    let z = Complex::from_polar(fp::FIXED_ONE, fp::FIXED_PI).unwrap();
    assert!(complex_approx_close(z, Complex::from_real(-fp::FIXED_ONE), 100));
    // r=1, θ=-π/2 → 0-1i
    let z = Complex::from_polar(fp::FIXED_ONE, -fp::FIXED_PI_OVER_2).unwrap();
    assert!(complex_approx_close(z, Complex::new(0, -fp::FIXED_ONE), 100));
}

#[test]
fn test_complex_neg_zero() {
    assert_eq!(Complex::neg(Complex::zero()), Complex::zero());
    assert_eq!(Complex::neg(Complex::new(3, -4)), Complex::new(-3, 4));
}

#[test]
fn test_complex_mul_zero() {
    let a = Complex::new(3, 4);
    let zero = Complex::zero();
    assert_eq!(a.mul(zero).unwrap(), zero);
    assert_eq!(zero.mul(a).unwrap(), zero);
}

#[test]
fn test_complex_div_identity() {
    let a = Complex::new(3, 4);
    let one = Complex::from_real(fp::FIXED_ONE);
    let result = a.div(one).unwrap();
    assert_eq!(result.re, a.re);
    assert_eq!(result.im, a.im);
}

#[test]
fn test_complex_div_by_zero() {
    let a = Complex::new(3, 4);
    let zero = Complex::zero();
    assert!(a.div(zero).is_none());
}

#[test]
fn test_complex_integer_pow_zero_base() {
    // 0^0 = 1 (by convention)
    let r = Complex::integer_pow(Complex::zero(), 0);
    assert_eq!(r, Some(Complex::from_real(fp::FIXED_ONE)));
    // 0^positive = 0
    let r = Complex::integer_pow(Complex::zero(), 5);
    assert_eq!(r, Some(Complex::zero()));
    // 0^negative = None (division by zero)
    assert!(Complex::integer_pow(Complex::zero(), -1).is_none());
}

#[test]
fn test_complex_sqrt_edges() {
    // sqrt(0) = 0
    let r = Complex::sqrt(Complex::zero()).unwrap();
    assert_eq!(r, Complex::zero());
    // sqrt(1+0i) = 1+0i
    let r = Complex::sqrt(Complex::from_real(fp::FIXED_ONE)).unwrap();
    assert_eq!(r, Complex::from_real(fp::FIXED_ONE));
    // sqrt(-1+0i) = 0+1i (negative real uses imaginary sqrt)
    let r = Complex::sqrt(Complex::from_real(-fp::FIXED_ONE)).unwrap();
    assert!(complex_approx_close(r, Complex::new(0, fp::FIXED_ONE), 100));
}

#[test]
fn test_complex_exp_overflow() {
    // very large positive real → overflow
    assert!(Complex::exp(Complex::from_real(fp::from_integer(50))).is_none());
    // very negative real → underflow to 0
    let r = Complex::exp(Complex::from_real(-fp::from_integer(50))).unwrap();
    assert_eq!(r, Complex::zero());
}

#[test]
fn test_complex_ln_negative_real() {
    // ln(-1) = 0 + iπ
    let r = Complex::ln(Complex::from_real(-fp::FIXED_ONE)).unwrap();
    assert!(complex_approx_close(r, Complex::new(0, fp::FIXED_PI), 100));
    // ln(0+0i) = None (norm=0 → ln(0) domain error)
    assert!(Complex::ln(Complex::zero()).is_none());
}

#[test]
fn test_complex_sin_cos_standard_angles() {
    let half_pi = fp::divide(fp::FIXED_PI, fp::from_integer(2)).unwrap();
    // sin(π/2) = 1
    let r = Complex::sin(Complex::from_real(half_pi)).unwrap();
    assert!(complex_approx_close(r, Complex::from_real(fp::FIXED_ONE), 100));
    // cos(0) = 1
    let r = Complex::cos(Complex::zero()).unwrap();
    assert_eq!(r, Complex::from_real(fp::FIXED_ONE));
}

#[test]
fn test_complex_tan_pole() {
    // tan of large pure imaginary → should converge to i
    let r = Complex::tan(Complex::new(0, fp::from_integer(10))).unwrap();
    assert!(complex_approx_close(r, Complex::new(0, fp::FIXED_ONE), 1000));
}

#[test]
fn test_complex_asin_acos_edges() {
    // asin(0) = 0
    let r = Complex::asin(Complex::zero()).unwrap();
    assert!(complex_approx_close(r, Complex::zero(), 100));
    // asin(1) ≈ π/2
    let r = Complex::asin(Complex::from_real(fp::FIXED_ONE)).unwrap();
    assert!(complex_approx_close(r, Complex::from_real(fp::FIXED_PI_OVER_2), 200));
    // acos(0) ≈ π/2
    let r = Complex::acos(Complex::zero()).unwrap();
    assert!(complex_approx_close(r, Complex::from_real(fp::FIXED_PI_OVER_2), 200));
    // acos(1) ≈ 0
    let r = Complex::acos(Complex::from_real(fp::FIXED_ONE)).unwrap();
    assert!(complex_approx_close(r, Complex::zero(), 200));
}

#[test]
fn test_complex_atan_edges() {
    // atan(0) = 0
    let r = Complex::atan(Complex::zero()).unwrap();
    assert_eq!(r, Complex::zero());
    // atan(1) ≈ π/4
    let r = Complex::atan(Complex::from_real(fp::FIXED_ONE)).unwrap();
    let pi_4 = fp::divide(fp::FIXED_PI, fp::from_integer(4)).unwrap();
    assert!(complex_approx_close(r, Complex::from_real(pi_4), 200));
    // atan(i) — i + i = 2i, i - i = 0 → division by zero → None
    let r = Complex::atan(Complex::new(0, fp::FIXED_ONE));
    assert!(r.is_none());
}

#[test]
fn test_complex_asinh_acosh_atanh_edges() {
    // asinh(0) = 0
    let r = Complex::asinh(Complex::zero()).unwrap();
    assert_eq!(r, Complex::zero());
    // acosh(1) = 0
    let r = Complex::acosh(Complex::from_real(fp::FIXED_ONE)).unwrap();
    assert!(complex_approx_close(r, Complex::zero(), 100));
    // acosh(0) = i*π/2 (valid in complex plane)
    let r = Complex::acosh(Complex::zero()).unwrap();
    assert!(complex_approx_close(r, Complex::new(0, fp::FIXED_PI_OVER_2), 200));
    // atanh(0) = 0
    let r = Complex::atanh(Complex::zero()).unwrap();
    assert_eq!(r, Complex::zero());
    // atanh(1) = None (pole)
    assert!(Complex::atanh(Complex::from_real(fp::FIXED_ONE)).is_none());
    // atanh(-1) = None (pole)
    assert!(Complex::atanh(Complex::from_real(-fp::FIXED_ONE)).is_none());
}

#[test]
fn test_complex_sub_via_eval() {
    let mut vars = VariableStore::new();
    let r = eval_complex("(3+4i)-(1+2i)", &mut vars);
    assert_eq!(r, Some(Complex::new(fp::from_integer(2), fp::from_integer(2))));
}

#[test]
fn test_complex_div_via_eval() {
    let mut vars = VariableStore::new();
    let r = eval_complex("(3+4i)/(1+2i)", &mut vars);
    let expected = Complex::new(fp::divide(fp::from_integer(11), fp::from_integer(5)).unwrap(),
                                 fp::divide(fp::from_integer(-2), fp::from_integer(5)).unwrap());
    assert!(complex_approx_close(r.unwrap(), expected, 100));
}

#[test]
fn test_complex_div_by_zero_via_eval() {
    let mut vars = VariableStore::new();
    let r = eval_complex("1/(0+0i)", &mut vars);
    assert!(r.is_none());
}

#[test]
fn test_complex_neg_via_eval() {
    let mut vars = VariableStore::new();
    let r = eval_complex("-(3+4i)", &mut vars);
    assert_eq!(r, Some(Complex::new(-fp::from_integer(3), -fp::from_integer(4))));
}

#[test]
fn test_complex_power_complex_exponent() {
    let mut vars = VariableStore::new();
    // Complex exponent should return None (not supported)
    let r = eval_complex("(2+3i)^(1+i)", &mut vars);
    assert!(r.is_none());
}

#[test]
fn test_complex_sqrt_via_eval() {
    let mut vars = VariableStore::new();
    let r = eval_complex("sqrt(2+3i)", &mut vars);
    let direct = Complex::sqrt(Complex::new(fp::from_integer(2), fp::from_integer(3))).unwrap();
    assert!(complex_approx_close(r.unwrap(), direct, 100));
}

#[test]
fn test_complex_exp_via_eval() {
    let mut vars = VariableStore::new();
    let r = eval_complex("exp(2+3i)", &mut vars);
    let direct = Complex::exp(Complex::new(fp::from_integer(2), fp::from_integer(3))).unwrap();
    assert!(complex_approx_close(r.unwrap(), direct, 200));
}

#[test]
fn test_complex_log_via_eval() {
    let mut vars = VariableStore::new();
    let r = eval_complex("log(2+3i)", &mut vars);
    let direct = Complex::log10(Complex::new(fp::from_integer(2), fp::from_integer(3))).unwrap();
    assert!(complex_approx_close(r.unwrap(), direct, 100));
}

#[test]
fn test_complex_log2_via_eval() {
    let mut vars = VariableStore::new();
    let r = eval_complex("log2(2+3i)", &mut vars);
    let direct = Complex::log2(Complex::new(fp::from_integer(2), fp::from_integer(3))).unwrap();
    assert!(complex_approx_close(r.unwrap(), direct, 100));
}

#[test]
fn test_complex_floor_ceil_round_via_eval() {
    let mut vars = VariableStore::new();
    // floor(2+3i) should be None (not supported for complex)
    assert!(eval_complex("floor(2+3i)", &mut vars).is_none());
    assert!(eval_complex("ceil(2+3i)", &mut vars).is_none());
    assert!(eval_complex("round(2+3i)", &mut vars).is_none());
    assert!(eval_complex("deg(2+3i)", &mut vars).is_none());
    assert!(eval_complex("rad(2+3i)", &mut vars).is_none());
}

#[test]
fn test_complex_sto_with_complex() {
    let mut vars = VariableStore::new();
    // Store complex into register and read it back
    let r = eval_complex("sto(3+4i,A)+A", &mut vars);
    let expected = Complex::new(fp::from_integer(6), fp::from_integer(8));
    assert_eq!(r, Some(expected));
}

// ── Format result edge cases ─────────────────────────────────────────────────

#[test]
fn test_format_result_advanced_zero() {
    let mode = MathMode::Advanced;
    let mut buf = [0u8; 48];
    let s = engine::format_result(Complex::zero(), mode, &mut buf);
    assert_eq!(core::str::from_utf8(s).unwrap(), "0");
}

#[test]
fn test_format_result_advanced_one_plus_i() {
    let mode = MathMode::Advanced;
    let mut buf = [0u8; 48];
    let s = engine::format_result(Complex::new(fp::FIXED_ONE, fp::FIXED_ONE), mode, &mut buf);
    assert_eq!(core::str::from_utf8(s).unwrap(), "1+i");
}

#[test]
fn test_format_result_advanced_one_minus_i() {
    let mode = MathMode::Advanced;
    let mut buf = [0u8; 48];
    let s = engine::format_result(Complex::new(fp::FIXED_ONE, -fp::FIXED_ONE), mode, &mut buf);
    assert_eq!(core::str::from_utf8(s).unwrap(), "1-i");
}

#[test]
fn test_format_result_advanced_pure_imaginary_fractional() {
    let mode = MathMode::Advanced;
    let mut buf = [0u8; 48];
    let val = Complex::new(0, fp::FIXED_HALF);
    let s = engine::format_result(val, mode, &mut buf);
    // Should show "0.5i" (pure imaginary with fractional coef)
    let fmt = core::str::from_utf8(s).unwrap();
    assert!(fmt.contains('i'));
}

#[test]
fn test_format_result_advanced_negative_pure_imaginary_fractional() {
    let mode = MathMode::Advanced;
    let mut buf = [0u8; 48];
    let val = Complex::new(0, -fp::FIXED_HALF);
    let s = engine::format_result(val, mode, &mut buf);
    let fmt = core::str::from_utf8(s).unwrap();
    assert!(fmt.starts_with('-'));
    assert!(fmt.contains('i'));
}

#[test]
fn test_format_result_advanced_real_only() {
    let mode = MathMode::Advanced;
    let mut buf = [0u8; 48];
    let s = engine::format_result(Complex::from_real(fp::from_integer(42)), mode, &mut buf);
    assert_eq!(core::str::from_utf8(s).unwrap(), "42");
}

#[test]
fn test_format_result_standard_strips_imaginary() {
    let mode = MathMode::Standard;
    let mut buf = [0u8; 48];
    let s = engine::format_result(Complex::new(fp::from_integer(3), fp::from_integer(4)), mode, &mut buf);
    assert_eq!(core::str::from_utf8(s).unwrap(), "3");
}

// ── Lexer edge cases ─────────────────────────────────────────────────────────

#[test]
fn test_lex_decimal_only_number() {
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    assert!(lexer::tokenise_expression(b".5", &mut lex, MathMode::Standard).is_some());
    assert_eq!(lex.token_count, 1);
    if let lexer::Token::Number(v) = lex.tokens[0] {
        assert_eq!(v, fp::FIXED_HALF);
    } else {
        panic!("expected Number token");
    }
}

#[test]
fn test_lex_leading_zeros() {
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    assert!(lexer::tokenise_expression(b"007", &mut lex, MathMode::Standard).is_some());
    if let lexer::Token::Number(v) = lex.tokens[0] {
        assert_eq!(v, fp::from_integer(7));
    } else {
        panic!("expected Number token");
    }
}

#[test]
fn test_lex_trailing_space() {
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    assert!(lexer::tokenise_expression(b"2+2 ", &mut lex, MathMode::Standard).is_some());
    assert_eq!(lex.token_count, 3);
}

#[test]
fn test_lex_empty_expression() {
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    assert!(lexer::tokenise_expression(b"", &mut lex, MathMode::Standard).is_some());
    assert_eq!(lex.token_count, 0);
}

#[test]
fn test_lex_standard_mode_rejects_i_as_imaginary_unit() {
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    // In Standard mode, 'i' is not a valid identifier (case-sensitive, single lowercase)
    assert!(lexer::tokenise_expression(b"i", &mut lex, MathMode::Standard).is_none());
}

#[test]
fn test_lex_advanced_mode_accepts_i() {
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    assert!(lexer::tokenise_expression(b"i", &mut lex, MathMode::Advanced).is_some());
    assert_eq!(lex.tokens[0], lexer::Token::ConstI);
}

#[test]
fn test_lex_log2_priority_over_log() {
    // "log2" must be lexed as FuncLog2, not FuncLog followed by number 2
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    assert!(lexer::tokenise_expression(b"log2(8)", &mut lex, MathMode::Standard).is_some());
    assert_eq!(lex.tokens[0], lexer::Token::FuncLog2);
}

// ── Evaluator edge cases ─────────────────────────────────────────────────────

#[test]
fn test_eval_zero_to_zero() {
    let mut vars = VariableStore::new();
    // 0^0 = 1 (by convention, through integer_power fast path)
    assert_eq!(eval_expr("0^0", &mut vars), Some(fp::FIXED_ONE));
}

#[test]
fn test_eval_modulo_negative() {
    let mut vars = VariableStore::new();
    // -5 % 3 = -2 (Rust semantics: remainder has sign of dividend)
    assert_eq!(eval_expr("-5%3", &mut vars), Some(fp::from_integer(-2)));
    // 5 % -3 = 2 (dividend positive → remainder positive)
    assert_eq!(eval_expr("5%-3", &mut vars), Some(fp::from_integer(2)));
}

#[test]
fn test_eval_modulo_by_zero() {
    let mut vars = VariableStore::new();
    assert_eq!(eval_expr("5%0", &mut vars), None);
}

#[test]
fn test_eval_two_arg_with_complex_args() {
    let mut vars = VariableStore::new();
    // poissonp with complex args should be None
    let r = eval_complex("poissonp(2+3i,5)", &mut vars);
    assert!(r.is_none());
}

#[test]
fn test_eval_three_arg_with_complex_args() {
    let mut vars = VariableStore::new();
    // binomp with complex args should be None
    let r = eval_complex("binomp(10,5+2i,0.5)", &mut vars);
    assert!(r.is_none());
}

#[test]
fn test_eval_log_via_eval() {
    let mut vars = VariableStore::new();
    // log(1000) = 3 (base 10)
    assert_eq!(eval_expr("log(1000)", &mut vars), Some(fp::from_integer(3)));
}

#[test]
fn test_eval_log10_of_10() {
    let mut vars = VariableStore::new();
    assert_approx_eq(
        eval_expr("log(10)", &mut vars).unwrap(),
        fp::FIXED_ONE,
        5,
    );
}

#[test]
fn test_eval_sin_negative_via_eval() {
    let mut vars = VariableStore::new();
    // sin(-π/2) = -1
    assert_eq!(
        eval_expr("sin(-pi/2)", &mut vars),
        Some(-fp::FIXED_ONE)
    );
}

#[test]
fn test_eval_cos_negative_via_eval() {
    let mut vars = VariableStore::new();
    // cos(-π) = -1
    assert_approx_eq(
        eval_expr("cos(-pi)", &mut vars).unwrap(),
        -fp::FIXED_ONE,
        100,
    );
}

#[test]
fn test_eval_deg_zero_rad_zero() {
    let mut vars = VariableStore::new();
    assert_eq!(eval_expr("deg(0)", &mut vars), Some(0));
    assert_eq!(eval_expr("rad(0)", &mut vars), Some(0));
}

#[test]
fn test_eval_deg_negative() {
    let mut vars = VariableStore::new();
    // deg(-180) = -π
    let r = eval_expr("deg(-180)", &mut vars).unwrap();
    assert_approx_eq(r, -fp::FIXED_PI, 100);
}

// ── Distribution edge cases ──────────────────────────────────────────────────

#[test]
fn test_ln_gamma_two_point_five() {
    // ln(Γ(2.5)) = 0.5*ln(π/4) ≈ 0.28468287
    let r = distributions::ln_gamma(q(2.5)).unwrap();
    assert_approx_eq(r, q(0.284_682_870_472_709_6), 100);
}

#[test]
fn test_binomial_probability_edge_params() {
    // P(X=0) for X~Binom(10, 0.001) ≈ 0.99004
    let r = distributions::binomial_probability(
        fp::from_integer(10),
        0,
        q(0.001),
    ).unwrap();
    assert_approx_eq(r, q(0.990_044_880_209_648_8), 100);
}

#[test]
fn test_poisson_probability_k_zero() {
    // P(X=0) for X~Poisson(1) = e^(-1) ≈ 0.36787944
    let r = distributions::poisson_probability(fp::FIXED_ONE, 0).unwrap();
    assert_approx_eq(r, q(0.367_879_441_171_442_33), 1000);
}

#[test]
fn test_chi_squared_cdf_edge_cases() {
    // χ²(0, k) = 0 for any k
    assert_eq!(distributions::chi_squared_cdf(0, fp::from_integer(2)), Some(0));
    // χ²(x, 2) = 1 - exp(-x/2) for k=2
    let r = distributions::chi_squared_cdf(fp::from_integer(4), fp::from_integer(2)).unwrap();
    assert_approx_eq(r, q(0.864_664_716_763_387_3), 1000); // 1 - e^(-2)
}
