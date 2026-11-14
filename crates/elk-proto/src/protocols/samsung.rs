//! Samsung32.
//!
//! Leader 4500/4500 us, 32 bits LSB first as 550 us mark and 550 (0) or
//! 1650 (1) us space, stop mark; bytes are address, address, command,
//! !command (Flipper Zero firmware decoder).

use super::Decoded;
use crate::pulse::Pulse;

pub fn decode(frame: &[Pulse]) -> Option<Decoded> {
    let [a0, a1, command, command_inv] = super::distance_bytes::<4>(frame, (4500, 4500), 550, 550, 1650)?;
    (a0 == a1 && command == !command_inv).then_some(Decoded::Samsung32 { address: a0, command })
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    #[test]
    fn decodes_address_and_command() {
        assert_eq!(decode(&samsung_frame(0x07, 0x02)), Some(Decoded::Samsung32 { address: 0x07, command: 0x02 }));
    }

    #[test]
    fn rejects_bad_checks_and_other_protocols() {
        let bad_command = distance_frame((4500, 4500), 550, 550, 1650, &[7, 7, 2, 2]);
        assert_eq!(decode(&bad_command), None);
        let different_address = distance_frame((4500, 4500), 550, 550, 1650, &[7, 8, 2, !2]);
        assert_eq!(decode(&different_address), None);
        assert_eq!(decode(&crate::nec::encode_command(7, 2, false)), None);
    }
}
