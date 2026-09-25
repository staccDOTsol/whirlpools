//! Whirlpool price/liquidity math, ported from programs/whirlpool/src/math.
//! sqrt prices are Q64.64 fixed point (u128). Liquidity is u128.
use crate::error::LaunchError;
use pinocchio::program_error::ProgramError;
use uint::construct_uint;

construct_uint! {
    pub struct U256(4);
}

pub const Q64_RESOLUTION: u8 = 64;
pub const Q64_MASK: u128 = 0xFFFF_FFFF_FFFF_FFFF;
pub const MAX_SQRT_PRICE_X64: u128 = 79226673515401279992447579055;
pub const MIN_SQRT_PRICE_X64: u128 = 4295048016;

#[inline(always)]
fn mul_u256(a: u128, b: u128) -> U256 {
    U256::from(a) * U256::from(b)
}

fn increasing(a: u128, b: u128) -> (u128, u128) {
    if a > b {
        (b, a)
    } else {
        (a, b)
    }
}

/// Token A (the launch token) held by `liquidity` between two sqrt prices.
/// Δa = L * (sqrt_upper - sqrt_lower) / (sqrt_upper * sqrt_lower)
pub fn amount_delta_a(sqrt_0: u128, sqrt_1: u128, liquidity: u128, round_up: bool) -> Result<u64, ProgramError> {
    let (lo, hi) = increasing(sqrt_0, sqrt_1);
    let numerator = mul_u256(liquidity, hi - lo) << 64;
    let denominator = mul_u256(hi, lo);
    if denominator.is_zero() {
        return Err(LaunchError::MathOverflow.into());
    }
    let (q, r) = numerator.div_mod(denominator);
    let q = if round_up && !r.is_zero() { q + U256::one() } else { q };
    if q > U256::from(u64::MAX) {
        return Err(LaunchError::MathOverflow.into());
    }
    Ok(q.as_u64())
}

/// Token B (SOL) held by `liquidity` between two sqrt prices.
/// Δb = L * (sqrt_upper - sqrt_lower)
pub fn amount_delta_b(sqrt_0: u128, sqrt_1: u128, liquidity: u128, round_up: bool) -> Result<u64, ProgramError> {
    let (lo, hi) = increasing(sqrt_0, sqrt_1);
    let n1 = hi - lo;
    if liquidity == 0 || n1 == 0 {
        return Ok(0);
    }
    let p = mul_u256(liquidity, n1);
    let q = p >> 64;
    if q > U256::from(u64::MAX) {
        return Err(LaunchError::MathOverflow.into());
    }
    let mut result = q.as_u64();
    if round_up && (p & U256::from(Q64_MASK)) != U256::zero() {
        result = result.checked_add(1).ok_or(LaunchError::MathOverflow)?;
    }
    Ok(result)
}

/// Liquidity that `amount_b` of SOL provides between two sqrt prices (floor).
/// L = amount_b << 64 / (sqrt_upper - sqrt_lower)
pub fn liquidity_from_token_b(amount_b: u64, sqrt_0: u128, sqrt_1: u128) -> Result<u128, ProgramError> {
    let (lo, hi) = increasing(sqrt_0, sqrt_1);
    let d = hi - lo;
    if d == 0 {
        return Err(LaunchError::MathOverflow.into());
    }
    let n = U256::from(amount_b) << 64;
    let q = n / U256::from(d);
    if q > U256::from(u128::MAX) {
        return Err(LaunchError::MathOverflow.into());
    }
    Ok(q.as_u128())
}


/// Liquidity that `amount_a` of token A provides between two sqrt prices (floor).
/// L = amount_a * sqrt_lower * sqrt_upper / ((sqrt_upper - sqrt_lower) << 64)
pub fn liquidity_from_token_a(amount_a: u64, sqrt_0: u128, sqrt_1: u128) -> Result<u128, ProgramError> {
    let (lo, hi) = increasing(sqrt_0, sqrt_1);
    let d = hi - lo;
    if d == 0 {
        return Err(LaunchError::MathOverflow.into());
    }
    let n = (U256::from(amount_a) * U256::from(lo) * U256::from(hi)) >> 64;
    let q = n / U256::from(d);
    if q > U256::from(u128::MAX) {
        return Err(LaunchError::MathOverflow.into());
    }
    Ok(q.as_u128())
}

/// Floor division to a multiple of `spacing` (works for negative ticks).
#[inline(always)]
pub fn floor_to_spacing(tick: i32, spacing: i32) -> i32 {
    tick.div_euclid(spacing) * spacing
}

/// Start index of the tick array that contains `tick`.
#[inline(always)]
pub fn tick_array_start(tick: i32, spacing: i32) -> i32 {
    floor_to_spacing(tick, spacing * crate::constants::TICK_ARRAY_SIZE)
}

/// Scale `liquidity` down so its token-A requirement fits `available_a`
/// (floor of liquidity * available / needed).
pub fn scale_liquidity(liquidity: u128, needed_a: u64, available_a: u64) -> Result<u128, ProgramError> {
    if needed_a == 0 {
        return Ok(liquidity);
    }
    let q = mul_u256(liquidity, available_a as u128) / U256::from(needed_a);
    if q > U256::from(u128::MAX) {
        return Err(LaunchError::MathOverflow.into());
    }
    Ok(q.as_u128())
}

pub fn sqrt_price_from_tick_index(tick: i32) -> u128 {
    if tick >= 0 {
        get_sqrt_price_positive_tick(tick)
    } else {
        get_sqrt_price_negative_tick(tick)
    }
}

// ---- the bit-ladder implementations below are copied verbatim from
// ---- programs/whirlpool/src/math/tick_math.rs (Apache-2.0, Orca).
fn mul_shift_96(n0: u128, n1: u128) -> u128 {
    (mul_u256(n0, n1) >> 96).as_u128()
}

// Performs the exponential conversion with Q64.64 precision
fn get_sqrt_price_positive_tick(tick: i32) -> u128 {
    let mut ratio: u128 = if tick & 1 != 0 {
        79232123823359799118286999567
    } else {
        79228162514264337593543950336
    };

    if tick & 2 != 0 {
        ratio = mul_shift_96(ratio, 79236085330515764027303304731);
    }
    if tick & 4 != 0 {
        ratio = mul_shift_96(ratio, 79244008939048815603706035061);
    }
    if tick & 8 != 0 {
        ratio = mul_shift_96(ratio, 79259858533276714757314932305);
    }
    if tick & 16 != 0 {
        ratio = mul_shift_96(ratio, 79291567232598584799939703904);
    }
    if tick & 32 != 0 {
        ratio = mul_shift_96(ratio, 79355022692464371645785046466);
    }
    if tick & 64 != 0 {
        ratio = mul_shift_96(ratio, 79482085999252804386437311141);
    }
    if tick & 128 != 0 {
        ratio = mul_shift_96(ratio, 79736823300114093921829183326);
    }
    if tick & 256 != 0 {
        ratio = mul_shift_96(ratio, 80248749790819932309965073892);
    }
    if tick & 512 != 0 {
        ratio = mul_shift_96(ratio, 81282483887344747381513967011);
    }
    if tick & 1024 != 0 {
        ratio = mul_shift_96(ratio, 83390072131320151908154831281);
    }
    if tick & 2048 != 0 {
        ratio = mul_shift_96(ratio, 87770609709833776024991924138);
    }
    if tick & 4096 != 0 {
        ratio = mul_shift_96(ratio, 97234110755111693312479820773);
    }
    if tick & 8192 != 0 {
        ratio = mul_shift_96(ratio, 119332217159966728226237229890);
    }
    if tick & 16384 != 0 {
        ratio = mul_shift_96(ratio, 179736315981702064433883588727);
    }
    if tick & 32768 != 0 {
        ratio = mul_shift_96(ratio, 407748233172238350107850275304);
    }
    if tick & 65536 != 0 {
        ratio = mul_shift_96(ratio, 2098478828474011932436660412517);
    }
    if tick & 131072 != 0 {
        ratio = mul_shift_96(ratio, 55581415166113811149459800483533);
    }
    if tick & 262144 != 0 {
        ratio = mul_shift_96(ratio, 38992368544603139932233054999993551);
    }

    ratio >> 32
}

fn get_sqrt_price_negative_tick(tick: i32) -> u128 {
    let abs_tick = tick.abs();

    let mut ratio: u128 = if abs_tick & 1 != 0 {
        18445821805675392311
    } else {
        18446744073709551616
    };

    if abs_tick & 2 != 0 {
        ratio = (ratio * 18444899583751176498) >> 64
    }
    if abs_tick & 4 != 0 {
        ratio = (ratio * 18443055278223354162) >> 64
    }
    if abs_tick & 8 != 0 {
        ratio = (ratio * 18439367220385604838) >> 64
    }
    if abs_tick & 16 != 0 {
        ratio = (ratio * 18431993317065449817) >> 64
    }
    if abs_tick & 32 != 0 {
        ratio = (ratio * 18417254355718160513) >> 64
    }
    if abs_tick & 64 != 0 {
        ratio = (ratio * 18387811781193591352) >> 64
    }
    if abs_tick & 128 != 0 {
        ratio = (ratio * 18329067761203520168) >> 64
    }
    if abs_tick & 256 != 0 {
        ratio = (ratio * 18212142134806087854) >> 64
    }
    if abs_tick & 512 != 0 {
        ratio = (ratio * 17980523815641551639) >> 64
    }
    if abs_tick & 1024 != 0 {
        ratio = (ratio * 17526086738831147013) >> 64
    }
    if abs_tick & 2048 != 0 {
        ratio = (ratio * 16651378430235024244) >> 64
    }
    if abs_tick & 4096 != 0 {
        ratio = (ratio * 15030750278693429944) >> 64
    }
    if abs_tick & 8192 != 0 {
        ratio = (ratio * 12247334978882834399) >> 64
    }
    if abs_tick & 16384 != 0 {
        ratio = (ratio * 8131365268884726200) >> 64
    }
    if abs_tick & 32768 != 0 {
        ratio = (ratio * 3584323654723342297) >> 64
    }
    if abs_tick & 65536 != 0 {
        ratio = (ratio * 696457651847595233) >> 64
    }
    if abs_tick & 131072 != 0 {
        ratio = (ratio * 26294789957452057) >> 64
    }
    if abs_tick & 262144 != 0 {
        ratio = (ratio * 37481735321082) >> 64
    }

    ratio
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sqrt_price_bounds_match_whirlpool() {
        assert_eq!(sqrt_price_from_tick_index(0), 1u128 << 64);
        assert_eq!(sqrt_price_from_tick_index(443636), MAX_SQRT_PRICE_X64);
        assert_eq!(sqrt_price_from_tick_index(-443636), MIN_SQRT_PRICE_X64);
    }
    #[test]
    fn ninety_ten_shape() {
        // 1 SOL at tick 0 with lower = -2432, upper = 443520 -> ~8.7 SOL worth of tokens
        let s = sqrt_price_from_tick_index(0);
        let lo = sqrt_price_from_tick_index(-2432);
        let hi = sqrt_price_from_tick_index(443520);
        let l = liquidity_from_token_b(1_000_000_000, lo, s).unwrap();
        let a = amount_delta_a(s, hi, l, true).unwrap();
        let b = amount_delta_b(lo, s, l, true).unwrap();
        assert!(b >= 999_999_990 && b <= 1_000_000_001, "b={b}");
        assert!(a > 8_600_000_000 && a < 8_900_000_000, "a={a}");
    }
    #[test]
    fn spacing_and_arrays() {
        assert_eq!(floor_to_spacing(-2355, 128), -2432);
        assert_eq!(floor_to_spacing(443636, 128), 443520);
        assert_eq!(tick_array_start(-100, 128), -11264);
        assert_eq!(tick_array_start(443520, 128), 439296);
    }
}
