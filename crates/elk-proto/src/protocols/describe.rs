//! Human-readable text for decoded frames and whole learned signals.
//!
//! Exists so the GUI and tools can label a capture in one line, for example
//! `NEC addr 0x00 cmd 0x59, NEC repeat x5`: the signal is split into frames,
//! each frame is decoded, and runs of identical frames are counted.

use std::fmt;

use super::{Decoded, decode};
use crate::frames;
use crate::nec::NecFrame;
use crate::pulse::Pulse;

impl fmt::Display for Decoded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Decoded::Nec(NecFrame::Command { address, command, extended: false }) => {
                write!(f, "NEC addr 0x{address:02X} cmd 0x{command:02X}")
            }
            Decoded::Nec(NecFrame::Command { address, command, extended: true }) => {
                write!(f, "NEC-ext addr 0x{address:04X} cmd 0x{command:02X}")
            }
            Decoded::Nec(NecFrame::Repeat) => write!(f, "NEC repeat"),
            Decoded::Rc5 { address, command, toggle } => {
                let name = if command & 0x40 != 0 { "RC5X" } else { "RC5" };
                write!(f, "{name} addr 0x{address:02X} cmd 0x{command:02X} toggle {}", u8::from(toggle))
            }
            Decoded::Rc6 { mode, toggle, bits, data } => {
                let digits = usize::from(bits).div_ceil(4);
                write!(f, "RC6 mode {mode} data 0x{data:0digits$X} ({bits} bits) toggle {}", u8::from(toggle))
            }
            Decoded::Sirc { bits, command, address, extended } => {
                write!(f, "SIRC{bits} addr 0x{address:02X} cmd 0x{command:02X}")?;
                if bits == 20 {
                    write!(f, " ext 0x{extended:02X}")?;
                }
                Ok(())
            }
            Decoded::Samsung32 { address, command } => write!(f, "Samsung32 addr 0x{address:02X} cmd 0x{command:02X}"),
            Decoded::Kaseikyo { vendor, genre1, genre2, data, id } => write!(
                f,
                "Kaseikyo vendor 0x{vendor:04X} genre 0x{genre1:X}/0x{genre2:X} data 0x{data:03X} id {id}"
            ),
        }
    }
}

/// One line describing every frame of a signal.
pub fn describe_pulses(pulses: &[Pulse]) -> String {
    let mut runs: Vec<(Option<Decoded>, usize)> = Vec::new();
    for frame in frames::split(pulses, frames::DEFAULT_GAP_US) {
        let decoded = decode(frame);
        match runs.last_mut() {
            Some((last, count)) if *last == decoded => *count += 1,
            _ => runs.push((decoded, 1)),
        }
    }
    if runs.is_empty() {
        return "no signal".into();
    }
    let parts: Vec<String> = runs
        .into_iter()
        .map(|(decoded, count)| {
            let name = decoded.map_or_else(|| "unknown".to_string(), |d| d.to_string());
            if count > 1 { format!("{name} x{count}") } else { name }
        })
        .collect();
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocols::encode::encode;

    fn text(d: Decoded) -> String {
        d.to_string()
    }

    #[test]
    fn nec_frames() {
        let nec = |address, command, extended| Decoded::Nec(NecFrame::Command { address, command, extended });
        assert_eq!(text(nec(0x00, 0x59, false)), "NEC addr 0x00 cmd 0x59");
        assert_eq!(text(nec(0xEF00, 0x03, true)), "NEC-ext addr 0xEF00 cmd 0x03");
        assert_eq!(text(Decoded::Nec(NecFrame::Repeat)), "NEC repeat");
    }

    #[test]
    fn other_protocols() {
        assert_eq!(text(Decoded::Rc5 { address: 0x05, command: 0x35, toggle: true }), "RC5 addr 0x05 cmd 0x35 toggle 1");
        assert_eq!(text(Decoded::Rc5 { address: 0x05, command: 0x75, toggle: false }), "RC5X addr 0x05 cmd 0x75 toggle 0");
        assert_eq!(
            text(Decoded::Rc6 { mode: 0, toggle: false, bits: 16, data: 0x0C3D }),
            "RC6 mode 0 data 0x0C3D (16 bits) toggle 0"
        );
        assert_eq!(
            text(Decoded::Rc6 { mode: 6, toggle: true, bits: 32, data: 0x800F_0410 }),
            "RC6 mode 6 data 0x800F0410 (32 bits) toggle 1"
        );
        assert_eq!(text(Decoded::Sirc { bits: 12, command: 0x15, address: 0x01, extended: 0 }), "SIRC12 addr 0x01 cmd 0x15");
        assert_eq!(text(Decoded::Sirc { bits: 15, command: 0x15, address: 0x97, extended: 0 }), "SIRC15 addr 0x97 cmd 0x15");
        assert_eq!(
            text(Decoded::Sirc { bits: 20, command: 0x15, address: 0x1A, extended: 0x3A }),
            "SIRC20 addr 0x1A cmd 0x15 ext 0x3A"
        );
        assert_eq!(text(Decoded::Samsung32 { address: 0x07, command: 0x02 }), "Samsung32 addr 0x07 cmd 0x02");
        assert_eq!(
            text(Decoded::Kaseikyo { vendor: 0x2002, genre1: 0x1, genre2: 0x0, data: 0x0123, id: 0 }),
            "Kaseikyo vendor 0x2002 genre 0x1/0x0 data 0x123 id 0"
        );
    }

    #[test]
    fn signal_with_repeats_is_counted() {
        let cmd = Decoded::Nec(NecFrame::Command { address: 0x00, command: 0x59, extended: false });
        let mut pulses = encode(&cmd);
        for _ in 0..5 {
            pulses.extend(encode(&Decoded::Nec(NecFrame::Repeat)));
        }
        assert_eq!(describe_pulses(&pulses), "NEC addr 0x00 cmd 0x59, NEC repeat x5");
    }

    #[test]
    fn identical_full_frames_are_counted_too() {
        let frame = Decoded::Samsung32 { address: 0x07, command: 0x02 };
        let pulses = [encode(&frame), encode(&frame)].concat();
        assert_eq!(describe_pulses(&pulses), "Samsung32 addr 0x07 cmd 0x02 x2");
    }

    #[test]
    fn unknown_frames_and_empty_signals() {
        let junk = vec![Pulse { on_us: 300, off_us: 300 }, Pulse { on_us: 300, off_us: 20_000 }];
        let nec = encode(&Decoded::Nec(NecFrame::Command { address: 1, command: 2, extended: false }));
        assert_eq!(describe_pulses(&[junk.clone(), nec].concat()), "unknown, NEC addr 0x01 cmd 0x02");
        assert_eq!(describe_pulses(&[junk.clone(), junk].concat()), "unknown x2");
        assert_eq!(describe_pulses(&[]), "no signal");
    }
}
