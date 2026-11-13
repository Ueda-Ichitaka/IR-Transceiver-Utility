//! Memory budget for captured data.
//!
//! Exists so continuous capture can never exhaust RAM on a shared machine:
//! the budget is the smaller of a fixed cap and a share of the memory that is
//! available when capture starts (`MemAvailable` in /proc/meminfo).

/// Upper bound for captured data.
pub const MAX_BUDGET_BYTES: u64 = 64 * 1024 * 1024;
/// Share of available memory a capture may use, in percent.
pub const AVAILABLE_SHARE_PERCENT: u64 = 5;

/// Extracts `MemAvailable` (in bytes) from the text of /proc/meminfo.
pub fn parse_mem_available(meminfo: &str) -> Option<u64> {
    let line = meminfo.lines().find_map(|l| l.strip_prefix("MemAvailable:"))?;
    let kib: u64 = line.trim().strip_suffix("kB")?.trim().parse().ok()?;
    kib.checked_mul(1024)
}

/// Budget for a capture given the available memory in bytes.
pub fn capture_budget(mem_available: u64) -> u64 {
    (mem_available / 100 * AVAILABLE_SHARE_PERCENT).min(MAX_BUDGET_BYTES)
}

/// Reads /proc/meminfo and returns the capture budget. Falls back to the
/// smallest sensible budget (1 MiB) when the file cannot be read.
pub fn system_capture_budget() -> u64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| parse_mem_available(&text))
        .map_or(1024 * 1024, capture_budget)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEMINFO: &str = "MemTotal:       32000000 kB\nMemFree:         1000000 kB\nMemAvailable:   20000000 kB\nBuffers:            1234 kB\n";

    #[test]
    fn parses_mem_available_in_bytes() {
        assert_eq!(parse_mem_available(MEMINFO), Some(20_000_000 * 1024));
    }

    #[test]
    fn missing_or_malformed_field_is_none() {
        assert_eq!(parse_mem_available("MemTotal: 1 kB\n"), None);
        assert_eq!(parse_mem_available("MemAvailable: lots kB\n"), None);
        assert_eq!(parse_mem_available(""), None);
    }

    #[test]
    fn budget_is_capped_at_64_mib() {
        assert_eq!(capture_budget(20_000_000 * 1024), MAX_BUDGET_BYTES);
    }

    #[test]
    fn budget_is_five_percent_when_memory_is_low() {
        assert_eq!(capture_budget(100 * 1024 * 1024), 5 * 1024 * 1024);
    }
}
