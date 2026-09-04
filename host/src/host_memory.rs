//! Host-side (not guest-side) available-memory gate. Growing the virtio-mem
//! device consumes RHEL host RAM; a grow request must never be sent unless
//! the host itself has enough free memory left over after the reserved
//! headroom, mirroring `scripts/live-resize-test.sh`'s `--host-reserve-bytes`
//! safety check.

use std::fs;

pub trait HostMemorySource {
    fn available_bytes(&self) -> Result<u64, String>;
}

pub(crate) fn validate_grow_headroom(
    current_bytes: u64,
    target_bytes: u64,
    available_bytes: u64,
    minimum_headroom_bytes: u64,
) -> Result<(), String> {
    if target_bytes <= current_bytes {
        return Ok(());
    }
    let grow_bytes = target_bytes - current_bytes;
    if grow_bytes > available_bytes || available_bytes - grow_bytes < minimum_headroom_bytes {
        return Err(format!(
            "host headroom is insufficient: available={available_bytes} grow={grow_bytes} minimum_headroom={minimum_headroom_bytes}"
        ));
    }
    Ok(())
}

pub struct ProcMeminfoSource {
    path: String,
}

impl ProcMeminfoSource {
    pub fn new() -> Self {
        Self {
            path: "/proc/meminfo".to_owned(),
        }
    }
}

impl Default for ProcMeminfoSource {
    fn default() -> Self {
        Self::new()
    }
}

impl HostMemorySource for ProcMeminfoSource {
    fn available_bytes(&self) -> Result<u64, String> {
        let contents = fs::read_to_string(&self.path)
            .map_err(|error| format!("failed to read {}: {error}", self.path))?;
        parse_mem_available(&contents)
    }
}

fn parse_mem_available(contents: &str) -> Result<u64, String> {
    for line in contents.lines() {
        let Some(rest) = line.strip_prefix("MemAvailable:") else {
            continue;
        };
        let value_kib: u64 = rest
            .trim()
            .strip_suffix("kB")
            .unwrap_or(rest.trim())
            .trim()
            .parse()
            .map_err(|_| format!("MemAvailable line is not a decimal kB value: {line}"))?;
        return value_kib
            .checked_mul(1024)
            .ok_or_else(|| format!("MemAvailable value overflows canonical bytes: {line}"));
    }
    Err("MemAvailable field was not found".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mem_available_in_kib() {
        let contents = "MemTotal:       16777216 kB\nMemAvailable:    8388608 kB\n";
        assert_eq!(
            parse_mem_available(contents).expect("valid"),
            8388608 * 1024
        );
    }

    #[test]
    fn rejects_missing_field() {
        assert!(parse_mem_available("MemTotal: 100 kB\n").is_err());
    }

    #[test]
    fn rejects_kibibyte_overflow() {
        assert!(parse_mem_available("MemAvailable: 18446744073709551615 kB\n").is_err());
    }

    #[test]
    fn grow_headroom_accepts_exact_reserve_and_shrinks() {
        assert_eq!(validate_grow_headroom(4, 6, 5, 3), Ok(()));
        assert_eq!(validate_grow_headroom(6, 4, 0, 3), Ok(()));
        assert_eq!(validate_grow_headroom(4, 4, 0, 3), Ok(()));
    }

    #[test]
    fn grow_headroom_rejects_reserve_shortfall_and_growth_beyond_available() {
        assert!(validate_grow_headroom(4, 6, 4, 3).is_err());
        assert!(validate_grow_headroom(4, 10, 5, 0).is_err());
    }
}
