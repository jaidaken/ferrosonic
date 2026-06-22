//! Saturating numeric conversions for narrowing casts.
//!
//! Each helper replaces an `as` cast that clippy flags as truncating or
//! sign-losing. They saturate at the target maximum instead of wrapping,
//! so an out-of-range value can never silently corrupt downstream state.
//! Inputs are expected non-negative (all call sites pass unsigned counts
//! or dimensions); a negative input would saturate to the maximum, so do
//! not use these for values that can legitimately be negative.

/// Convert to `u16`, saturating at `u16::MAX` on overflow.
pub fn u16_sat<T: TryInto<u16>>(x: T) -> u16 {
    x.try_into().unwrap_or(u16::MAX)
}

/// Convert to `u32`, saturating at `u32::MAX` on overflow.
pub fn u32_sat<T: TryInto<u32>>(x: T) -> u32 {
    x.try_into().unwrap_or(u32::MAX)
}

/// Convert to `usize`, saturating at `usize::MAX` on overflow.
pub fn usize_sat<T: TryInto<usize>>(x: T) -> usize {
    x.try_into().unwrap_or(usize::MAX)
}

/// Convert to `i32`, saturating at `i32::MAX` on overflow.
pub fn i32_sat<T: TryInto<i32>>(x: T) -> i32 {
    x.try_into().unwrap_or(i32::MAX)
}

/// Convert to `u8`, saturating at `u8::MAX` on overflow.
pub fn u8_sat<T: TryInto<u8>>(x: T) -> u8 {
    x.try_into().unwrap_or(u8::MAX)
}
