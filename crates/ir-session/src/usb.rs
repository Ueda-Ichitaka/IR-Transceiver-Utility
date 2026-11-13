//! `UsbConnector`: connects the session to the first supported dongle.
//!
//! Exists to adapt `ir_transport::usb` to the session's `Connector` trait and
//! to describe the opened device in one line for the GUI.

use ir_transport::usb::{DeviceSummary, UsbTransport, VENDOR_ID};

use crate::session::Connector;

/// Opens the first supported dongle found (design decision: several dongles
/// are supported, the first one is selected automatically).
#[derive(Debug, Default)]
pub struct UsbConnector;

impl Connector for UsbConnector {
    type Transport = UsbTransport;

    fn connect(&mut self) -> Result<(UsbTransport, String), String> {
        let transport = UsbTransport::open().map_err(|e| e.to_string())?;
        let description = describe(&transport.summary);
        Ok((transport, description))
    }
}

/// One-line description, e.g.
/// `Smart IR Blaster (045c:0132, bus 003, address 14, serial 0308...)`.
pub fn describe(s: &DeviceSummary) -> String {
    let name = s.product.as_deref().unwrap_or("ELK Smart dongle");
    let serial = s.serial.as_deref().map(|n| format!(", serial {n}")).unwrap_or_default();
    format!(
        "{name} ({VENDOR_ID:04x}:{:04x}, bus {}, address {}{serial})",
        s.product_id, s.bus_id, s.address
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(product: Option<&str>, serial: Option<&str>) -> DeviceSummary {
        DeviceSummary {
            product_id: 0x0132,
            bus_id: "003".into(),
            address: 14,
            serial: serial.map(Into::into),
            product: product.map(Into::into),
        }
    }

    #[test]
    fn describes_full_summary() {
        assert_eq!(
            describe(&summary(Some("Smart IR Blaster"), Some("030820250001"))),
            "Smart IR Blaster (045c:0132, bus 003, address 14, serial 030820250001)"
        );
    }

    #[test]
    fn missing_strings_are_left_out() {
        assert_eq!(describe(&summary(None, None)), "ELK Smart dongle (045c:0132, bus 003, address 14)");
    }
}
