//! Host-side (not guest-side) available-memory gate. Growing the virtio-mem
//! device consumes RHEL host RAM; a grow request must never be sent unless
//! the host itself has enough free memory left over after the reserved
//! headroom, mirroring `scripts/live-resize-test.sh`'s `--host-reserve-bytes`
//! safety check.

use std::fs;

pub trait HostMemorySource {
    fn available_bytes(&self) -> Result<u64, String>;
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
        return Ok(value_kib.saturating_mul(1024));
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
}
