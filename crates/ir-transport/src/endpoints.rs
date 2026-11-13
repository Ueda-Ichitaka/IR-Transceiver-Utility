//! Chooses the bulk OUT and IN endpoints from interface descriptors.
//!
//! Exists so endpoint addresses are read from the device instead of being
//! hard-coded, with the same checks as `init_device_from_desc()` in
//! elksmart-comm.c. Works on a plain description so it is testable without USB.

/// USB class codes accepted for the dongle's interface.
pub const CLASS_COMM: u8 = 0x02;
pub const CLASS_VENDOR_SPEC: u8 = 0xFF;

/// Transfer type of an endpoint, as far as the checks need it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Bulk,
    Other,
}

/// Minimal endpoint description.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointInfo {
    pub address: u8,
    pub kind: Kind,
}

/// Chosen endpoint addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Endpoints {
    pub out_addr: u8,
    pub in_addr: u8,
}

/// Why an interface does not look like the dongle's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointError {
    UnexpectedClass(u8),
    UnexpectedEndpointCount(usize),
    NotBulk(u8),
    NotBidirectional,
}

/// Validates the interface and returns the OUT and IN endpoint addresses.
pub fn select(class: u8, endpoints: &[EndpointInfo]) -> Result<Endpoints, EndpointError> {
    if class != CLASS_COMM && class != CLASS_VENDOR_SPEC {
        return Err(EndpointError::UnexpectedClass(class));
    }
    if endpoints.len() != 2 {
        return Err(EndpointError::UnexpectedEndpointCount(endpoints.len()));
    }
    let (mut out_addr, mut in_addr) = (None, None);
    for ep in endpoints {
        if ep.kind != Kind::Bulk {
            return Err(EndpointError::NotBulk(ep.address));
        }
        if ep.address & 0x80 != 0 {
            in_addr = Some(ep.address);
        } else {
            out_addr = Some(ep.address);
        }
    }
    match (out_addr, in_addr) {
        (Some(out_addr), Some(in_addr)) => Ok(Endpoints { out_addr, in_addr }),
        _ => Err(EndpointError::NotBidirectional),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bulk(address: u8) -> EndpointInfo {
        EndpointInfo { address, kind: Kind::Bulk }
    }

    #[test]
    fn real_0132_descriptor_selects_02_and_82() {
        assert_eq!(
            select(0xFF, &[bulk(0x02), bulk(0x82)]),
            Ok(Endpoints { out_addr: 0x02, in_addr: 0x82 })
        );
    }

    #[test]
    fn order_of_endpoints_does_not_matter() {
        assert_eq!(
            select(0x02, &[bulk(0x81), bulk(0x01)]),
            Ok(Endpoints { out_addr: 0x01, in_addr: 0x81 })
        );
    }

    #[test]
    fn other_class_is_rejected() {
        assert_eq!(select(0x03, &[bulk(0x02), bulk(0x82)]), Err(EndpointError::UnexpectedClass(0x03)));
    }

    #[test]
    fn endpoint_count_must_be_two() {
        assert_eq!(select(0xFF, &[bulk(0x02)]), Err(EndpointError::UnexpectedEndpointCount(1)));
    }

    #[test]
    fn non_bulk_endpoint_is_rejected() {
        let int_ep = EndpointInfo { address: 0x83, kind: Kind::Other };
        assert_eq!(select(0xFF, &[bulk(0x02), int_ep]), Err(EndpointError::NotBulk(0x83)));
    }

    #[test]
    fn two_out_endpoints_are_not_bidirectional() {
        assert_eq!(select(0xFF, &[bulk(0x01), bulk(0x02)]), Err(EndpointError::NotBidirectional));
    }
}
