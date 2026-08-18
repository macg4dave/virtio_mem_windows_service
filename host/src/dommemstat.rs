//! Alternative memory-stat source using the virtio-balloon-backed
//! `virsh dommemstat` counters instead of the QEMU Guest Agent
//! `guest-get-memory-stats` command, which the connected guest agent does
//! not implement (see docs/issues.md ISSUE-001).

use virtio_mem_core::MemoryStats;

use crate::runtime::GuestStatsSource;
use crate::virsh::VirshCommand;

const KIB: u64 = 1024;

pub struct DomMemStatSource<C> {
    command: C,
    vm_name: String,
}

impl<C> DomMemStatSource<C> {
    pub fn new(command: C, vm_name: impl Into<String>) -> Self {
        Self {
            command,
            vm_name: vm_name.into(),
        }
    }
}

impl<C: VirshCommand> GuestStatsSource for DomMemStatSource<C> {
    fn get_memory_stats(&self) -> Result<MemoryStats, String> {
        let output = self
            .command
            .run(&["dommemstat".to_owned(), self.vm_name.clone()])
            .map_err(|error| error.to_string())?;
        parse_dommemstat(&output)
    }
}

/// Parse `virsh dommemstat` output (`<field> <value-in-kib>` lines) into
/// `MemoryStats`. `actual` (the balloon-adjusted guest memory size) is used
/// as the total; `unused` (memory the guest is not using) is used as free.
/// `available` is used when the virtio-balloon driver reports it, otherwise
/// it falls back to `unused`, matching `parse_memory_stats`'s behavior for
/// an optional `stat-available` value.
pub fn parse_dommemstat(output: &str) -> Result<MemoryStats, String> {
    let mut fields = std::collections::HashMap::new();
    for line in output.lines() {
        let mut parts = line.split_whitespace();
        let (Some(name), Some(value)) = (parts.next(), parts.next()) else {
            continue;
        };
        let value_kib: u64 = value
            .parse()
            .map_err(|_| format!("dommemstat field {name} has a non-numeric value: {value}"))?;
        fields.insert(name.to_owned(), value_kib);
    }

    let actual_kib = fields
        .get("actual")
        .copied()
        .ok_or_else(|| "dommemstat response is missing the required 'actual' field".to_owned())?;
    let unused_kib = fields
        .get("unused")
        .copied()
        .ok_or_else(|| "dommemstat response is missing the required 'unused' field".to_owned())?;
    let available_kib = fields.get("available").copied().unwrap_or(unused_kib);

    let total_bytes = actual_kib.saturating_mul(KIB);
    let free_bytes = unused_kib.saturating_mul(KIB);
    let available_bytes = available_kib.saturating_mul(KIB);

    if free_bytes > total_bytes || available_bytes > total_bytes {
        return Err(format!(
            "dommemstat reported inconsistent values: unused={unused_kib}KiB available={available_kib}KiB actual={actual_kib}KiB"
        ));
    }

    Ok(MemoryStats {
        free_bytes,
        available_bytes,
        total_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::virsh::VirshError;

    struct Fake {
        output: &'static str,
    }
    impl VirshCommand for Fake {
        fn run(&self, arguments: &[String]) -> Result<String, VirshError> {
            assert_eq!(arguments, &["dommemstat".to_owned(), "guest".to_owned()]);
            Ok(self.output.to_owned())
        }
    }

    #[test]
    fn parses_balloon_stats_without_qga() {
        let stats = DomMemStatSource::new(
            Fake {
                output: "actual 8388608\nunused 2097152\navailable 6291456\nrss 512000\n",
            },
            "guest",
        )
        .get_memory_stats()
        .expect("valid dommemstat output");

        assert_eq!(stats.total_bytes, 8388608 * KIB);
        assert_eq!(stats.free_bytes, 2097152 * KIB);
        assert_eq!(stats.available_bytes, 6291456 * KIB);
    }

    #[test]
    fn falls_back_to_unused_when_available_is_missing() {
        let stats = parse_dommemstat("actual 100\nunused 40\n").expect("valid output");
        assert_eq!(stats.available_bytes, 40 * KIB);
    }

    #[test]
    fn rejects_missing_required_fields() {
        assert!(parse_dommemstat("unused 40\n").is_err());
        assert!(parse_dommemstat("actual 100\n").is_err());
    }

    #[test]
    fn rejects_inconsistent_values() {
        assert!(parse_dommemstat("actual 10\nunused 20\n").is_err());
    }
}
