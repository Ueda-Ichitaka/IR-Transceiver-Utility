//! Memory limits for captured data.
//!
//! Exists so continuous capture is bounded twice: by a configurable limit for
//! the captured data itself (default 4096 MiB), and by a guard that stops
//! capture when the machine runs low on memory, so a large limit cannot
//! exhaust RAM on a shared machine. Values come from /proc/meminfo.

/// Default limit for captured data.
pub const DEFAULT_CAPTURE_LIMIT_BYTES: u64 = 4096 * 1024 * 1024;
/// Share of total memory that must stay available, in percent.
pub const RESERVE_PERCENT: u64 = 10;
/// Lower bound of the reserve.
pub const MIN_RESERVE_BYTES: u64 = 256 * 1024 * 1024;

/// Memory figures from /proc/meminfo, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemInfo {
    pub total: u64,
    pub available: u64,
}

/// Parses `MemTotal` and `MemAvailable` from the text of /proc/meminfo.
pub fn parse_meminfo(meminfo: &str) -> Option<MemInfo> {
    let field = |name: &str| -> Option<u64> {
        let rest = meminfo.lines().find_map(|l| l.strip_prefix(name))?;
        let kib: u64 = rest.trim().strip_suffix("kB")?.trim().parse().ok()?;
        kib.checked_mul(1024)
    };
    Some(MemInfo { total: field("MemTotal:")?, available: field("MemAvailable:")? })
}

/// Memory that must stay available while capturing.
pub fn reserve(total: u64) -> u64 {
    (total.saturating_mul(RESERVE_PERCENT) / 100).max(MIN_RESERVE_BYTES)
}

/// True while capturing may continue.
pub fn memory_ok(info: MemInfo) -> bool {
    info.available >= reserve(info.total)
}

/// Reads /proc/meminfo. `None` when it cannot be read or parsed.
pub fn read_meminfo() -> Option<MemInfo> {
    parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;
    const MEMINFO: &str = "MemTotal:       32000000 kB\nMemFree:         1000000 kB\nMemAvailable:   20000000 kB\nBuffers:            1234 kB\n";

    #[test]
    fn parses_total_and_available_in_bytes() {
        assert_eq!(
            parse_meminfo(MEMINFO),
            Some(MemInfo { total: 32_000_000 * 1024, available: 20_000_000 * 1024 })
        );
    }

    #[test]
    fn missing_or_malformed_fields_are_none() {
        assert_eq!(parse_meminfo("MemTotal: 1 kB\n"), None);
        assert_eq!(parse_meminfo("MemTotal: 1 kB\nMemAvailable: lots kB\n"), None);
        assert_eq!(parse_meminfo(""), None);
    }

    #[test]
    fn default_limit_is_4096_mib() {
        assert_eq!(DEFAULT_CAPTURE_LIMIT_BYTES, 4 * GIB);
    }

    #[test]
    fn reserve_is_ten_percent_with_floor() {
        assert_eq!(reserve(32 * GIB), 32 * GIB / 10);
        assert_eq!(reserve(GIB), MIN_RESERVE_BYTES);
    }

    #[test]
    fn memory_ok_compares_available_with_reserve() {
        assert!(memory_ok(MemInfo { total: 30 * GIB, available: 20 * GIB }));
        assert!(!memory_ok(MemInfo { total: 30 * GIB, available: 2 * GIB }));
    }
}
