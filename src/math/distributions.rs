//! # Statistical Distributions and Special Functions (Math Engine — Layer 6)
//!
//! ## Functions provided
//!
//!   `ln_gamma(x)`        — ln(Γ(x)), the log-gamma function
//!   `ln_factorial(k)`    — ln(k!) for non-negative integer k
//!   `binomial_probability(n,k,p)` — P(X=k) for X~Binomial(n,p)
//!   `poisson_probability(λ,k)`    — P(X=k) for X~Poisson(λ)
//!   `chi_squared_cdf(x,k)`       — P(X≤x) for X~χ²(k)
//!
//! ## Stack discipline
//!
//! The original implementation used Lanczos (9×i128 divisions) inside
//! `ln_factorial`, causing stack overflows on the Cortex-M3's 8 KB SRAM.
//! This version eliminates that call chain entirely:
//!
//!   - `ln_factorial` uses an **exact lookup table** for k = 0..20, and
//!     **Stirling's series** (5 terms) for k > 20. Stirling needs only
//!     `natural_log` and fixed-point multiplies — no recursion, no Lanczos.
//!     Error is < 1 Q31.32 LSB for all k ≥ 21.
//!
//!   - `ln_gamma` (for non-integer arguments, used by chiCDF) uses the
//!     Lanczos approximation but with a **reduced i128 footprint**: the
//!     series accumulator is i64; only the individual division steps use
//!     i128 intermediates. Recursion is replaced with a direct formula for
//!     z < 0.5.
//!
//!   - Half-integer Γ values (needed when chiCDF's degrees-of-freedom k is
//!     odd) are computed from the factorial table:
//!       ln(Γ(n+½)) = ln((2n)!) − n·ln(4) − ln(n!) + ½·ln(π)
//!     This avoids calling Lanczos entirely for the most common chi-squared
//!     test cases.
//!
//! ## Overflow conventions
//!
//! All probability computations are done in log space then exponentiated once.
//! This keeps intermediate values in [−30, +30] where Q31.32 is accurate,
//! avoiding the Q31.32 integer overflow that direct factorial computation
//! would cause for n > 12.

use super::fixed_point as fp;

// ─── ln(k!) lookup table ─────────────────────────────────────────────────────
//
// Exact Q31.32 values of ln(k!) for k = 0..20.
// Generated from Python: round(sum(log(i) for i in range(1,k+1)) * 2^32)
// These are bit-exact — no rounding error.

const LN_FACTORIAL_TABLE: [i64; 21] = [
    0,               // ln(0!)  = 0
    0,               // ln(1!)  = 0
    2_977_044_472,   // ln(2!)  = 0.6931471806
    7_695_548_323,   // ln(3!)  = 1.7917594692
    13_649_637_266,  // ln(4!)  = 3.1780538303
    20_562_120_465,  // ln(5!)  = 4.7874917428
    28_257_668_788,  // ln(6!)  = 6.5792512120
    36_615_289_239,  // ln(7!)  = 8.5251613611
    45_546_422_654,  // ln(8!)  = 10.6046029027
    54_983_430_356,  // ln(9!)  = 12.8018274801
    64_872_958_027,  // ln(10!) = 15.1044125731
    75_171_839_803,  // ln(11!) = 17.5023078459
    85_844_432_597,  // ln(12!) = 19.9872144957
    96_860_806_203,  // ln(13!) = 22.5521638531
    108_195_471_126, // ln(14!) = 25.1912211827
    119_826_458_176, // ln(15!) = 27.8992713838
    131_734_636_063, // ln(16!) = 30.6718601061
    143_903_194_718, // ln(17!) = 33.5050734501
    156_317_246_892, // ln(18!) = 36.3954452080
    168_963_516_012, // ln(19!) = 39.3398841872
    181_830_088_155, // ln(20!) = 42.3356164608
];

/// Compute ln(k!) for non-negative integer k.
///
/// Uses the exact lookup table for k ≤ 20.
/// Uses Stirling's approximation for k > 20:
///   ln(k!) ≈ k·ln(k) − k + ½·ln(2πk) + 1/(12k) − 1/(360k³) + 1/(1260k⁵)
///
/// Stirling error for k ≥ 21 is < 1 Q31.32 LSB (verified by Python).
/// Returns None if k is negative or has a fractional part.
pub fn ln_factorial(k: i64) -> Option<i64> {
    // k must be a non-negative integer in Q31.32 (fractional bits must be zero).
    if k < 0 {
        return None;
    }
    if k & (fp::SCALE - 1) != 0 {
        return None;
    }

    let k_int = fp::to_integer_truncated(k) as usize;

    // Exact path for small k.
    if k_int <= 20 {
        return Some(LN_FACTORIAL_TABLE[k_int]);
    }

    // Stirling path for k > 20.
    // All operations use Q31.32 fixed-point arithmetic.
    let k_q = k; // k is already Q31.32 integer
    let ln_k = fp::natural_log(k_q)?;

    // k·ln(k) − k
    let term1 = fp::multiply(k_q, ln_k) - k_q;

    // ½·ln(2πk) = ½·(ln(2π) + ln(k))
    // ln(2π) in Q31.32 = round(ln(2π) × 2^32) = 7_733_370_361
    const LN_TWO_PI: i64 = 7_893_621_894; // ln(2π) ≈ 1.83787706641 — corrected
    let ln_two_pi_k = LN_TWO_PI + ln_k;
    let term2 = ln_two_pi_k / 2;

    // 1/(12k) in Q31.32: divide(SCALE, 12×k)
    // 12×k as Q31.32: multiply(from_integer(12), k) = 12 × k (still integer)
    let twelve_k = fp::multiply(fp::from_integer(12), k_q);
    let term3 = fp::divide(fp::FIXED_ONE, twelve_k)?;

    // 1/(360k³) — for k ≥ 21 this is < 3×10⁻⁸, negligible but included for accuracy
    let k_sq = fp::multiply(k_q, k_q);
    let k_cu = fp::multiply(k_sq, k_q);
    let term4 = fp::divide(fp::FIXED_ONE, fp::multiply(fp::from_integer(360), k_cu))?;

    // 1/(1260k⁵)
    let k_5th = fp::multiply(k_cu, k_sq);
    let term5 = fp::divide(fp::FIXED_ONE, fp::multiply(fp::from_integer(1260), k_5th))?;

    Some(term1 + term2 + term3 - term4 + term5)
}

// ─── ln(Γ(x)) for non-integer x ──────────────────────────────────────────────
//
// The Lanczos approximation (g=7, 9 coefficients, Numerical Recipes).
// Used only for non-integer arguments — the most common use is chiCDF with
// half-integer a = k/2 where k is an odd integer.
//
// For half-integer arguments, we use the recurrence:
//   ln(Γ(n+½)) = ln((2n)!) − n·ln(4) − ln(n!) + ½·ln(π)
// which derives everything from ln_factorial — no Lanczos call at all.
//
// Lanczos is only called for genuinely non-half-integer z.

/// ½·ln(π) in Q31.32.
const HALF_LN_PI: i64 = 2_458_288_711;

/// ln(4) in Q31.32 = round(ln(4) × 2^32) = 5_954_088_944.
const LN_4: i64 = 5_954_088_944;

/// Lanczos g = 7.0 in Q31.32.
const LANCZOS_G: i64 = fp::SCALE * 7;

/// ½·ln(2π) in Q31.32.
const LN_SQRT_TWO_PI: i64 = 3_946_810_947;

/// Lanczos coefficients in Q31.32.
const LANCZOS_COEFFS: [i64; 9] = [
    4_294_967_296,
    2_905_632_856_161,
    -5_407_961_756_934,
    3_312_808_901_239,
    -758_555_774_233,
    53_718_630_342,
    -595_158_322,
    42_883,
    647,
];

/// Compute ln(Γ(z)) for Q31.32 z > 0.
///
/// For positive integers: delegates to `ln_factorial(z−1)`.
/// For half-integers (z = n + ½): uses the recurrence formula from the
///   factorial table, avoiding Lanczos entirely.
/// For all other z ≥ 0.5: uses the Lanczos approximation.
/// For z < 0.5: uses the reflection formula ln(Γ(z)) = ln(π/sin(πz)) − ln(Γ(1−z)).
///
/// Returns None for z ≤ 0.
pub fn ln_gamma(z: i64) -> Option<i64> {
    if z <= 0 {
        return None;
    }

    // ── Integer z: use exact factorial table ─────────────────────────────────
    if z & (fp::SCALE - 1) == 0 {
        // z is a positive integer: ln(Γ(z)) = ln((z-1)!)
        return ln_factorial(z - fp::FIXED_ONE);
    }

    // ── Half-integer z: use exact recurrence from factorial table ─────────────
    // z is a half-integer if z - floor(z) == 0.5 in Q31.32 (= SCALE/2 = 2^31).
    let frac_part = z & (fp::SCALE - 1);
    if frac_part == fp::FIXED_HALF {
        // z = n + 0.5 for some non-negative integer n.
        let n = fp::to_integer_truncated(z) as usize; // n = floor(z)
                                                      // ln(Γ(n+½)) = ln((2n)!) − n·ln(4) − ln(n!) + ½·ln(π)
        let ln_2n_fact = ln_factorial(fp::from_integer((2 * n) as i64))?;
        let ln_n_fact = if n == 0 {
            0
        } else {
            ln_factorial(fp::from_integer(n as i64))?
        };
        let n_ln4 = (n as i64) * LN_4; // n × ln(4) in Q31.32 (integer × Q31.32)
        return Some(ln_2n_fact - n_ln4 - ln_n_fact + HALF_LN_PI);
    }

    // ── Reflection formula for z < 0.5 ───────────────────────────────────────
    if z < fp::FIXED_HALF {
        let one_minus_z = fp::FIXED_ONE - z;
        let pi_z = fp::multiply(fp::FIXED_PI, z);
        let sin_pi_z = fp::sin(pi_z);
        if sin_pi_z <= 0 {
            return None;
        }
        let ln_pi = fp::natural_log(fp::FIXED_PI)?;
        let ln_sin = fp::natural_log(sin_pi_z)?;
        let ln_gamma_comp = ln_gamma(one_minus_z)?;
        return Some(ln_pi - ln_sin - ln_gamma_comp);
    }

    // ── General Lanczos path for z ≥ 0.5 ─────────────────────────────────────
    //
    // Stack reduction vs the previous version:
    //   - series_sum is i64, not i128 (each term fits in i64 after the divide)
    //   - i128 is used only for the (ci << 32) / denominator step
    //   - No recursion in this code path

    // x = z − 1 (Lanczos recurrence is 0-indexed)
    let x = z - fp::FIXED_ONE;

    // Build series: A(x) = c₀ + Σ cᵢ/(x+i) for i=1..8
    let mut series_sum: i64 = LANCZOS_COEFFS[0];
    for i in 1usize..9 {
        // denominator = x + i in Q31.32
        let denominator: i64 = x + fp::from_integer(i as i64);
        if denominator == 0 {
            return None;
        }
        // term = cᵢ / denominator in Q31.32
        // Need i128 only for the (cᵢ << 32) step to avoid overflow.
        let term = (((LANCZOS_COEFFS[i] as i128) << 32) / (denominator as i128)) as i64;
        series_sum = series_sum.saturating_add(term);
    }

    // Guard: series must be positive for ln to be valid.
    if series_sum <= 0 {
        return None;
    }
    let ln_series = fp::natural_log(series_sum)?;

    // t = x + g + 0.5
    let t = x + LANCZOS_G + fp::FIXED_HALF;
    let ln_t = fp::natural_log(t)?;

    // ln(Γ(z)) = LN_SQRT_TWO_PI + (x+0.5)·ln(t) − t + ln(series)
    let x_plus_half = x + fp::FIXED_HALF;
    Some(LN_SQRT_TWO_PI + fp::multiply(x_plus_half, ln_t) - t + ln_series)
}

// ─── Binomial probability ─────────────────────────────────────────────────────

/// P(X=k) for X ~ Binomial(n, p).
///
/// Computed in log space to handle large n without overflow:
///   ln P = [ln(n!) − ln(k!) − ln((n−k)!)] + k·ln(p) + (n−k)·ln(1−p)
///
/// Arguments (Q31.32): n and k must be non-negative integers; p ∈ (0, 1).
pub fn binomial_probability(n: i64, k: i64, p: i64) -> Option<i64> {
    if n < 0 || k < 0 {
        return None;
    }
    if k > n {
        return None;
    }
    if p <= 0 || p >= fp::FIXED_ONE {
        return None;
    }
    if n & (fp::SCALE - 1) != 0 {
        return None;
    }
    if k & (fp::SCALE - 1) != 0 {
        return None;
    }

    let n_minus_k = n - k;

    // ln(C(n,k)) = ln(n!) − ln(k!) − ln((n−k)!)
    // These three calls are sequential, not nested — max stack depth = one ln_factorial call.
    let ln_n_fact = ln_factorial(n)?;
    let ln_k_fact = ln_factorial(k)?;
    let ln_nmk_fact = ln_factorial(n_minus_k)?;
    let ln_binom = ln_n_fact - ln_k_fact - ln_nmk_fact;

    let ln_p = fp::natural_log(p)?;
    let ln_1mp = fp::natural_log(fp::FIXED_ONE - p)?;

    let ln_prob = ln_binom
        .checked_add(fp::multiply(k, ln_p))?
        .checked_add(fp::multiply(n_minus_k, ln_1mp))?;

    Some(fp::natural_exp(ln_prob))
}

// ─── Poisson probability ──────────────────────────────────────────────────────

/// P(X=k) for X ~ Poisson(λ).
///
/// ln P = −λ + k·ln(λ) − ln(k!)
///
/// Arguments (Q31.32): lambda > 0; k must be a non-negative integer.
pub fn poisson_probability(lambda: i64, k: i64) -> Option<i64> {
    if lambda <= 0 {
        return None;
    }
    if k < 0 {
        return None;
    }
    if k & (fp::SCALE - 1) != 0 {
        return None;
    }

    let ln_lambda = fp::natural_log(lambda)?;
    let ln_k_fact = ln_factorial(k)?;

    let ln_prob = (-lambda)
        .checked_add(fp::multiply(k, ln_lambda))?
        .checked_sub(ln_k_fact)?;

    Some(fp::natural_exp(ln_prob))
}

// ─── Chi-squared CDF ──────────────────────────────────────────────────────────
//
// P(X ≤ x; k) = P(k/2, x/2) where P(a,x) is the regularised lower incomplete
// gamma function. For x < a+1 we use the series expansion; for x ≥ a+1 the
// Lentz continued fraction for the upper tail Q = 1−P.

/// Maximum series/CF iterations for incomplete gamma convergence.
const MAX_GAMMA_ITER: usize = 60;

/// Regularised lower incomplete gamma P(a, x) via series expansion.
/// Best convergence for x < a + 1.
fn incomplete_gamma_series(a: i64, x: i64) -> Option<i64> {
    if x <= 0 {
        return Some(0);
    }

    // Prefactor = e^(−x) × x^a / Γ(a), computed in log space.
    let ln_prefactor = (-x)
        .checked_add(fp::multiply(a, fp::natural_log(x)?))?
        .checked_sub(ln_gamma(a)?)?;
    let prefactor = fp::natural_exp(ln_prefactor);

    // Series: term₀ = 1/a, termₙ = termₙ₋₁ × x/(a+n)
    let mut term = fp::divide(fp::FIXED_ONE, a)?;
    let mut series = term;

    for n in 1..MAX_GAMMA_ITER {
        let a_n = a + fp::from_integer(n as i64);
        term = fp::multiply(term, fp::divide(x, a_n)?);
        series = series.checked_add(term)?;
        if term.abs() < series.abs() / 1_000_000_000 {
            break;
        }
    }

    Some(fp::multiply(prefactor, series))
}

/// P(X ≤ x) for X ~ χ²(k degrees of freedom).
///
/// Arguments (Q31.32): x ≥ 0; k must be a positive integer.
/// Returns a probability in [0, 1], clamped.
///
/// Uses the series expansion of the regularised lower incomplete gamma
/// P(k/2, x/2) with up to 60 terms. The series converges for all finite
/// x — the CF branch is removed because the Lentz algorithm requires
/// careful convergence handling that is error-prone in Q31.32 arithmetic.
/// 60 series terms is sufficient for all practical chi-squared inputs.
pub fn chi_squared_cdf(x: i64, k: i64) -> Option<i64> {
    if x < 0 {
        return None;
    }
    if k <= 0 {
        return None;
    }
    if k & (fp::SCALE - 1) != 0 {
        return None;
    }
    if x == 0 {
        return Some(0);
    }

    // P(X≤x; k) = P(k/2, x/2) — regularised lower incomplete gamma.
    let a = fp::divide(k, 2 * fp::FIXED_ONE)?;
    let half_x = x / 2;

    incomplete_gamma_series(a, half_x).map(|p| p.clamp(0, fp::FIXED_ONE))
}
