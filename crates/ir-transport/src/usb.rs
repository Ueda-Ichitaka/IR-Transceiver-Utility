//! `UsbTransport`: the nusb-backed `Transport` for the real dongle.
//!
//! Exists to do the actual USB work: find the 045c:0132, claim interface 0
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
pub const PRODUCT_ID: u16 = 0x0132;
pub const INTERFACE: u8 = 0;

/// Identifying details of the opened device.
#[derive(Debug, Clone)]
pub struct DeviceSummary {
    pub bus_id: String,
    pub address: u8,
    pub serial: Option<String>,
    pub product: Option<String>,
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
            OpenError::NotFound => write!(f, "no {VENDOR_ID:04x}:{PRODUCT_ID:04x} device found"),
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
}

impl UsbTransport {
    /// Opens the first 045c:0132 found.
    pub fn open() -> Result<Self, OpenError> {
        let usb_err = |e: nusb::Error| OpenError::Usb(e.to_string());
        let info = nusb::list_devices()
            .wait()
            .map_err(usb_err)?
            .find(|d| d.vendor_id() == VENDOR_ID && d.product_id() == PRODUCT_ID)
            .ok_or(OpenError::NotFound)?;
        let summary = DeviceSummary {
            bus_id: info.bus_id().to_string(),
            address: info.device_address(),
            serial: info.serial_number().map(str::to_string),
            product: info.product_string().map(str::to_string),
        };
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

        let out_ep = interface.endpoint::<Bulk, Out>(endpoints.out_addr).map_err(usb_err)?;
        let in_ep = interface.endpoint::<Bulk, In>(endpoints.in_addr).map_err(usb_err)?;
        let in_packet_size = in_ep.max_packet_size();
        Ok(Self { out_ep, in_ep, in_packet_size, _interface: interface, summary, endpoints })
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
