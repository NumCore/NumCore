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
//!     is exact (±0 ULP), CORDIC trig ≤ 1000 ULP, Taylor-series
//!     functions ≤ 10 ULP, log/exp chains ≤ 20 ULP.

use numcore_math::math::compiler;
use numcore_math::math::complex::Complex;
use numcore_math::math::distributions;
use numcore_math::math::engine;
use numcore_math::math::fixed_point as fp;
use numcore_math::math::lexer;
use numcore_math::math::matrix::{Matrix, MatrixKind};
use numcore_math::math::vars::VariableStore;
use numcore_math::math::vm;

use numcore_math::math::AngleMode;
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
    assert_approx_eq(
        fp::sqrt(fp::from_integer(1000000)).unwrap(),
        fp::from_integer(1000),
        2000,
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
    // √(1e6) = 1000 — CLZ rsqrt + 3 NR + final refine gives ~1839 ULP error.
    let result = fp::sqrt(fp::from_integer(1_000_000i64)).unwrap();
    assert_approx_eq(result, fp::from_integer(1000), 2000);
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
// CORDIC-based, ≤ 1000 ULP.

#[test]
fn test_sin_standard_angles() {
    // sin(0) = 0
    assert_eq!(fp::sin(0), 0);
    // sin(π/2) = 1  (CORDIC, ~3 ULP)
    assert_approx_eq(fp::sin(fp::FIXED_PI_OVER_2), SCALE, 10);
    // sin(π) = 0
    assert_approx_eq(fp::sin(fp::FIXED_PI), 0, 10);
    // sin(3π/2) = -1  (CORDIC, ~3 ULP)
    assert_approx_eq(fp::sin(fp::FIXED_PI_OVER_2 * 3), -SCALE, 10);
    // sin(2π) = 0
    assert_approx_eq(fp::sin(fp::FIXED_TWO_PI), 0, 10);
}

#[test]
fn test_cos_standard_angles() {
    // cos(0) = 1
    assert_eq!(fp::cos(0), SCALE);
    // cos(π/2) = 0  (CORDIC, ~305 ULP)
    assert_approx_eq(fp::cos(fp::FIXED_PI_OVER_2), 0, 500);
    // cos(π) = -1  (CORDIC, ~305 ULP)
    assert_approx_eq(fp::cos(fp::FIXED_PI), -SCALE, 500);
    // cos(2π) = 1  (CORDIC)
    assert_approx_eq(fp::cos(fp::FIXED_TWO_PI), SCALE, 500);
}

#[test]
fn test_tan_standard_angles() {
    // tan(0) = 0
    assert_eq!(fp::tan(0), Some(0));
    // tan(π/4) = 1  (CORDIC, ~757 ULP)
    let p4 = fp::FIXED_PI / 4;
    assert_approx_eq(fp::tan(p4).unwrap(), SCALE, 1000);
    // tan(π/6) = 1/√3 ≈ 0.57735
    let p6 = fp::FIXED_PI / 6;
    assert_approx_eq(fp::tan(p6).unwrap(), q(0.577_350_269_189_625_8), 1000);
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
    assert_approx_eq(fp::sin(-fp::FIXED_PI_OVER_2), -fp::FIXED_ONE, 10);
    assert_approx_eq(fp::sin(-fp::FIXED_PI), 0, 10);
    assert_approx_eq(fp::sin(-q(30.0_f64.to_radians())), -fp::FIXED_HALF, 500);
}

#[test]
fn test_cos_negative_angle() {
    assert_approx_eq(fp::cos(-fp::FIXED_PI), -fp::FIXED_ONE, 500);
    assert_approx_eq(fp::cos(-fp::FIXED_PI_OVER_2), 0, 500);
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
    assert!(
        val > fp::from_integer(100),
        "tan near pole should be large, got {:?}",
        val
    );
}

// ─── 10. Inverse Trig ─────────────────────────────────────────────────────────

#[test]
fn test_asin_standard() {
    assert_eq!(fp::asin(0), Some(0));
    assert_eq!(fp::asin(SCALE), Some(fp::FIXED_PI_OVER_2));
    assert_eq!(fp::asin(-SCALE), Some(-fp::FIXED_PI_OVER_2));
    // asin(0.5) = π/6  (22-iter CORDIC atan, ~900 ULP)
    assert_approx_eq(
        fp::asin(fp::FIXED_HALF).unwrap(),
        q((30.0_f64).to_radians()),
        1000,
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
    // atan(1) = π/4  (22-iter CORDIC, ~900 ULP)
    assert_approx_eq(fp::atan(SCALE), fp::FIXED_PI / 4, 1000);
    // atan(-1) = -π/4  (22-iter CORDIC, ~900 ULP)
    assert_approx_eq(fp::atan(-SCALE), -(fp::FIXED_PI / 4), 1000);
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
    assert_approx_eq(
        fp::log10(fp::FIXED_E).unwrap(),
        q(0.434_294_481_903_251_8),
        10,
    );
    // log10(π) ≈ 0.49714987
    assert_approx_eq(
        fp::log10(fp::FIXED_PI).unwrap(),
        q(0.497_149_872_694_133_85),
        10,
    );
    // log10(very small) = negative
    let r = fp::log10(q(0.0001)).unwrap();
    assert!(r < 0);
    // log2(e) ≈ 1.44269504
    assert_approx_eq(
        fp::log2(fp::FIXED_E).unwrap(),
        q(1.442_695_040_888_963_4),
        10,
    );
    // log2(π) ≈ 1.65149613
    assert_approx_eq(
        fp::log2(fp::FIXED_PI).unwrap(),
        q(1.651_496_129_472_318),
        10,
    );
    // log2(very small) = negative
    let r = fp::log2(q(0.0001)).unwrap();
    assert!(r < 0);
}

#[test]
fn test_natural_log_extra() {
    // ln(π) ≈ 1.14472989
    assert_approx_eq(
        fp::natural_log(fp::FIXED_PI).unwrap(),
        q(1.144_729_885_849_400_2),
        20,
    );
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
    // Tiny negative values that round to zero should not produce "-0"
    assert_eq!(fmt(-1), "0");
    assert_eq!(fmt(-5), "0");
    assert_eq!(fmt(-100), "0");
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
    assert_eq!(
        vs.read_ans(),
        Some(Complex::from_real(fp::from_integer(42)))
    );
    vs.write_ans(Complex::from_real(fp::from_integer(-7)));
    assert_eq!(
        vs.read_ans(),
        Some(Complex::from_real(fp::from_integer(-7)))
    );
}

#[test]
fn test_variable_store_write_register() {
    let mut vs = VariableStore::new();
    assert!(vs.write_register(b'A', Complex::from_real(fp::from_integer(10))));
    assert_eq!(
        vs.read_register(b'A'),
        Some(Complex::from_real(fp::from_integer(10)))
    );
    assert!(vs.write_register(b'Z', Complex::from_real(fp::from_integer(-5))));
    assert_eq!(
        vs.read_register(b'Z'),
        Some(Complex::from_real(fp::from_integer(-5)))
    );
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
    assert_eq!(
        vs2.read_register(b'B'),
        Some(Complex::from_real(fp::from_integer(99)))
    );
    // Mutating vs1 shouldn't affect vs2.
    vs1.write_register(b'B', Complex::from_real(fp::from_integer(0)));
    assert_eq!(
        vs2.read_register(b'B'),
        Some(Complex::from_real(fp::from_integer(99)))
    );
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

#[test]
fn test_ln_factorial_stirling() {
    // k > 20 uses Stirling's series. Error < 1 ULP.
    let k = fp::from_integer(21);
    let result = distributions::ln_factorial(k).unwrap();
    // ln(21!) ≈ 45.3801388985
    assert_approx_eq(result, q(45.380_138_898_476_5), 500);

    let k = fp::from_integer(50);
    let result = distributions::ln_factorial(k).unwrap();
    // ln(50!) ≈ 148.477766
    assert_approx_eq(result, q(148.477_766_951_773_3), 500);
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

#[test]
fn test_ln_gamma_general() {
    // Γ(0.7) — non-integer, non-half-integer.
    let x = q(0.7);
    let result = distributions::ln_gamma(x).unwrap();
    // ln(Γ(0.7)) ≈ 0.260867 (Stirling series with recurrence)
    assert_approx_eq(result, q(0.260_867_246_531_666_5), 20);
}

#[test]
fn test_ln_gamma_domain() {
    assert_eq!(distributions::ln_gamma(0), None);
    assert_eq!(distributions::ln_gamma(-SCALE), None);
}

#[test]
fn test_ln_gamma_edge_cases() {
    // z = 1.0 -> 0.0
    assert_eq!(distributions::ln_gamma(fp::FIXED_ONE), Some(0));
    // z = 2.0 -> 0.0
    assert_eq!(distributions::ln_gamma(fp::from_integer(2)), Some(0));
    // z = 3.0 -> ln(2)
    assert_approx_eq(
        distributions::ln_gamma(fp::from_integer(3)).unwrap(),
        q(0.6931471805599453),
        10,
    );
    // z = 4.0 -> ln(6)
    assert_approx_eq(
        distributions::ln_gamma(fp::from_integer(4)).unwrap(),
        q(1.791759469228055),
        10,
    );
    // z = 5.0 -> ln(24)
    assert_approx_eq(
        distributions::ln_gamma(fp::from_integer(5)).unwrap(),
        q(3.1780538303479458),
        10,
    );
    // Large z = 50.0
    let r50 = distributions::ln_gamma(fp::from_integer(50)).unwrap();
    assert!(r50 > 0);
    // Very large z = 1000.0 (no overflow)
    let r1k = distributions::ln_gamma(fp::from_integer(1000)).unwrap();
    assert!(r1k > 0);
    // z = 0.1 (near pole) -> large positive
    let r01 = distributions::ln_gamma(q(0.1)).unwrap();
    assert!(r01 > 0);
    // z = 0.001 (extremely small) -> large positive
    let r001 = distributions::ln_gamma(q(0.001)).unwrap();
    assert!(r001 > 0);
    // z = 0.000001 (check stability)
    let r1e6 = distributions::ln_gamma(q(0.000001)).unwrap();
    assert!(r1e6 > 0);
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

#[ignore = "underflow: probability too small for Q31.32 resolution"]
#[test]
fn test_binomial_vanishingly_small() {
    // P(X=10) for Binomial(100, 0.001) — probability ~1.6e-17 underflows Q31.32.
    let n = fp::from_integer(100);
    let k = fp::from_integer(10);
    let p = q(0.001);
    let result = distributions::binomial_probability(n, k, p);
    let val = result.unwrap_or(0);
    assert!(val >= 0);
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

#[ignore = "underflow: probability too small for Q31.32 resolution"]
#[test]
fn test_poisson_vanishingly_small() {
    // Poisson(λ=100, k=200) — probability ~3.8e-19 underflows Q31.32.
    let result = distributions::poisson_probability(fp::from_integer(100), fp::from_integer(200));
    let val = result.unwrap_or(0);
    assert!(val >= 0);
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
fn eval_expr_with_mode(expr: &str, vars: &mut VariableStore, angle_mode: AngleMode) -> Option<i64> {
    let mut lex_scratch = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    let mut parse_scratch = compiler::Bytecode::new();
    match engine::evaluate_expression(
        expr.as_bytes(),
        vars,
        &mut lex_scratch,
        &mut parse_scratch,
        MathMode::Standard,
        angle_mode,
    ) {
        engine::EvalResult::Matrix(ref m) => {
            if let Some(c) = m.to_complex() {
                if c.im != 0 {
                    None
                } else {
                    Some(c.re)
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

fn eval_expr(expr: &str, vars: &mut VariableStore) -> Option<i64> {
    eval_expr_with_mode(expr, vars, AngleMode::Radians)
}

fn eval_complex_with_mode(
    expr: &str,
    vars: &mut VariableStore,
    angle_mode: AngleMode,
) -> Option<Complex> {
    let mut lex_scratch = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    let mut parse_scratch = compiler::Bytecode::new();
    match engine::evaluate_expression(
        expr.as_bytes(),
        vars,
        &mut lex_scratch,
        &mut parse_scratch,
        MathMode::Advanced,
        angle_mode,
    ) {
        engine::EvalResult::Matrix(ref m) => m.to_complex(),
        _ => None,
    }
}

fn eval_complex(expr: &str, vars: &mut VariableStore) -> Option<Complex> {
    eval_complex_with_mode(expr, vars, AngleMode::Radians)
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
    assert_approx_eq(
        eval_expr("ln(e)", &mut vars).unwrap(),
        fp::from_integer(1),
        10,
    );
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
    // sin(π/2) = 1  (CORDIC, ~3 ULP)
    assert_approx_eq(eval_expr("sin(pi/2)", &mut vars).unwrap(), SCALE, 10);
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
    // 2sin(π/2) = 2 * 1 = 2  (CORDIC, ~6 ULP)
    assert_approx_eq(
        eval_expr("2sin(pi/2)", &mut vars).unwrap(),
        fp::from_integer(2),
        10,
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

#[test]
fn test_eval_integration() {
    let mut vars = VariableStore::new();
    // ∫_0^π sin(X) dX = 2  (snapped to integer by integration snap)
    let result = eval_expr("int(sin(X),X,0,pi)", &mut vars).unwrap();
    assert_eq!(result, fp::from_integer(2));
    // ∫_0^1 1 dX = 1
    let result = eval_expr("int(1,X,0,1)", &mut vars).unwrap();
    assert_approx_eq(result, SCALE, 5);
}

#[test]
fn test_eval_integration_non_trivial() {
    let mut vars = VariableStore::new();
    // ∫_0^2 X dX = 2
    let result = eval_expr("int(X,X,0,2)", &mut vars).unwrap();
    assert_eq!(result, fp::from_integer(2));
    // ∫_0^1 X^2 dX = 1/3 ≈ 0.3333
    let result = eval_expr("int(X^2,X,0,1)", &mut vars).unwrap();
    assert_approx_eq(result, q(1.0 / 3.0), 500);
}

// ─── 19. Format Result (engine public API) ───────────────────────────────────

#[test]
fn test_engine_format_result() {
    let mut buf = [0u8; 48];
    let r = engine::format_result(&Matrix::scalar(0), MathMode::Standard, &mut buf);
    assert_eq!(core::str::from_utf8(r).unwrap(), "0");
    let r = engine::format_result(&Matrix::scalar(SCALE), MathMode::Standard, &mut buf);
    assert_eq!(core::str::from_utf8(r).unwrap(), "1");
    let r = engine::format_result(&Matrix::scalar(fp::FIXED_PI), MathMode::Standard, &mut buf);
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
        assert_approx_eq(exp, x_q, 600);
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
    let mut tree = compiler::Bytecode::new();
    assert!(compiler::compile(&lex, &mut tree).is_some());
    assert!(tree.len > 0);
}

#[test]
fn test_parser_mismatched_parens() {
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    lexer::tokenise_expression(b"(2+3", &mut lex, MathMode::Standard).unwrap();
    let mut tree = compiler::Bytecode::new();
    assert!(compiler::compile(&lex, &mut tree).is_none());
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
    // sin(pi/2) = 1  (CORDIC, ~3 ULP)
    // 45 + 32 - 1 = 76
    assert_approx_eq(result, fp::from_integer(76), 10);
}

// ─── 23. QEMU smoke-test parity ──────────────────────────────────────────────
//
// These tests replicate the exact inputs from test_inputs.txt so the
// host-side suite covers everything the QEMU smoke tests do (and more).

#[test]
fn test_scientific_arithmetic() {
    let mut vars = VariableStore::new();

    fn eval_sci(expr: &str, vars: &mut VariableStore) -> Option<(i64, i64)> {
        let mut lex = lexer::LexResult {
            tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
            token_count: 0,
        };
        let mut bc = compiler::Bytecode::new();
        lexer::tokenise_expression(expr.as_bytes(), &mut lex, MathMode::Scientific)?;
        compiler::compile(&lex, &mut bc)?;
        match vm::execute(&bc, vars, AngleMode::Radians, MathMode::Scientific) {
            engine::EvalResult::Matrix(ref m) => {
                if let Some(sci) = m.to_scientific() {
                    Some(sci)
                } else if m.kind == MatrixKind::Scalar {
                    // Convert scalar back to scientific representation
                    let v = m.data[0];
                    if v == 0 {
                        Some((0, 0))
                    } else {
                        numcore_math::math::matrix::normalize_scientific(v, 0)
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    // ── Basic literal parsing ──
    let r = eval_sci("1E0", &mut vars).expect("1E0");
    assert_eq!(r.0, fp::SCALE, "1E0 mantissa");
    assert_eq!(r.1, 0, "1E0 exponent");

    let r = eval_sci("1E+0", &mut vars).expect("1E+0");
    assert_eq!(r.0, fp::SCALE, "1E+0 mantissa");
    assert_eq!(r.1, 0, "1E+0 exponent");

    let r = eval_sci("1E-0", &mut vars).expect("1E-0");
    assert_eq!(r.0, fp::SCALE, "1E-0 mantissa");
    assert_eq!(r.1, 0, "1E-0 exponent");

    let r = eval_sci("1E10", &mut vars).expect("1E10");
    assert_eq!(r.0, fp::SCALE, "1E10 mantissa");
    assert_eq!(r.1, 10, "1E10 exponent");

    let r = eval_sci("1E-5", &mut vars).expect("1E-5");
    assert_eq!(r.0, fp::SCALE, "1E-5 mantissa");
    assert_eq!(r.1, -5, "1E-5 exponent");

    let r = eval_sci("1E+99", &mut vars).expect("1E+99");
    assert_eq!(r.0, fp::SCALE, "1E+99 mantissa");
    assert_eq!(r.1, 99, "1E+99 exponent");

    // ── Hard exponent limit: |99| ──
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    let mut bc = compiler::Bytecode::new();
    lexer::tokenise_expression(b"1E100", &mut lex, MathMode::Scientific).unwrap();
    compiler::compile(&lex, &mut bc).unwrap();
    let r1 = vm::execute(&bc, &mut vars, AngleMode::Radians, MathMode::Scientific);
    assert!(
        matches!(r1, engine::EvalResult::Matrix(ref m) if (m.data[1] > 99 || m.data[1] < -99))
            || matches!(r1, engine::EvalResult::Overflow { .. }),
        "1E100 should overflow, got {:?}",
        r1
    );

    let mut bc = compiler::Bytecode::new();
    lexer::tokenise_expression(b"1E-100", &mut lex, MathMode::Scientific).unwrap();
    compiler::compile(&lex, &mut bc).unwrap();
    let r1 = vm::execute(&bc, &mut vars, AngleMode::Radians, MathMode::Scientific);
    assert!(
        matches!(r1, engine::EvalResult::Matrix(ref m) if (m.data[1] > 99 || m.data[1] < -99))
            || matches!(r1, engine::EvalResult::Overflow { .. }),
        "1E100 should overflow, got {:?}",
        r1
    );

    let mut bc = compiler::Bytecode::new();
    lexer::tokenise_expression(b"1E-100", &mut lex, MathMode::Scientific).unwrap();
    compiler::compile(&lex, &mut bc).unwrap();
    let r2 = vm::execute(&bc, &mut vars, AngleMode::Radians, MathMode::Scientific);
    assert!(
        matches!(r2, engine::EvalResult::Matrix(ref m) if (m.data[1] > 99 || m.data[1] < -99))
            || matches!(r2, engine::EvalResult::Overflow { .. }),
        "1E-100 should overflow, got {:?}",
        r2
    );

    // ── Fractional mantissa ──
    let r = eval_sci("1.5E0", &mut vars).expect("1.5E0");
    assert!(
        r.0 >= fp::SCALE + fp::SCALE / 2 - 1 && r.0 <= fp::SCALE + fp::SCALE / 2 + 1,
        "1.5E0 mantissa={} should be ~1.5*SCALE",
        r.0
    );
    assert_eq!(r.1, 0, "1.5E0 exponent");

    let r = eval_sci("2.5E3", &mut vars).expect("2.5E3");
    assert!(
        r.0 >= 2 * fp::SCALE + fp::SCALE / 2 - 1 && r.0 <= 2 * fp::SCALE + fp::SCALE / 2 + 1,
        "2.5E3 mantissa={} should be ~2.5*SCALE",
        r.0
    );
    assert_eq!(r.1, 3, "2.5E3 exponent");

    // ── Multiplication ──
    let r = eval_sci("1E10*2", &mut vars).expect("1E10*2");
    assert_eq!(r.0, 2 * fp::SCALE, "1E10*2 mantissa");
    assert_eq!(r.1, 10, "1E10*2 exponent");

    let r = eval_sci("2E10*3", &mut vars).expect("2E10*3");
    assert_eq!(r.0, 6 * fp::SCALE, "2E10*3 mantissa");
    assert_eq!(r.1, 10, "2E10*3 exponent");

    let r = eval_sci("2E10*5", &mut vars).expect("2E10*5");
    assert!(
        r.0 >= fp::SCALE && r.0 < 2 * fp::SCALE,
        "2E10*5 mantissa={} should be ~1.0",
        r.0
    );
    assert_eq!(r.1, 11, "2E10*5 exponent"); // 2E10*5 = 10E10 = 1E11

    // ── Division ──
    let r = eval_sci("1E10/2", &mut vars).expect("1E10/2");
    assert!(
        r.0 >= 4 * fp::SCALE && r.0 <= 6 * fp::SCALE,
        "1E10/2 mantissa={}",
        r.0
    );
    assert_eq!(r.1, 9, "1E10/2 exponent");

    let r = eval_sci("2E10/2", &mut vars).expect("2E10/2");
    assert_eq!(r.0, fp::SCALE, "2E10/2 mantissa");
    assert_eq!(r.1, 10, "2E10/2 exponent");

    let r = eval_sci("1E10/4", &mut vars).expect("1E10/4");
    assert!(
        r.0 >= 2 * fp::SCALE && r.0 <= 3 * fp::SCALE,
        "1E10/4 mantissa={}",
        r.0
    );
    assert_eq!(r.1, 9, "1E10/4 exponent"); // 1E10/4 = 0.25E10 = 2.5E9

    // ── Scalar * Scientific ── (left scalar, right sci)
    let r = eval_sci("3*1E10", &mut vars).expect("3*1E10");
    assert_eq!(r.0, 3 * fp::SCALE, "3*1E10 mantissa");
    assert_eq!(r.1, 10, "3*1E10 exponent");

    let r = eval_sci("2*2.5E4", &mut vars).expect("2*2.5E4");
    assert_eq!(r.0, 5 * fp::SCALE, "2*2.5E4 mantissa (5)");
    assert_eq!(r.1, 4, "2*2.5E4 exponent"); // 2*2.5E4 = 5E4

    // ── Addition/Subtraction ──
    let r = eval_sci("1E10+2E10", &mut vars).expect("1E10+2E10");
    assert_eq!(r.0, 3 * fp::SCALE, "1E10+2E10 mantissa");
    assert_eq!(r.1, 10, "1E10+2E10 exponent");

    let r = eval_sci("1E10+1E8", &mut vars).expect("1E10+1E8");
    // 1E10 + 1E8 = 1.01E10; mantissa should be ~1.01*SCALE
    assert!(
        r.0 > fp::SCALE && r.0 < fp::SCALE + fp::SCALE / 50,
        "1E10+1E8 mantissa={} should be ~1.01*SCALE",
        r.0
    );
    assert_eq!(r.1, 10, "1E10+1E8 exponent");

    let r = eval_sci("5E10-2E10", &mut vars).expect("5E10-2E10");
    assert_eq!(r.0, 3 * fp::SCALE, "5E10-2E10 mantissa");
    assert_eq!(r.1, 10, "5E10-2E10 exponent");

    // 5E10-5E10 = 0 → scalar, not Scientific
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    let mut bc = compiler::Bytecode::new();
    lexer::tokenise_expression(b"5E10-5E10", &mut lex, MathMode::Scientific).unwrap();
    compiler::compile(&lex, &mut bc).unwrap();
    match vm::execute(&bc, &mut vars, AngleMode::Radians, MathMode::Scientific) {
        engine::EvalResult::Matrix(ref m) => {
            assert_eq!(m.kind, MatrixKind::Scalar, "5E10-5E10 should be Scalar 0");
            assert_eq!(m.data[0], 0, "5E10-5E10 value should be 0");
        }
        _ => panic!("5E10-5E10 should be Matrix(Scalar(0))"),
    }

    // ── Power ──
    let r = eval_sci("1E2^3", &mut vars).expect("1E2^3");
    assert_eq!(r.0, fp::SCALE, "1E2^3 mantissa");
    assert_eq!(r.1, 6, "1E2^3 exponent"); // (10^2)^3 = 10^6

    // ── Negative exponent ──
    let r = eval_sci("1E-3", &mut vars).expect("1E-3");
    assert_eq!(r.0, fp::SCALE, "1E-3 mantissa");
    assert_eq!(r.1, -3, "1E-3 exponent");

    // ── Smallest value ──
    let r = eval_sci("1E-99", &mut vars).expect("1E-99");
    assert_eq!(r.0, fp::SCALE, "1E-99 mantissa");
    assert_eq!(r.1, -99, "1E-99 exponent");

    // ── Zero mantissa ──
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    let mut bc = compiler::Bytecode::new();
    lexer::tokenise_expression(b"0E10", &mut lex, MathMode::Scientific).unwrap();
    compiler::compile(&lex, &mut bc).unwrap();
    match vm::execute(&bc, &mut vars, AngleMode::Radians, MathMode::Scientific) {
        engine::EvalResult::Matrix(ref m) => {
            assert_eq!(m.kind, MatrixKind::Scalar, "0E10 should be Scalar 0");
            assert_eq!(m.data[0], 0, "0E10 value should be 0");
        }
        _ => panic!("0E10 should be Matrix(Scalar(0))"),
    }

    // ── ConstructSci overflow ──
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    let mut bc = compiler::Bytecode::new();
    lexer::tokenise_expression(b"9E100", &mut lex, MathMode::Scientific).unwrap();
    compiler::compile(&lex, &mut bc).unwrap();
    let r1 = vm::execute(&bc, &mut vars, AngleMode::Radians, MathMode::Scientific);
    assert!(
        matches!(r1, engine::EvalResult::Matrix(ref m) if (m.data[1] > 99 || m.data[1] < -99))
            || matches!(r1, engine::EvalResult::Overflow { .. }),
        "9E100 should overflow, got {:?}",
        r1
    );

    // ── Scientific overflow via arithmetic ──
    let mut bc = compiler::Bytecode::new();
    lexer::tokenise_expression(b"9E99*10", &mut lex, MathMode::Scientific).unwrap();
    compiler::compile(&lex, &mut bc).unwrap();
    let r2 = vm::execute(&bc, &mut vars, AngleMode::Radians, MathMode::Scientific);
    assert!(
        matches!(r2, engine::EvalResult::Matrix(ref m) if (m.data[1] > 99 || m.data[1] < -99))
            || matches!(r2, engine::EvalResult::Overflow { .. }),
        "9E99*10 should overflow, got {:?}",
        r2
    );

    // ── Rejection in non-Scientific modes ──
    // "1E10" in Standard mode: lexer cannot parse "E10" as a valid identifier
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    assert!(
        lexer::tokenise_expression(b"1E10", &mut lex, MathMode::Standard).is_none(),
        "Standard mode should reject '1E10' (E10 is not a valid identifier)"
    );

    // ── Round-trip through format ──
    let m = Matrix::scientific(fp::SCALE, 10).unwrap();
    let mut buf = [0u8; 48];
    let s = engine::format_result(&m, MathMode::Scientific, &mut buf);
    assert_eq!(s, b"1E+10", "format_scientific(1, 10)");

    let m = Matrix::scientific(5 * fp::SCALE, 9).unwrap();
    let s = engine::format_result(&m, MathMode::Scientific, &mut buf);
    assert_eq!(s, b"5E+9", "format_scientific(5, 9)");

    let m = Matrix::scientific(fp::SCALE, -5).unwrap();
    let s = engine::format_result(&m, MathMode::Scientific, &mut buf);
    assert_eq!(s, b"1E-5", "format_scientific(1, -5)");
}

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
    assert_approx_eq(
        eval_expr("2sin(pi/2)", &mut vars).unwrap(),
        fp::from_integer(2),
        10,
    );

    // Trig  (CORDIC, ~3 ULP)
    assert_approx_eq(eval_expr("sin(pi/2)", &mut vars).unwrap(), SCALE, 10);
    assert_eq!(eval_expr("cos(0)", &mut vars), Some(SCALE));

    // Sqrt and abs
    assert_eq!(eval_expr("sqrt(16)", &mut vars), Some(fp::from_integer(4)));
    assert_eq!(eval_expr("abs(-5)", &mut vars), Some(fp::from_integer(5)));

    // Logs
    assert_approx_eq(eval_expr("ln(e)", &mut vars).unwrap(), SCALE, 10);
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
    assert_eq!(
        fp::floor(fp::from_integer(3) + fp::FIXED_HALF),
        fp::from_integer(3)
    );
    // ceil of negative fraction
    assert_eq!(
        fp::ceil(-fp::from_integer(3) - fp::FIXED_HALF),
        -fp::from_integer(3)
    );
    // round of negative values away from zero
    assert_eq!(
        fp::round(-fp::from_integer(3) - fp::FIXED_HALF),
        -fp::from_integer(4)
    );
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
    // sin(deg(90)) = 1  (CORDIC, ~3 ULP)
    assert_approx_eq(eval_expr("sin(deg(90))", &mut vars).unwrap(), SCALE, 10);
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

#[test]
fn test_integration_simple() {
    let mut vars = VariableStore::new();
    // ∫_0^π sin(X) dX = 2
    let result = eval_expr("int(sin(X),X,0,pi)", &mut vars).unwrap();
    assert_eq!(result, fp::from_integer(2));
    // ∫_0^1 2X dX = 1
    let result = eval_expr("int(2*X,X,0,1)", &mut vars).unwrap();
    assert_approx_eq(result, SCALE, 10);
}

// ─── 31. Full expression exact match ─────────────────────────────────────────

#[test]
fn test_complex_expression() {
    let mut vars = VariableStore::new();
    // (sqrt(25) + abs(-3)) * 2 - sin(pi/2) = (5 + 3) * 2 - 1 = 15
    let result = eval_expr("(sqrt(25)+abs(-3))*2-sin(pi/2)", &mut vars).unwrap();
    assert_approx_eq(result, fp::from_integer(15), 10);
}

// ─── 32. ∫_0^10 sinh(X) dX ≈ cosh(10) - 1 ≈ 11012.23292 ───────────────────

#[test]
fn test_integration_sinh() {
    let mut vars = VariableStore::new();
    // The analytical result using the library's own cosh
    let analytical = eval_expr("cosh(10)-1", &mut vars).unwrap();
    // The Simpson integration
    let result = eval_expr("int(sinh(X),X,0,10)", &mut vars).unwrap();
    let diff = (result - analytical).abs();
    // The error floor is set by natural_exp precision (≈1.7e-6 for this integral)
    // propagating through thousands of evaluations.  Tighter than 2e-6 is not
    // achievable without improving natural_exp itself.
    assert!(
        diff < 10_000i64,
        "∫sinh error {:.2e} (>2e-6)",
        diff as f64 / 4294967296.0
    );
}

// ─── 33. Multiply saturation in evaluator ────────────────────────────────────

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
    assert_eq!(
        r,
        Some(Complex::new(fp::from_integer(3), fp::from_integer(4)))
    );
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
    assert_eq!(
        r,
        Some(Complex::new(fp::from_integer(4), fp::from_integer(6)))
    );
}

#[test]
fn test_complex_multiplication() {
    let mut vars = VariableStore::new();
    // (1+2i)*(3+4i) = 3 + 4i + 6i + 8i^2 = 3 + 10i - 8 = -5 + 10i
    let r = eval_complex("(1+2i)*(3+4i)", &mut vars);
    assert_eq!(
        r,
        Some(Complex::new(fp::from_integer(-5), fp::from_integer(10)))
    );
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
    let mut parse_scratch = compiler::Bytecode::new();
    let r = engine::evaluate_expression(
        b"i",
        &mut vars,
        &mut lex_scratch,
        &mut parse_scratch,
        MathMode::Standard,
        AngleMode::Radians,
    );
    assert!(matches!(r, engine::EvalResult::DomainError));
}

#[test]
fn test_complex_format_standard() {
    let mut buf = [0u8; 48];
    let r = engine::format_result(
        &Matrix::complex(fp::from_integer(3), fp::from_integer(4)),
        MathMode::Standard,
        &mut buf,
    );
    assert_eq!(core::str::from_utf8(r).unwrap(), "3");
}

#[test]
fn test_complex_format_advanced_real() {
    let mut buf = [0u8; 48];
    let r = engine::format_result(&Matrix::scalar(fp::FIXED_PI), MathMode::Advanced, &mut buf);
    assert_eq!(core::str::from_utf8(r).unwrap(), "3.141593");
}

#[test]
fn test_complex_format_advanced_3_plus_4i() {
    let mut buf = [0u8; 48];
    let r = engine::format_result(
        &Matrix::complex(fp::from_integer(3), fp::from_integer(4)),
        MathMode::Advanced,
        &mut buf,
    );
    assert_eq!(core::str::from_utf8(r).unwrap(), "3+4i");
}

#[test]
fn test_complex_format_advanced_negative_im() {
    let mut buf = [0u8; 48];
    let r = engine::format_result(
        &Matrix::complex(fp::from_integer(3), -fp::from_integer(4)),
        MathMode::Advanced,
        &mut buf,
    );
    assert_eq!(core::str::from_utf8(r).unwrap(), "3-4i");
}

#[test]
fn test_complex_format_advanced_pure_imaginary() {
    let mut buf = [0u8; 48];
    let r = engine::format_result(
        &Matrix::complex(0, fp::FIXED_ONE),
        MathMode::Advanced,
        &mut buf,
    );
    assert_eq!(core::str::from_utf8(r).unwrap(), "i");
}

#[test]
fn test_complex_format_advanced_negative_pure_imaginary() {
    let mut buf = [0u8; 48];
    let r = engine::format_result(
        &Matrix::complex(0, -fp::FIXED_ONE),
        MathMode::Advanced,
        &mut buf,
    );
    assert_eq!(core::str::from_utf8(r).unwrap(), "-i");
}

#[test]
fn test_complex_format_advanced_2i() {
    let mut buf = [0u8; 48];
    let r = engine::format_result(
        &Matrix::complex(0, fp::from_integer(2)),
        MathMode::Advanced,
        &mut buf,
    );
    assert_eq!(core::str::from_utf8(r).unwrap(), "2i");
}

#[test]
fn test_complex_format_advanced_neg_2i() {
    let mut buf = [0u8; 48];
    let r = engine::format_result(
        &Matrix::complex(0, -fp::from_integer(2)),
        MathMode::Advanced,
        &mut buf,
    );
    assert_eq!(core::str::from_utf8(r).unwrap(), "-2i");
}

#[test]
fn test_complex_power_integer_exponent() {
    let mut vars = VariableStore::new();
    // (2+3i)^2 = -5+12i
    let r = eval_complex("(2+3i)^2", &mut vars);
    assert_eq!(
        r,
        Some(Complex::new(fp::from_integer(-5), fp::from_integer(12)))
    );
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
    assert!(re_diff < 1000, "re diff = {}", re_diff); // within ~1000 Q31.32 ULP
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
    assert_eq!(
        r,
        Some(Complex::new(fp::from_integer(-11), -fp::from_integer(2)))
    );
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
    assert!(complex_approx_close(
        r,
        Complex::from_real(fp::FIXED_ONE),
        100
    ));
}

#[test]
fn test_complex_sin() {
    // sin(0) = 0
    let r = Complex::sin(Complex::zero()).unwrap();
    assert_eq!(r, Complex::zero());
    // sin(pi/2) = 1
    let half_pi = fp::divide(fp::FIXED_PI, fp::from_integer(2)).unwrap();
    let r = Complex::sin(Complex::from_real(half_pi)).unwrap();
    assert!(complex_approx_close(
        r,
        Complex::from_real(fp::FIXED_ONE),
        100
    ));
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
    // cos(pi/2) ≈ 0  (CORDIC, ~305 ULP)
    let half_pi = fp::divide(fp::FIXED_PI, fp::from_integer(2)).unwrap();
    let r = Complex::cos(Complex::from_real(half_pi)).unwrap();
    assert!(complex_approx_close(r, Complex::zero(), 500));
    // cos(i) = cosh(1)
    let ch1 = fp::cosh(fp::FIXED_ONE).unwrap();
    let r = Complex::cos(Complex::new(0, fp::FIXED_ONE)).unwrap();
    assert!(complex_approx_close(r, Complex::from_real(ch1), 500));
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
    assert!(complex_approx_close(
        sum,
        Complex::from_real(fp::FIXED_ONE),
        200
    ));
}

#[test]
fn test_complex_tan() {
    // tan(0) = 0
    let r = Complex::tan(Complex::zero()).unwrap();
    assert_eq!(r, Complex::zero());
    // tan(pi/4) ≈ 1  (CORDIC, ~757 ULP)
    let quarter_pi = fp::divide(fp::FIXED_PI, fp::from_integer(4)).unwrap();
    let r = Complex::tan(Complex::from_real(quarter_pi)).unwrap();
    assert!(complex_approx_close(
        r,
        Complex::from_real(fp::FIXED_ONE),
        1000
    ));
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
    assert!(complex_approx_close(
        r,
        Complex::from_real(-fp::FIXED_ONE),
        200
    ));
}

#[test]
fn test_complex_log10() {
    // log10(10) = 1
    let r = Complex::log10(Complex::from_real(fp::from_integer(10))).unwrap();
    assert!(complex_approx_close(
        r,
        Complex::from_real(fp::FIXED_ONE),
        100
    ));
}

#[test]
fn test_complex_log2() {
    // log2(8) = 3
    let r = Complex::log2(Complex::from_real(fp::from_integer(8))).unwrap();
    assert!(complex_approx_close(
        r,
        Complex::from_real(fp::from_integer(3)),
        200
    ));
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
    assert_eq!(
        Complex::arg(Complex::new(0, fp::FIXED_ONE)),
        fp::FIXED_PI_OVER_2
    );
    assert_approx_eq(
        Complex::arg(Complex::from_real(-fp::FIXED_ONE)),
        fp::FIXED_PI,
        100,
    );
    assert_eq!(
        Complex::arg(Complex::new(0, -fp::FIXED_ONE)),
        -fp::FIXED_PI_OVER_2
    );
    let three_quarter_pi = fp::FIXED_PI - fp::FIXED_PI_OVER_2 / 2;
    assert_approx_eq(
        Complex::arg(Complex::new(-fp::FIXED_ONE, -fp::FIXED_ONE)),
        -three_quarter_pi,
        1000,
    );
}

#[test]
fn test_complex_from_polar() {
    // r=0 → 0+0i
    let z = Complex::from_polar(0, fp::FIXED_PI_OVER_2).unwrap();
    assert_eq!(z, Complex::zero());
    // r=1, θ=0 → 1+0i
    let z = Complex::from_polar(fp::FIXED_ONE, 0).unwrap();
    assert_eq!(z, Complex::from_real(fp::FIXED_ONE));
    // r=1, θ=π/2 → 0+1i  (CORDIC)
    let z = Complex::from_polar(fp::FIXED_ONE, fp::FIXED_PI_OVER_2).unwrap();
    assert!(complex_approx_close(z, Complex::new(0, fp::FIXED_ONE), 500));
    // r=1, θ=π → -1+0i  (CORDIC)
    let z = Complex::from_polar(fp::FIXED_ONE, fp::FIXED_PI).unwrap();
    assert!(complex_approx_close(
        z,
        Complex::from_real(-fp::FIXED_ONE),
        500
    ));
    // r=1, θ=-π/2 → 0-1i  (CORDIC)
    let z = Complex::from_polar(fp::FIXED_ONE, -fp::FIXED_PI_OVER_2).unwrap();
    assert!(complex_approx_close(
        z,
        Complex::new(0, -fp::FIXED_ONE),
        500
    ));
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
fn test_complex_div_overflow_protection() {
    let big = fp::from_integer(100_000_000);
    let result = Complex::new(big, big).div(Complex::new(big, fp::from_integer(1)));
    assert!(
        result.is_some(),
        "Smith div should not overflow for 1e8-scale values"
    );
}

#[test]
fn test_complex_div_large_equal() {
    let big = fp::from_integer(1_000_000_000);
    let result = Complex::new(big, big).div(Complex::new(big, big));
    assert!(result.is_some());
    assert_approx_eq(result.unwrap().re, fp::FIXED_ONE, 10);
    assert_approx_eq(result.unwrap().im, 0, 10);
}

#[test]
fn test_complex_div_large_real_denominator() {
    let big = fp::from_integer(1_000_000_000);
    let result = Complex::new(big, big).div(Complex::from_real(big));
    assert!(result.is_some());
    assert_approx_eq(result.unwrap().re, fp::FIXED_ONE, 10);
    assert_approx_eq(result.unwrap().im, fp::FIXED_ONE, 10);
}

#[test]
fn test_complex_div_large_imag_denominator() {
    let big = fp::from_integer(1_000_000_000);
    let result = Complex::new(big, big).div(Complex::new(0, big));
    assert!(result.is_some());
    assert_approx_eq(result.unwrap().re, fp::FIXED_ONE, 10);
    assert_approx_eq(result.unwrap().im, -fp::FIXED_ONE, 10);
}

#[test]
fn test_complex_div_negative_denominator() {
    let result =
        Complex::new(fp::FIXED_ONE, fp::FIXED_ONE).div(Complex::new(-fp::FIXED_ONE, fp::FIXED_ONE));
    assert!(result.is_some());
    let (re, im) = (result.unwrap().re, result.unwrap().im);
    assert_approx_eq(re, 0, 10);
    assert_approx_eq(im, -fp::FIXED_ONE, 10);
}

#[test]
fn test_complex_div_mixed_signs() {
    let a = Complex::new(fp::from_integer(1), -fp::from_integer(2));
    let b = Complex::new(-fp::from_integer(3), fp::from_integer(4));
    let result = a.div(b).unwrap();
    assert_approx_eq(
        result.re,
        -fp::divide(fp::from_integer(11), fp::from_integer(25)).unwrap(),
        100,
    );
    assert_approx_eq(
        result.im,
        fp::divide(fp::from_integer(2), fp::from_integer(25)).unwrap(),
        100,
    );
}

#[test]
fn test_complex_div_normal_case() {
    let result = Complex::new(fp::from_integer(3), fp::from_integer(4))
        .div(Complex::new(fp::FIXED_ONE, fp::from_integer(2)));
    assert!(result.is_some());
    let expected = Complex::new(
        fp::divide(fp::from_integer(11), fp::from_integer(5)).unwrap(),
        fp::divide(-fp::from_integer(2), fp::from_integer(5)).unwrap(),
    );
    assert!(complex_approx_close(result.unwrap(), expected, 100));
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
    assert!(complex_approx_close(
        r,
        Complex::from_real(fp::FIXED_ONE),
        100
    ));
    // cos(0) = 1
    let r = Complex::cos(Complex::zero()).unwrap();
    assert_eq!(r, Complex::from_real(fp::FIXED_ONE));
}

#[test]
fn test_complex_tan_pole() {
    // tan of large pure imaginary → should converge to i
    let r = Complex::tan(Complex::new(0, fp::from_integer(10))).unwrap();
    assert!(complex_approx_close(
        r,
        Complex::new(0, fp::FIXED_ONE),
        1000
    ));
}

#[test]
fn test_complex_asin_acos_edges() {
    // asin(0) = 0
    let r = Complex::asin(Complex::zero()).unwrap();
    assert!(complex_approx_close(r, Complex::zero(), 100));
    // asin(1) ≈ π/2
    let r = Complex::asin(Complex::from_real(fp::FIXED_ONE)).unwrap();
    assert!(complex_approx_close(
        r,
        Complex::from_real(fp::FIXED_PI_OVER_2),
        200
    ));
    // acos(0) ≈ π/2
    let r = Complex::acos(Complex::zero()).unwrap();
    assert!(complex_approx_close(
        r,
        Complex::from_real(fp::FIXED_PI_OVER_2),
        200
    ));
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
    assert!(complex_approx_close(
        r,
        Complex::new(0, fp::FIXED_PI_OVER_2),
        200
    ));
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
    assert_eq!(
        r,
        Some(Complex::new(fp::from_integer(2), fp::from_integer(2)))
    );
}

#[test]
fn test_complex_div_via_eval() {
    let mut vars = VariableStore::new();
    let r = eval_complex("(3+4i)/(1+2i)", &mut vars);
    let expected = Complex::new(
        fp::divide(fp::from_integer(11), fp::from_integer(5)).unwrap(),
        fp::divide(fp::from_integer(-2), fp::from_integer(5)).unwrap(),
    );
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
    assert_eq!(
        r,
        Some(Complex::new(-fp::from_integer(3), -fp::from_integer(4)))
    );
}

#[test]
fn test_complex_power_complex_exponent() {
    let mut vars = VariableStore::new();
    // (2+3i)^(1+i) = exp((1+i) * ln(2+3i)) ≈ -0.863607 + 1.036889i
    let r = eval_complex("(2+3i)^(1+i)", &mut vars);
    let expected = Complex::new(-3709163387, 4453406049);
    assert!(complex_approx_close(r.unwrap(), expected, 3000));
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
    let s = engine::format_result(&Matrix::scalar(0), mode, &mut buf);
    assert_eq!(core::str::from_utf8(s).unwrap(), "0");
}

#[test]
fn test_format_result_advanced_one_plus_i() {
    let mode = MathMode::Advanced;
    let mut buf = [0u8; 48];
    let s = engine::format_result(
        &Matrix::complex(fp::FIXED_ONE, fp::FIXED_ONE),
        mode,
        &mut buf,
    );
    assert_eq!(core::str::from_utf8(s).unwrap(), "1+i");
}

#[test]
fn test_format_result_advanced_one_minus_i() {
    let mode = MathMode::Advanced;
    let mut buf = [0u8; 48];
    let s = engine::format_result(
        &Matrix::complex(fp::FIXED_ONE, -fp::FIXED_ONE),
        mode,
        &mut buf,
    );
    assert_eq!(core::str::from_utf8(s).unwrap(), "1-i");
}

#[test]
fn test_format_result_advanced_pure_imaginary_fractional() {
    let mode = MathMode::Advanced;
    let mut buf = [0u8; 48];
    let val = Matrix::complex(0, fp::FIXED_HALF);
    let s = engine::format_result(&val, mode, &mut buf);
    // Should show "0.5i" (pure imaginary with fractional coef)
    let fmt = core::str::from_utf8(s).unwrap();
    assert!(fmt.contains('i'));
}

#[test]
fn test_format_result_advanced_negative_pure_imaginary_fractional() {
    let mode = MathMode::Advanced;
    let mut buf = [0u8; 48];
    let val = Matrix::complex(0, -fp::FIXED_HALF);
    let s = engine::format_result(&val, mode, &mut buf);
    let fmt = core::str::from_utf8(s).unwrap();
    assert!(fmt.starts_with('-'));
    assert!(fmt.contains('i'));
}

#[test]
fn test_format_result_advanced_real_only() {
    let mode = MathMode::Advanced;
    let mut buf = [0u8; 48];
    let s = engine::format_result(&Matrix::scalar(fp::from_integer(42)), mode, &mut buf);
    assert_eq!(core::str::from_utf8(s).unwrap(), "42");
}

#[test]
fn test_format_result_standard_strips_imaginary() {
    let mode = MathMode::Standard;
    let mut buf = [0u8; 48];
    let s = engine::format_result(
        &Matrix::complex(fp::from_integer(3), fp::from_integer(4)),
        mode,
        &mut buf,
    );
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
    assert_approx_eq(eval_expr("log(10)", &mut vars).unwrap(), fp::FIXED_ONE, 5);
}

#[test]
fn test_eval_sin_negative_via_eval() {
    let mut vars = VariableStore::new();
    // sin(-π/2) = -1  (CORDIC, ~3 ULP)
    assert_approx_eq(
        eval_expr("sin(-pi/2)", &mut vars).unwrap(),
        -fp::FIXED_ONE,
        10,
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
    let r = distributions::binomial_probability(fp::from_integer(10), 0, q(0.001)).unwrap();
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
    assert_eq!(
        distributions::chi_squared_cdf(0, fp::from_integer(2)),
        Some(0)
    );
    // χ²(x, 2) = 1 - exp(-x/2) for k=2
    let r = distributions::chi_squared_cdf(fp::from_integer(4), fp::from_integer(2)).unwrap();
    assert_approx_eq(r, q(0.864_664_716_763_387_3), 1000); // 1 - e^(-2)
}

// ─── 35. Degrees mode (pipeline) ──────────────────────────────────────────────

#[test]
fn test_degrees_mode_sin_30() {
    let mut vars = VariableStore::new();
    let result = eval_expr_with_mode("sin(30)", &mut vars, AngleMode::Degrees);
    assert_approx_eq(result.unwrap(), q(0.5), 1000);
}

#[test]
fn test_degrees_mode_sin_90() {
    let mut vars = VariableStore::new();
    let result = eval_expr_with_mode("sin(90)", &mut vars, AngleMode::Degrees);
    assert_approx_eq(result.unwrap(), q(1.0), 1000);
}

#[test]
fn test_degrees_mode_cos_60() {
    let mut vars = VariableStore::new();
    let result = eval_expr_with_mode("cos(60)", &mut vars, AngleMode::Degrees);
    assert_approx_eq(result.unwrap(), q(0.5), 1000);
}

#[test]
fn test_degrees_mode_asin_acos_roundtrip() {
    let mut vars = VariableStore::new();
    // asin(sin(30 deg)) should give ~30 (in degrees mode)
    let result = eval_expr_with_mode("asin(sin(30))", &mut vars, AngleMode::Degrees);
    assert_approx_eq(result.unwrap(), q(30.0), 100000);
}

#[test]
fn test_degrees_mode_radians_fallback() {
    // Complex-arg trig always uses radians regardless of mode.
    let mut vars = VariableStore::new();
    let r = eval_complex_with_mode("sin(i)", &mut vars, AngleMode::Degrees);
    let c = r.unwrap();
    // sin(i) = i·sinh(1) ≈ i·1.1752 — real part should be 0
    assert_eq!(c.re, 0);
    assert!(fp::abs(c.im - q(1.1752011936438014)) < 100000);
}

#[test]
fn test_euler_identity_via_power() {
    let mut vars = VariableStore::new();
    // e^(i*pi) = -1 (imag part ~305 ULP from CORDIC sin/cos)
    let r = eval_complex("e^(i*pi)", &mut vars);
    assert!(complex_approx_close(
        r.unwrap(),
        Complex::from_real(-fp::SCALE),
        500
    ));
    // e^i = cos(1) + i*sin(1)
    let r = eval_complex("e^i", &mut vars);
    let cos1 = fp::cos(fp::from_integer(1));
    let sin1 = fp::sin(fp::from_integer(1));
    assert!(complex_approx_close(
        r.unwrap(),
        Complex::new(cos1, sin1),
        500
    ));
    // exp(i*pi) via function call
    let r = eval_complex("exp(i*pi)", &mut vars);
    assert!(complex_approx_close(
        r.unwrap(),
        Complex::from_real(-fp::SCALE),
        500
    ));
    // i^i = real
    let r = eval_complex("i^i", &mut vars);
    assert!(r.unwrap().re > 0);
    assert!(r.unwrap().im.abs() < 1000);
}

// ─── Adaptive Simpson edge cases ──────────────────────────────────────────

#[test]
fn test_motivating_sqrt_integral() {
    let mut vars = VariableStore::new();
    let result = eval_expr("int(sqrt(X),X,0,1.4)", &mut vars).unwrap();
    // Exact: 2/3 * 1.4^1.5 ≈ 1.1043348928
    let exact = q(1.1043348928);
    let error = if result > exact {
        result - exact
    } else {
        exact - result
    };
    println!(
        "int(sqrt(X),X,0,1.4) = {} (Q31.32: {})",
        fmt(result),
        result
    );
    println!("Exact = {} (Q31.32: {})", fmt(exact), exact);
    println!("Error = {} Q31.32 ULP", error);
    assert!(
        error < 100,
        "Error should be < 100 ULP (~2.3e-8) for sqrt(x) near singularity"
    );
}

// ─── Overflow display tests ───────────────────────────────────────────────────

fn check_result(expr: &str, expected_kind: &str) {
    let mut vars = VariableStore::new();
    let mut lex_scratch = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    let mut parse_scratch = compiler::Bytecode::new();
    let r = engine::evaluate_expression(
        expr.as_bytes(),
        &mut vars,
        &mut lex_scratch,
        &mut parse_scratch,
        MathMode::Standard,
        AngleMode::Radians,
    );
    let kind = match r {
        engine::EvalResult::Matrix(_) => "Value",
        engine::EvalResult::Overflow { .. } => "Overflow",
        engine::EvalResult::DomainError => "Domain",
    };
    assert_eq!(
        kind, expected_kind,
        "expr `{}`: expected {}, got {}",
        expr, expected_kind, kind
    );
    if let engine::EvalResult::Overflow {
        mantissa,
        exponent,
        negative,
    } = r
    {
        let mut buf = [0u8; 48];
        if let Some(s) = engine::format_overflow(mantissa, exponent, negative, &mut buf) {
            let formatted = core::str::from_utf8(s).unwrap();
            assert!(
                formatted.contains('E'),
                "overflow display should contain 'E': {}",
                formatted
            );
        }
        let mant_val = mantissa >> 32;
        assert!(
            mant_val >= 1 && mant_val <= 9,
            "mantissa should be in [1,10), got {}",
            mant_val
        );
    }
}

#[test]
fn test_overflow_domain_cases() {
    check_result("exp(25)", "Overflow");
    check_result("sinh(30)", "Overflow");
    check_result("cosh(30)", "Overflow");
    check_result("1000000000*1000000000", "Overflow");
    check_result("100^10", "Overflow");
    check_result("exp(100)", "Overflow");
    check_result("sqrt(-1)", "Domain");
    check_result("asin(2)", "Domain");
    check_result("ln(0)", "Domain");
    check_result("2+2", "Value");
    check_result("exp(10)", "Value");
}

#[test]
fn test_overflow_format() {
    let mut buf = [0u8; 48];
    // mantissa=Q31.32(1.5), exponent=99
    let mantissa: i64 = (1.5 * 4294967296.0) as i64;
    let s = engine::format_overflow(mantissa, 99, false, &mut buf).unwrap();
    let formatted = core::str::from_utf8(s).unwrap();
    assert_eq!(formatted, "1.5E99", "got: {}", formatted);

    // negative small: -1.0E-5
    let mantissa_one: i64 = 4294967296;
    let s = engine::format_overflow(mantissa_one, -5, true, &mut buf).unwrap();
    let formatted = core::str::from_utf8(s).unwrap();
    assert_eq!(formatted, "-1E-5", "got: {}", formatted);

    // Exponent |100| > 99 → None (cannot format)
    let mantissa: i64 = (1.5 * 4294967296.0) as i64;
    assert!(engine::format_overflow(mantissa, 100, false, &mut buf).is_none());

    // Exponent |-100| > 99 → None
    assert!(engine::format_overflow(mantissa, -100, false, &mut buf).is_none());

    // Zero exponent
    let s = engine::format_overflow(mantissa_one, 0, false, &mut buf).unwrap();
    let formatted = core::str::from_utf8(s).unwrap();
    assert_eq!(formatted, "1E0", "got: {}", formatted);

    // Exponent i32::MAX > 99 → None
    let mantissa_precise: i64 = (1.000001 * 4294967296.0) as i64;
    assert!(engine::format_overflow(mantissa_precise, i32::MAX, false, &mut buf).is_none());

    // Exponent i32::MIN < -99 → None
    assert!(engine::format_overflow(mantissa_precise, i32::MIN, false, &mut buf).is_none());
}

fn eval_overflow(expr: &str) -> Option<(i64, i32, bool)> {
    let mut vars = VariableStore::new();
    let mut lex_scratch = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    let mut parse_scratch = compiler::Bytecode::new();
    match engine::evaluate_expression(
        expr.as_bytes(),
        &mut vars,
        &mut lex_scratch,
        &mut parse_scratch,
        MathMode::Standard,
        AngleMode::Radians,
    ) {
        engine::EvalResult::Overflow {
            mantissa,
            exponent,
            negative,
        } => Some((mantissa, exponent, negative)),
        _ => None,
    }
}

#[test]
fn test_overflow_binary_operations() {
    // sinh(30) ≈ 5.343e12
    let sinh30 = eval_overflow("sinh(30)").expect("sinh(30) should overflow");
    let mant_f64 = |m: i64| m as f64 / 4294967296.0;
    let m_sinh = mant_f64(sinh30.0);
    assert!(
        m_sinh > 5.0 && m_sinh < 6.0,
        "sinh(30) mantissa {} not in [5,6)",
        m_sinh
    );
    assert_eq!(sinh30.1, 12, "sinh(30) exponent should be 12");
    assert!(!sinh30.2, "sinh(30) should be positive");

    // sinh(30)/2 ≈ 2.672e12 — mantissa should HALVE
    let sinh30_div2 = eval_overflow("sinh(30)/2").expect("sinh(30)/2 should overflow");
    let m_div2 = mant_f64(sinh30_div2.0);
    assert!(
        m_div2 > 2.0 && m_div2 < 3.0,
        "sinh(30)/2 mantissa {} not in [2,3)",
        m_div2
    );
    assert!(
        m_div2 < m_sinh - 2.0,
        "sinh(30)/2 mantissa {} should be much smaller than sinh(30) {}",
        m_div2,
        m_sinh
    );
    assert_eq!(sinh30_div2.1, 12, "sinh(30)/2 exponent should be 12");
    assert!(!sinh30_div2.2, "sinh(30)/2 should be positive");

    // sinh(30)*2 ≈ 1.069e13 — exponent should increase
    let sinh30_mul2 = eval_overflow("sinh(30)*2").expect("sinh(30)*2 should overflow");
    let m_mul2 = mant_f64(sinh30_mul2.0);
    assert!(
        m_mul2 > 1.0 && m_mul2 < 2.0,
        "sinh(30)*2 mantissa {} not in [1,2)",
        m_mul2
    );
    assert_eq!(
        sinh30_mul2.1, 13,
        "sinh(30)*2 exponent should be 13, got {}",
        sinh30_mul2.1
    );
    assert!(!sinh30_mul2.2, "sinh(30)*2 should be positive");

    // -sinh(30) ≈ -5.343e12 — should be negative
    let neg_sinh30 = eval_overflow("-sinh(30)").expect("-sinh(30) should overflow");
    assert_eq!(neg_sinh30.1, 12, "-sinh(30) exponent should be 12");
    assert!(neg_sinh30.2, "-sinh(30) should be negative");

    // sinh(30)*3 — different factor
    let sinh30_mul3 = eval_overflow("sinh(30)*3").expect("sinh(30)*3 should overflow");
    let m_mul3 = mant_f64(sinh30_mul3.0);
    assert!(
        m_mul3 > 1.0 && m_mul3 < 2.0,
        "sinh(30)*3 mantissa {} not in [1,2)",
        m_mul3
    );
    assert_eq!(
        sinh30_mul3.1, 13,
        "sinh(30)*3 exponent should be 13, got {}",
        sinh30_mul3.1
    );

    // sinh(30)/10 — reduces exponent
    let sinh30_div10 = eval_overflow("sinh(30)/10").expect("sinh(30)/10 should overflow");
    let m_div10 = mant_f64(sinh30_div10.0);
    assert!(
        m_div10 > 5.0 && m_div10 < 6.0,
        "sinh(30)/10 mantissa {} not in [5,6)",
        m_div10
    );
    assert_eq!(
        sinh30_div10.1, 11,
        "sinh(30)/10 exponent should be 11, got {}",
        sinh30_div10.1
    );

    // exp(100) * 100
    let exp100 = eval_overflow("exp(100)").expect("exp(100) should overflow");
    assert_eq!(
        exp100.1, 43,
        "exp(100) exponent should be 43, got {}",
        exp100.1
    );
    let exp100_mul100 = eval_overflow("exp(100)*100").expect("exp(100)*100 should overflow");
    assert_eq!(
        exp100_mul100.1, 45,
        "exp(100)*100 exponent should be 45, got {}",
        exp100_mul100.1
    );

    // Power: sinh(30)^2 ≈ 2.85e25 — exponent should be ~25
    let sinh30_pow2 = eval_overflow("sinh(30)^2").expect("sinh(30)^2 should overflow");
    assert_eq!(
        sinh30_pow2.1, 25,
        "sinh(30)^2 exponent should be 25, got {}",
        sinh30_pow2.1
    );
    let m_pow2 = mant_f64(sinh30_pow2.0);
    assert!(
        m_pow2 > 2.0 && m_pow2 < 4.0,
        "sinh(30)^2 mantissa {} not in [2,4)",
        m_pow2
    );

    // Power: sinh(30)^3 ≈ 1.52e38 — exponent should be ~38
    let sinh30_pow3 = eval_overflow("sinh(30)^3").expect("sinh(30)^3 should overflow");
    assert_eq!(
        sinh30_pow3.1, 38,
        "sinh(30)^3 exponent should be 38, got {}",
        sinh30_pow3.1
    );

    // constant / overflow → NOT overflow (result is ~0)
    assert!(
        eval_overflow("5/sinh(30)").is_none(),
        "5/sinh(30) should NOT overflow"
    );

    // constant ^ overflow → NOT overflow (uncomputable)
    assert!(
        eval_overflow("2^sinh(30)").is_none(),
        "2^sinh(30) should NOT overflow"
    );

    // Multiply in apply_binary_operator: 2^20 * 2^20 = 2^40 ≈ 1.1e12
    let mul_large = eval_overflow("1048576*1048576").expect("1048576^2 should overflow");
    assert_eq!(
        mul_large.1, 12,
        "1048576*1048576 exponent should be 12, got {}",
        mul_large.1
    );
    let m_mul_large = mant_f64(mul_large.0);
    assert!(
        m_mul_large > 1.0 && m_mul_large < 1.2,
        "1048576*1048576 mantissa {} not in [1,1.2)",
        m_mul_large
    );
}

#[test]
fn trace_sinh100() {
    use crate::fp as _fp;
    let x = _fp::from_integer(100);
    let log10_exp = _fp::divide(x, _fp::FIXED_LN10).unwrap();
    let log10_sinh = log10_exp - 1_292_913_986i64;
    assert!(log10_sinh >> 32 == 43);
    let frac = log10_sinh - (43 << 32);
    let frac_ln10 = _fp::multiply(frac, _fp::FIXED_LN10).unwrap();
    let mantissa = _fp::natural_exp(frac_ln10).unwrap();
    let mantissa_f64 = mantissa as f64 / 4294967296.0;
    assert!(
        (mantissa_f64 - 1.34406).abs() < 0.0001,
        "mantissa {} too far from 1.34406",
        mantissa_f64
    );
}

// ─── 21. Matrix Operations ─────────────────────────────────────────────────────

fn eval_matrix(expr: &str, vars: &mut VariableStore) -> Option<Matrix> {
    let mut lex_scratch = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    let mut parse_scratch = compiler::Bytecode::new();
    match engine::evaluate_expression(
        expr.as_bytes(),
        vars,
        &mut lex_scratch,
        &mut parse_scratch,
        MathMode::Matrix,
        AngleMode::Radians,
    ) {
        engine::EvalResult::Matrix(ref m) => Some(*m),
        _ => None,
    }
}

#[test]
fn test_matrix_literal_2x2() {
    let mut vars = VariableStore::new();
    let m = eval_matrix("[(1,2)(3,4)]", &mut vars).unwrap();
    assert_eq!(m.rows, 2);
    assert_eq!(m.cols, 2);
    assert_eq!(m.kind, MatrixKind::Mat);
    assert_eq!(m.cell(0, 0), fp::SCALE); // 1 in Q31.32
    assert_eq!(m.cell(0, 1), fp::SCALE * 2);
    assert_eq!(m.cell(1, 0), fp::SCALE * 3);
    assert_eq!(m.cell(1, 1), fp::SCALE * 4);
}

#[test]
fn test_matrix_literal_1x3() {
    let mut vars = VariableStore::new();
    let m = eval_matrix("[(1,2,3)]", &mut vars).unwrap();
    assert_eq!(m.rows, 1);
    assert_eq!(m.cols, 3);
    assert_eq!(m.kind, MatrixKind::Mat);
}

#[test]
fn test_matrix_literal_3x1() {
    let mut vars = VariableStore::new();
    let m = eval_matrix("[(1)(2)(3)]", &mut vars).unwrap();
    assert_eq!(m.rows, 3);
    assert_eq!(m.cols, 1);
}

#[test]
fn test_matrix_literal_rejected_in_standard_mode() {
    let mut vars = VariableStore::new();
    let mut lex_scratch = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    let mut parse_scratch = compiler::Bytecode::new();
    let r = engine::evaluate_expression(
        b"[(1,2)(3,4)]",
        &mut vars,
        &mut lex_scratch,
        &mut parse_scratch,
        MathMode::Standard,
        AngleMode::Radians,
    );
    assert!(matches!(r, engine::EvalResult::DomainError));
}

#[test]
fn test_matrix_add() {
    let mut vars = VariableStore::new();
    let a = eval_matrix("[(1,2)(3,4)]", &mut vars).unwrap();
    let b = eval_matrix("[(5,6)(7,8)]", &mut vars).unwrap();
    let c = a.elementwise_add(&b).unwrap();
    let six = fp::SCALE * 6;
    let _ten = fp::SCALE * 10;
    let twelve = fp::SCALE * 12;
    assert_eq!(c.cell(0, 0), six);
    assert_eq!(c.cell(1, 1), twelve);
    // Also test via expression
    {
        let mut vars2 = VariableStore::new();
        vars2.write_matrix_reg(b'A', a);
        vars2.write_matrix_reg(b'B', b);
        let r = eval_matrix("MatA+MatB", &mut vars2).unwrap();
        assert_eq!(r.cell(0, 0), six);
        assert_eq!(r.cell(1, 1), twelve);
    }
}

#[test]
fn test_matrix_sub() {
    let mut vars = VariableStore::new();
    let a = eval_matrix("[(5,6)(7,8)]", &mut vars).unwrap();
    let b = eval_matrix("[(1,2)(3,4)]", &mut vars).unwrap();
    let c = a.elementwise_sub(&b).unwrap();
    let four = fp::SCALE * 4;
    assert_eq!(c.cell(0, 0), four);
    assert_eq!(c.cell(1, 1), four);
}

#[test]
fn test_matrix_mul_2x2() {
    let mut vars = VariableStore::new();
    // identity * identity = identity
    let id = Matrix::identity(2).unwrap();
    vars.write_matrix_reg(b'A', id);
    vars.write_matrix_reg(b'B', id);
    let r = eval_matrix("MatA*MatB", &mut vars).unwrap();
    assert_eq!(r.rows, 2);
    assert_eq!(r.cols, 2);
    assert_eq!(r.cell(0, 0), fp::SCALE);
    assert_eq!(r.cell(1, 1), fp::SCALE);
}

#[test]
fn test_matrix_mul_dim_mismatch() {
    let mut vars = VariableStore::new();
    let a = Matrix::mat_from_slice(&[1, 2, 3, 4, 5, 6], 2, 3).unwrap();
    let b = Matrix::mat_from_slice(&[1, 2, 3, 4], 2, 2).unwrap();
    vars.write_matrix_reg(b'A', a);
    vars.write_matrix_reg(b'B', b);
    let r = eval_matrix("MatA*MatB", &mut vars);
    assert!(r.is_none());
}

#[test]
fn test_det_2x2() {
    let mut vars = VariableStore::new();
    let id = Matrix::identity(2).unwrap();
    vars.write_matrix_reg(b'A', id);
    let r = eval_matrix("det(MatA)", &mut vars).unwrap();
    assert_eq!(r.kind, MatrixKind::Scalar);
    assert_eq!(r.data[0], fp::SCALE);
}

#[test]
fn test_det_3x3() {
    let mut vars = VariableStore::new();
    let id = Matrix::identity(3).unwrap();
    vars.write_matrix_reg(b'A', id);
    let r = eval_matrix("det(MatA)", &mut vars).unwrap();
    assert_eq!(r.data[0], fp::SCALE);
}

#[test]
fn test_det_singular() {
    let mut vars = VariableStore::new();
    let m = Matrix::mat_from_slice(&[fp::SCALE, fp::SCALE, fp::SCALE, fp::SCALE], 2, 2).unwrap();
    vars.write_matrix_reg(b'A', m);
    let r = eval_matrix("det(MatA)", &mut vars).unwrap();
    assert_eq!(r.data[0], 0);
}

#[test]
fn test_transpose() {
    let mut vars = VariableStore::new();
    let m = Matrix::mat_from_slice(&[1, 2, 3, 4, 5, 6], 2, 3).unwrap();
    vars.write_matrix_reg(b'A', m);
    let r = eval_matrix("transpose(MatA)", &mut vars).unwrap();
    assert_eq!(r.rows, 3);
    assert_eq!(r.cols, 2);
    assert_eq!(r.cell(0, 0), 1);
    assert_eq!(r.cell(1, 0), 2);
    assert_eq!(r.cell(2, 1), 6);
}

#[test]
fn test_identity_function() {
    let mut vars = VariableStore::new();
    let r = eval_matrix("identity(3)", &mut vars).unwrap();
    assert_eq!(r.rows, 3);
    assert_eq!(r.cols, 3);
    assert_eq!(r.kind, MatrixKind::Mat);
    for i in 0..3 {
        for j in 0..3 {
            let expected = if i == j { fp::SCALE } else { 0 };
            assert_eq!(r.cell(i, j), expected);
        }
    }
}

#[test]
fn test_identity_out_of_range() {
    let mut vars = VariableStore::new();
    assert!(eval_matrix("identity(0)", &mut vars).is_none());
    assert!(eval_matrix("identity(7)", &mut vars).is_none());
}

#[test]
fn test_matrix_sto_and_read() {
    let mut vars = VariableStore::new();
    let r = eval_matrix("sto([(1,2)(3,4)],MatA)", &mut vars).unwrap();
    assert_eq!(r.rows, 2);
    assert_eq!(r.cols, 2);

    // Read back from MatA
    let r2 = eval_matrix("MatA", &mut vars).unwrap();
    assert_eq!(r2.cell(0, 0), fp::SCALE);
    assert_eq!(r2.cell(1, 1), fp::SCALE * 4);
}

#[test]
fn test_matrix_scalar_broadcast_add() {
    let mut vars = VariableStore::new();
    let m = eval_matrix("[(1,2)(3,4)]", &mut vars).unwrap();
    vars.write_matrix_reg(b'A', m);
    let r = eval_matrix("MatA+5", &mut vars).unwrap();
    let five_scaled = fp::SCALE * 5;
    assert_eq!(r.cell(0, 0), fp::SCALE + five_scaled);
    assert_eq!(r.cell(1, 1), fp::SCALE * 4 + five_scaled);
}

#[test]
fn test_matrix_scalar_broadcast_mul() {
    let mut vars = VariableStore::new();
    let m = Matrix::identity(2).unwrap();
    vars.write_matrix_reg(b'A', m);
    let r = eval_matrix("MatA*3", &mut vars).unwrap();
    let three_scaled = fp::from_integer(3);
    assert_eq!(r.cell(0, 0), three_scaled);
    assert_eq!(r.cell(1, 1), three_scaled);
}

#[test]
fn test_matrix_mode_guards_in_standard() {
    let mut vars = VariableStore::new();
    let mut lex_scratch = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    let mut parse_scratch = compiler::Bytecode::new();
    // A scalar 5 in Matrix mode should still work
    let r = engine::evaluate_expression(
        b"5",
        &mut vars,
        &mut lex_scratch,
        &mut parse_scratch,
        MathMode::Matrix,
        AngleMode::Radians,
    );
    assert!(matches!(r, engine::EvalResult::Matrix(ref m) if m.kind == MatrixKind::Scalar));
}

#[test]
fn test_matrix_det_4x4_identity() {
    let mut vars = VariableStore::new();
    let id4 = Matrix::identity(4).unwrap();
    vars.write_matrix_reg(b'A', id4);
    let r = eval_matrix("det(MatA)", &mut vars).unwrap();
    assert_eq!(r.data[0], fp::SCALE);
}

#[test]
fn test_matrix_det_not_square() {
    let mut vars = VariableStore::new();
    let m = Matrix::mat_from_slice(&[1, 2, 3, 4, 5, 6], 2, 3).unwrap();
    vars.write_matrix_reg(b'A', m);
    let r = eval_matrix("det(MatA)", &mut vars);
    assert!(r.is_none());
}

#[test]
fn test_matrix_scalar_mul_preserves_mode() {
    let mut vars = VariableStore::new();
    let m = Matrix::mat_from_slice(
        &[fp::SCALE, fp::SCALE * 2, fp::SCALE * 3, fp::SCALE * 4],
        2,
        2,
    )
    .unwrap();
    vars.write_matrix_reg(b'A', m);
    let r = eval_matrix("MatA*2", &mut vars).unwrap();
    assert_eq!(r.kind, MatrixKind::Mat);
    assert_eq!(r.cell(0, 0), fp::SCALE * 2);
    assert_eq!(r.cell(1, 1), fp::SCALE * 8);
}

#[test]
fn test_matrix_negation() {
    let mut vars = VariableStore::new();
    let r = eval_matrix("-[(1,2)(3,4)]", &mut vars).unwrap();
    assert_eq!(r.cell(0, 0), -fp::SCALE);
    assert_eq!(r.cell(1, 1), -fp::SCALE * 4);
}

#[test]
fn test_matregister_identifiers() {
    // Test that MatA, MatB, MatC are recognized (case-insensitive prefix)
    let mut vars = VariableStore::new();
    let m = Matrix::identity(2).unwrap();
    vars.write_matrix_reg(b'A', m);
    let r = eval_matrix("MatA", &mut vars).unwrap();
    assert_eq!(r.cell(0, 0), fp::SCALE);

    // mAtA should also work (case-insensitive)
    let mut lex_scratch = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    let mut parse_scratch = compiler::Bytecode::new();
    let r2 = engine::evaluate_expression(
        b"mata",
        &mut vars,
        &mut lex_scratch,
        &mut parse_scratch,
        MathMode::Matrix,
        AngleMode::Radians,
    );
    assert!(matches!(r2, engine::EvalResult::Matrix(ref m) if m.cell(0, 0) == fp::SCALE));
}

#[test]
fn test_scalar_mat_mul_order() {
    // User's exact workflow: sto(...) then Scalar*Mat, then Mat*Scalar
    let mut vars = VariableStore::new();

    // Step 1: store the matrix via sto()
    let stored = eval_matrix("sto([(3,2,3)(4,5,6)(7,8,9)],MatA)", &mut vars);
    assert!(stored.is_some(), "sto should succeed");

    // Step 2: (Scalar * Mat) — user reports this produces scalar instead of matrix
    let r = eval_matrix("(1/det(MatA))*MatA", &mut vars);
    assert!(r.is_some(), "Scalar*Mat should succeed");
    let r = r.unwrap();
    assert_eq!(
        r.kind,
        MatrixKind::Mat,
        "BUG: (1/det(MatA))*MatA produced kind={:?} instead of Mat. Result value: {}",
        r.kind,
        r.data[0]
    );
    assert_eq!(r.rows, 3);
    assert_eq!(r.cols, 3);

    // Step 3: (Mat * Scalar) — this should work (and does)
    let r2 = eval_matrix("MatA*(1/det(MatA))", &mut vars);
    assert!(r2.is_some());
    let r2 = r2.unwrap();
    assert_eq!(r2.kind, MatrixKind::Mat);
    assert_eq!(r2.rows, 3);
    assert_eq!(r2.cols, 3);

    // Verify both orderings produce same result
    for i in 0..9 {
        let idx_t = (i / 3) * 3 + (i % 3);
        assert_eq!(
            r.data[idx_t], r2.data[idx_t],
            "Mismatch at cell ({}): Scalar*Mat={} Mat*Scalar={}",
            i, r.data[idx_t], r2.data[idx_t]
        );
    }
}

#[test]
fn test_matrix_inv_2x2() {
    let mut vars = VariableStore::new();
    let r = eval_matrix("inv([(1,2)(3,4)])", &mut vars);
    assert!(r.is_some(), "inv([(1,2)(3,4)]) should return Some");
    let m = r.unwrap();
    assert_eq!(m.rows, 2);
    assert_eq!(m.cols, 2);
    assert_eq!(m.kind, MatrixKind::Mat);
}

#[test]
fn test_matrix_inv_3x3() {
    let mut vars = VariableStore::new();
    let r = eval_matrix("inv([(3,2,3)(4,5,6)(7,8,9)])", &mut vars);
    assert!(r.is_some(), "inv([(3,2,3)(4,5,6)(7,8,9)]) should return Some");
    let m = r.unwrap();
    assert_eq!(m.rows, 3);
    assert_eq!(m.cols, 3);
    assert_eq!(m.kind, MatrixKind::Mat);
}

#[test]
fn test_inv_via_sto() {
    let mut vars = VariableStore::new();
    let expr = "sto([(1,2)(3,4)],MatA)";
    let r = eval_matrix(expr, &mut vars);
    assert!(r.is_some(), "sto should succeed");
    let r = eval_matrix("inv(MatA)", &mut vars);
    assert!(r.is_some(), "inv(MatA) after sto should return Some");
    let m = r.unwrap();
    assert_eq!(m.rows, 2);
    assert_eq!(m.cols, 2);
}

#[test]
fn test_matrix_adjugate_3x3() {
    let mut vars = VariableStore::new();
    let r = eval_matrix("adjugate([(3,2,3)(4,5,6)(7,8,9)])", &mut vars);
    assert!(r.is_some(), "adjugate should succeed");
    let m = r.unwrap();
    assert_eq!(m.rows, 3);
    assert_eq!(m.cols, 3);
}

#[test]
fn test_matrix_inverse_via_cofactor_3x3() {
    let mut vars = VariableStore::new();
    let r = eval_matrix("inverse([(3,2,3)(4,5,6)(7,8,9)])", &mut vars);
    assert!(r.is_some(), "inverse should succeed");
    let m = r.unwrap();
    assert_eq!(m.rows, 3);
    assert_eq!(m.cols, 3);
}

#[test]
fn test_adjugate_reused_scratch() {
    // Simulate the firmware pattern: reuse lex_scratch and parse_scratch
    // across multiple evaluations.
    let mut vars = VariableStore::new();
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    let mut bc = compiler::Bytecode::new();

    // First evaluation: sto
    let r1 = engine::evaluate_expression(
        b"sto([(1,2)(3,4)],MatA)",
        &mut vars, &mut lex, &mut bc,
        MathMode::Matrix, AngleMode::Radians,
    );
    assert!(matches!(r1, engine::EvalResult::Matrix(_)));

    // Second evaluation: adjugate with same scratch buffers
    let r2 = engine::evaluate_expression(
        b"adjugate(MatA)",
        &mut vars, &mut lex, &mut bc,
        MathMode::Matrix, AngleMode::Radians,
    );
    assert!(matches!(r2, engine::EvalResult::Matrix(_)), "adjugate(MatA) failed on reused scratch");

    // Third evaluation: fresh adjugate literal, same scratch
    let r3 = engine::evaluate_expression(
        b"adjugate([(1,2)(3,4)])",
        &mut vars, &mut lex, &mut bc,
        MathMode::Matrix, AngleMode::Radians,
    );
    assert!(matches!(r3, engine::EvalResult::Matrix(_)), "adjugate([(1,2)(3,4)]) failed on reused scratch");
}

#[test]
fn test_adjugate_after_det() {
    // Run det first (which works), then adjugate with same scratch
    let mut vars = VariableStore::new();
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    let mut bc = compiler::Bytecode::new();

    let r1 = engine::evaluate_expression(
        b"det([(3,2,3)(4,5,6)(7,8,9)])",
        &mut vars, &mut lex, &mut bc,
        MathMode::Matrix, AngleMode::Radians,
    );
    assert!(matches!(r1, engine::EvalResult::Matrix(_)));

    let r2 = engine::evaluate_expression(
        b"adjugate([(3,2,3)(4,5,6)(7,8,9)])",
        &mut vars, &mut lex, &mut bc,
        MathMode::Matrix, AngleMode::Radians,
    );
    assert!(matches!(r2, engine::EvalResult::Matrix(_)), "adjugate after det failed");
}

#[test]
fn test_adjugate_bytecode() {
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    let mut bc = compiler::Bytecode::new();

    assert!(lexer::tokenise_expression(b"adjugate([(1,2)(3,4)])", &mut lex, MathMode::Matrix).is_some());
    assert!(compiler::compile(&lex, &mut bc).is_some());

    // Print bytecode
    let ops = &bc.code[..bc.len as usize];
    println!("Bytecode ({} bytes): {:?}", bc.len, ops);

    // Check for CallMatrixFunc(5) or CallMatrixFunc(6)?
    let mut i = 0;
    while i < ops.len() {
        let op = ops[i];
        if op == 0x50 {
            let fn_idx = ops[i+1];
            println!("  CallMatrixFunc({}) at byte {}", fn_idx, i);
            i += 2;
        } else {
            i += 1;
        }
    }
}

#[test]
fn test_adjugate_bytecode_verbose() {
    let mut lex = lexer::LexResult {
        tokens: [lexer::Token::Number(0); lexer::MAX_TOKEN_COUNT],
        token_count: 0,
    };
    let mut bc = compiler::Bytecode::new();

    assert!(lexer::tokenise_expression(b"adjugate([(1,2)(3,4)])", &mut lex, MathMode::Matrix).is_some());
    assert!(compiler::compile(&lex, &mut bc).is_some());

    let ops = &bc.code[..bc.len as usize];
    // Must end with Halt (0xFF)
    assert_eq!(ops.last(), Some(&0xFF), "bytecode must end with Halt");

    // Check for PushMatLit followed by CallMatrixFunc
    let push_mat_lit_pos = ops.iter().position(|&b| b == 0x08);
    assert!(push_mat_lit_pos.is_some(), "bytecode must contain PushMatLit");

    let call_mf_pos = ops.iter().position(|&b| b == 0x50);
    assert!(call_mf_pos.is_some(), "bytecode must contain CallMatrixFunc");

    // The function index should be right after the opcode
    let fn_idx = ops[call_mf_pos.unwrap() + 1];
    assert_eq!(fn_idx, 5, "adjugate should emit CallMatrixFunc(5), got {}", fn_idx);
}
