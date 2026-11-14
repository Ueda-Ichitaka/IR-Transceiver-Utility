//! `UsbTransport`: the nusb-backed `Transport` for the real dongle.
//!
//! Exists to do the actual USB work: find supported dongles (vendor 045c,
//! products 0132, 0195, 0184, 0134), claim interface 0
//! (detaching a kernel driver if one is bound), pick the bulk endpoints from
//! the interface descriptor and run blocking bulk transfers with timeouts.
//! Only exercised against hardware (elk-probe), not by unit tests.

use std::fmt;
use std::time::Duration;

use nusb::MaybeFuture;
use nusb::descriptors::TransferType;
use nusb::transfer::{Buffer, Bulk, In, Out, TransferError};

use crate::endpoints::{self, EndpointError, EndpointInfo, Endpoints, Kind};
use crate::transport::{Transport, TransportError};

pub const VENDOR_ID: u16 = 0x045C;
/// Supported product ids: 0132, EKX4S (0195), EKX5S-T (0184), EKX5S (0134).
pub const PRODUCT_IDS: [u16; 4] = [0x0132, 0x0195, 0x0184, 0x0134];
pub const INTERFACE: u8 = 0;

/// True for a supported ELK Smart dongle.
pub fn is_supported(vendor_id: u16, product_id: u16) -> bool {
    vendor_id == VENDOR_ID && PRODUCT_IDS.contains(&product_id)
}

/// Identifying details of a dongle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSummary {
    pub product_id: u16,
    pub bus_id: String,
    pub address: u8,
    pub serial: Option<String>,
    pub product: Option<String>,
    /// Sysfs directory of the device (Linux), e.g. `/sys/bus/usb/devices/3-2`.
    pub sysfs_path: Option<std::path::PathBuf>,
}

/// Why the dongle could not be opened.
#[derive(Debug)]
pub enum OpenError {
    NotFound,
    Usb(String),
    NoInterfaceDescriptor,
    Descriptor(EndpointError),
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpenError::NotFound => write!(f, "no supported ELK Smart dongle found"),
            OpenError::Usb(msg) => write!(f, "USB error: {msg}"),
            OpenError::NoInterfaceDescriptor => write!(f, "interface {INTERFACE} has no descriptor"),
            OpenError::Descriptor(e) => write!(f, "unexpected interface layout: {e:?}"),
        }
    }
}

impl std::error::Error for OpenError {}

/// An opened and claimed dongle.
pub struct UsbTransport {
    out_ep: nusb::Endpoint<Bulk, Out>,
    in_ep: nusb::Endpoint<Bulk, In>,
    in_packet_size: usize,
    _interface: nusb::Interface,
    pub summary: DeviceSummary,
    pub endpoints: Endpoints,
    /// Class and subclass of interface 0.
    pub interface_class: u8,
    pub interface_subclass: u8,
}

fn usb_err(e: nusb::Error) -> OpenError {
    OpenError::Usb(e.to_string())
}

fn summary(info: &nusb::DeviceInfo) -> DeviceSummary {
    DeviceSummary {
        product_id: info.product_id(),
        bus_id: info.bus_id().to_string(),
        address: info.device_address(),
        serial: info.serial_number().map(str::to_string),
        product: info.product_string().map(str::to_string),
        sysfs_path: Some(info.sysfs_path().to_path_buf()),
    }
}

fn supported_devices() -> Result<Vec<nusb::DeviceInfo>, OpenError> {
    Ok(nusb::list_devices()
        .wait()
        .map_err(usb_err)?
        .filter(|d| is_supported(d.vendor_id(), d.product_id()))
        .collect())
}

/// Lists all connected supported dongles, in enumeration order.
pub fn list() -> Result<Vec<DeviceSummary>, OpenError> {
    Ok(supported_devices()?.iter().map(summary).collect())
}

impl UsbTransport {
    /// Maximum packet size of the IN endpoint.
    pub fn packet_size(&self) -> usize {
        self.in_packet_size
    }

    /// Opens the first supported dongle found.
    pub fn open() -> Result<Self, OpenError> {
        Self::open_where(|_| true)
    }

    /// Opens the dongle at the given bus and address (from [`list`]).
    pub fn open_at(bus_id: &str, address: u8) -> Result<Self, OpenError> {
        Self::open_where(|s| s.bus_id == bus_id && s.address == address)
    }

    fn open_where(select: impl Fn(&DeviceSummary) -> bool) -> Result<Self, OpenError> {
        let info = supported_devices()?
            .into_iter()
            .find(|d| select(&summary(d)))
            .ok_or(OpenError::NotFound)?;
        let summary = summary(&info);
        let device = info.open().wait().map_err(usb_err)?;
        let interface = device.detach_and_claim_interface(INTERFACE).wait().map_err(usb_err)?;

        let desc = interface.descriptor().ok_or(OpenError::NoInterfaceDescriptor)?;
        let eps: Vec<EndpointInfo> = desc
            .endpoints()
            .map(|e| EndpointInfo {
                address: e.address(),
                kind: if e.transfer_type() == TransferType::Bulk { Kind::Bulk } else { Kind::Other },
            })
            .collect();
        let endpoints = endpoints::select(desc.class(), &eps).map_err(OpenError::Descriptor)?;
        let (interface_class, interface_subclass) = (desc.class(), desc.subclass());

        let out_ep = interface.endpoint::<Bulk, Out>(endpoints.out_addr).map_err(usb_err)?;
        let in_ep = interface.endpoint::<Bulk, In>(endpoints.in_addr).map_err(usb_err)?;
        let in_packet_size = in_ep.max_packet_size();
        Ok(Self {
            out_ep,
            in_ep,
            in_packet_size,
            _interface: interface,
            summary,
            endpoints,
            interface_class,
            interface_subclass,
        })
    }
}

fn map_transfer_error(e: TransferError) -> TransportError {
    match e {
        TransferError::Cancelled => TransportError::Timeout,
        TransferError::Disconnected => TransportError::Disconnected,
        other => TransportError::Usb(format!("{other:?}")),
    }
}

impl Transport for UsbTransport {
    fn write(&mut self, data: &[u8], timeout: Duration) -> Result<(), TransportError> {
        let completion = self.out_ep.transfer_blocking(data.to_vec().into(), timeout);
        completion.status.map_err(map_transfer_error)
    }

    fn read(&mut self, timeout: Duration) -> Result<Vec<u8>, TransportError> {
        let completion = self.in_ep.transfer_blocking(Buffer::new(self.in_packet_size), timeout);
        let len = completion.actual_len;
        match completion.status {
            Ok(()) => {}
            // A transfer cancelled on timeout may still have received data.
            Err(TransferError::Cancelled) if len > 0 => {}
            Err(e) => return Err(map_transfer_error(e)),
        }
        let mut bytes = completion.buffer.into_vec();
        bytes.truncate(len);
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_ids() {
        for pid in [0x0132, 0x0195, 0x0184, 0x0134] {
            assert!(is_supported(0x045C, pid), "{pid:04x}");
        }
        assert!(!is_supported(0x045C, 0x02AA));
        assert!(!is_supported(0x1234, 0x0132));
    }
}
