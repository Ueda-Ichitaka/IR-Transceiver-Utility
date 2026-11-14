//! Timing comparison shared by the protocol decoders.
//!
//! Exists so every decoder accepts the same deviation from nominal timings.
//! The 0132 measures marks about 50 us long and spaces about 50 us short, so
//! a relative tolerance is used rather than a fixed one.

/// Allowed deviation from nominal timings, in percent.
pub const TOLERANCE_PERCENT: u32 = 30;

/// True when `value` is within [`TOLERANCE_PERCENT`] of `nominal`.
pub fn near(value: u32, nominal: u32) -> bool {
    let (value, nominal) = (u64::from(value) * 100, u64::from(nominal));
    value >= nominal * u64::from(100 - TOLERANCE_PERCENT) && value <= nominal * u64::from(100 + TOLERANCE_PERCENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_thirty_percent_either_way() {
        assert!(near(560, 560));
        assert!(near(728, 560));
        assert!(near(392, 560));
        assert!(!near(729, 560));
        assert!(!near(391, 560));
        assert!(!near(0, 560));
    }
}
