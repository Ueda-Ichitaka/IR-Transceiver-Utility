//! Manchester (bi-phase) helpers for RC5 and RC6.
//!
//! Exists to turn a frame into a sequence of half-bit levels: every mark or
//! space run must be a whole number of half-bit times (within tolerance).
//! The final space of a frame is the gap and is left out.

use crate::pulse::Pulse;

/// Allowed deviation of a run from a whole number of half bits, in percent
/// of one half bit.
pub const UNIT_TOLERANCE_PERCENT: u32 = 35;

/// The frame as (is_mark, duration) runs, without the final gap.
pub fn runs(frame: &[Pulse]) -> Vec<(bool, u32)> {
    let last = frame.len().saturating_sub(1);
    let mut out = Vec::with_capacity(frame.len() * 2);
    for (i, p) in frame.iter().enumerate() {
        out.push((true, p.on_us));
        if i < last {
            out.push((false, p.off_us));
        }
    }
    out
}

/// Expands runs into half-bit levels (`true` = mark). `None` if a run is
/// not 1..=`max_units` half bits long.
pub fn halves(runs: &[(bool, u32)], half_us: u32, max_units: u32) -> Option<Vec<bool>> {
    let mut out = Vec::new();
    for &(level, duration) in runs {
        let units = (duration + half_us / 2) / half_us;
        if units == 0 || units > max_units {
            return None;
        }
        if duration.abs_diff(units * half_us) * 100 > half_us * UNIT_TOLERANCE_PERCENT {
            return None;
        }
        out.extend(std::iter::repeat_n(level, units as usize));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(on_us: u32, off_us: u32) -> Pulse {
        Pulse { on_us, off_us }
    }

    #[test]
    fn runs_leave_out_the_gap() {
        assert_eq!(runs(&[p(1, 2), p(3, 90_000)]), vec![(true, 1), (false, 2), (true, 3)]);
        assert!(runs(&[]).is_empty());
    }

    #[test]
    fn halves_expand_whole_units() {
        let r = [(true, 889), (false, 1778), (true, 900)];
        assert_eq!(halves(&r, 889, 2), Some(vec![true, false, false, true]));
    }

    #[test]
    fn halves_reject_partial_or_long_runs() {
        assert_eq!(halves(&[(true, 1300)], 889, 2), None, "1.46 units");
        assert_eq!(halves(&[(true, 2667)], 889, 2), None, "3 units");
        assert_eq!(halves(&[(true, 200)], 889, 2), None, "too short");
    }
}
