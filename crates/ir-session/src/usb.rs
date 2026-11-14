//! `UsbConnector`: connects the session to the first supported dongle.
//!
//! Exists to adapt `ir_transport::usb` to the session's `Connector` trait and
//! to describe the opened device for the GUI: one line, a short location and
//! labelled facts (bus, interface, endpoints).

use ir_transport::endpoints::Endpoints;
use ir_transport::usb::{DeviceSummary, INTERFACE, UsbTransport, VENDOR_ID};

use crate::session::{Connector, DeviceInfo};

/// Opens the first supported dongle found (design decision: several dongles
/// are supported, the first one is selected automatically).
#[derive(Debug, Default)]
pub struct UsbConnector;

impl Connector for UsbConnector {
    type Transport = UsbTransport;

    fn connect(&mut self) -> Result<(UsbTransport, DeviceInfo), String> {
        let transport = UsbTransport::open().map_err(|e| e.to_string())?;
        let interface = InterfaceSummary {
            class: transport.interface_class,
            subclass: transport.interface_subclass,
            packet_size: transport.packet_size(),
        };
        let info = device_info(&transport.summary, &transport.endpoints, &interface);
        Ok((transport, info))
    }
}

/// What the GUI shows about the dongle's interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterfaceSummary {
    pub class: u8,
    pub subclass: u8,
    pub packet_size: usize,
}

/// Description, short location and labelled facts of an opened dongle.
pub fn device_info(s: &DeviceSummary, endpoints: &Endpoints, interface: &InterfaceSummary) -> DeviceInfo {
    let id = format!("{VENDOR_ID:04x}:{:04x}", s.product_id);
    let name = s.product.as_deref().unwrap_or("ELK Smart dongle");
    let mut facts = vec![
        ("Device".to_string(), format!("{id} {name}")),
        ("Bus/addr".to_string(), format!("{}/{:03}", s.bus_id, s.address)),
    ];
    if let Some(serial) = &s.serial {
        facts.push(("Serial".to_string(), serial.clone()));
    }
    facts.push((
        "Interface".to_string(),
        format!("{INTERFACE} · class 0x{:02X} / sub 0x{:02X}", interface.class, interface.subclass),
    ));
    facts.push((
        "Endpoints".to_string(),
        format!(
            "OUT 0x{:02X} bulk · IN 0x{:02X} bulk · {} B",
            endpoints.out_addr, endpoints.in_addr, interface.packet_size
        ),
    ));
    DeviceInfo { description: describe(s), location: format!("{id} · bus {} dev {:03}", s.bus_id, s.address), facts }
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
    fn device_info_has_location_and_facts() {
        let iface = InterfaceSummary { class: 0xFF, subclass: 0xF0, packet_size: 64 };
        let eps = Endpoints { out_addr: 0x02, in_addr: 0x82 };
        let info = device_info(&summary(Some("Smart IR Blaster"), Some("030820250001")), &eps, &iface);
        assert_eq!(info.description, "Smart IR Blaster (045c:0132, bus 003, address 14, serial 030820250001)");
        assert_eq!(info.location, "045c:0132 · bus 003 dev 014");
        let facts: Vec<(&str, &str)> = info.facts.iter().map(|(l, v)| (l.as_str(), v.as_str())).collect();
        assert_eq!(
            facts,
            vec![
                ("Device", "045c:0132 Smart IR Blaster"),
                ("Bus/addr", "003/014"),
                ("Serial", "030820250001"),
                ("Interface", "0 · class 0xFF / sub 0xF0"),
                ("Endpoints", "OUT 0x02 bulk · IN 0x82 bulk · 64 B"),
            ]
        );
        let bare = device_info(&summary(None, None), &eps, &iface);
        assert_eq!(bare.facts[0].1, "045c:0132 ELK Smart dongle");
        assert!(bare.facts.iter().all(|(l, _)| l != "Serial"));
    }

    #[test]
    fn missing_strings_are_left_out() {
        assert_eq!(describe(&summary(None, None)), "ELK Smart dongle (045c:0132, bus 003, address 14)");
    }
}
