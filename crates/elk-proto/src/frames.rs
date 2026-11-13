//! Splits a pulse train into frames at long gaps.
//!
//! Exists because one learn captures everything while a button is held: the
//! first frame plus repeat frames (on the 0132, NEC repeats every ~108 ms).
//! Splitting lets the app show, decode and trim them separately.

use crate::pulse::Pulse;

/// Default gap that ends a frame. Longer than any space inside common IR
/// frames (NEC leader space is 4.5 ms), shorter than inter-frame gaps.
pub const DEFAULT_GAP_US: u32 = 10_000;

/// Splits after every pulse whose off-time is at least `gap_us`. Each frame
/// keeps its final pulse, including that pulse's gap.
pub fn split(pulses: &[Pulse], gap_us: u32) -> Vec<&[Pulse]> {
    let mut frames = Vec::new();
    let mut start = 0;
    for (i, pulse) in pulses.iter().enumerate() {
        if pulse.off_us >= gap_us {
            frames.push(&pulses[start..=i]);
            start = i + 1;
        }
    }
    if start < pulses.len() {
        frames.push(&pulses[start..]);
    }
    frames
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(on_us: u32, off_us: u32) -> Pulse {
        Pulse { on_us, off_us }
    }

    #[test]
    fn splits_after_long_gaps() {
        let pulses = [p(1, 1), p(1, 20_000), p(2, 2), p(2, 30_000), p(3, 0)];
        let frames = split(&pulses, DEFAULT_GAP_US);
        assert_eq!(frames, vec![&pulses[0..2], &pulses[2..4], &pulses[4..5]]);
    }

    #[test]
    fn gap_equal_to_threshold_splits() {
        let pulses = [p(1, 10_000), p(1, 1)];
        assert_eq!(split(&pulses, 10_000).len(), 2);
    }

    #[test]
    fn no_gap_is_one_frame() {
        let pulses = [p(1, 1), p(1, 1)];
        assert_eq!(split(&pulses, DEFAULT_GAP_US), vec![&pulses[..]]);
    }

    #[test]
    fn trailing_gap_does_not_create_empty_frame() {
        let pulses = [p(1, 1), p(1, 50_000)];
        assert_eq!(split(&pulses, DEFAULT_GAP_US), vec![&pulses[..]]);
    }

    #[test]
    fn empty_input_has_no_frames() {
        assert!(split(&[], DEFAULT_GAP_US).is_empty());
    }
}
