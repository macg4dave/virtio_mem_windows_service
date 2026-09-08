use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use virtio_mem_core::RawTelemetryEnvelope;

pub trait RawTelemetrySource {
    fn read(&self) -> Result<RawTelemetryEnvelope, String>;
}

impl RawTelemetrySource for Box<dyn RawTelemetrySource> {
    fn read(&self) -> Result<RawTelemetryEnvelope, String> {
        (**self).read()
    }
}

pub trait UnixClock {
    fn now_unix_seconds(&self) -> Result<u64, String>;
}

#[derive(Debug, Clone, Copy)]
pub struct SystemUnixClock;

impl UnixClock for SystemUnixClock {
    fn now_unix_seconds(&self) -> Result<u64, String> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .map_err(|error| format!("system clock is before the Unix epoch: {error}"))
    }
}

#[derive(Debug)]
pub struct FileRawTelemetrySource<T = SystemUnixClock> {
    path: PathBuf,
    expected_vm_name: String,
    max_age: Duration,
    future_tolerance: Duration,
    clock: T,
}

impl FileRawTelemetrySource<SystemUnixClock> {
    pub fn new(
        path: impl Into<PathBuf>,
        expected_vm_name: impl Into<String>,
        max_age: Duration,
        future_tolerance: Duration,
    ) -> Self {
        Self::with_clock(
            path,
            expected_vm_name,
            max_age,
            future_tolerance,
            SystemUnixClock,
        )
    }
}

impl<T> FileRawTelemetrySource<T> {
    pub fn with_clock(
        path: impl Into<PathBuf>,
        expected_vm_name: impl Into<String>,
        max_age: Duration,
        future_tolerance: Duration,
        clock: T,
    ) -> Self {
        Self {
            path: path.into(),
            expected_vm_name: expected_vm_name.into(),
            max_age,
            future_tolerance,
            clock,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl<T: UnixClock> RawTelemetrySource for FileRawTelemetrySource<T> {
    fn read(&self) -> Result<RawTelemetryEnvelope, String> {
        let contents = std::fs::read_to_string(&self.path)
            .map_err(|error| format!("read raw telemetry {}: {error}", self.path.display()))?;
        let record = contents
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .ok_or_else(|| format!("raw telemetry {} has no records", self.path.display()))?;
        let envelope: RawTelemetryEnvelope = serde_json::from_str(record)
            .map_err(|error| format!("parse latest raw telemetry record: {error}"))?;
        let now = self.clock.now_unix_seconds()?;
        envelope
            .validate_for(
                &self.expected_vm_name,
                now,
                self.max_age.as_secs(),
                self.future_tolerance.as_secs(),
            )
            .map_err(|error| error.to_string())?;
        Ok(envelope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use virtio_mem_core::MemoryTelemetrySnapshot;

    struct FixedClock(u64);

    impl UnixClock for FixedClock {
        fn now_unix_seconds(&self) -> Result<u64, String> {
            Ok(self.0)
        }
    }

    fn snapshot() -> MemoryTelemetrySnapshot {
        MemoryTelemetrySnapshot {
            physical_total_bytes: 100,
            physical_available_bytes: 40,
            memory_load_percent: 60,
            commit_total_bytes: 50,
            commit_limit_bytes: 100,
            commit_peak_bytes: 50,
            system_cache_bytes: 0,
            kernel_paged_bytes: 0,
            kernel_nonpaged_bytes: 0,
        }
    }

    fn path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "virtio-mem-raw-telemetry-{name}-{}.jsonl",
            std::process::id()
        ))
    }

    #[test]
    fn reads_latest_fresh_matching_record() {
        let path = path("fresh");
        let old = RawTelemetryEnvelope::new("guest", 900, snapshot());
        let latest = RawTelemetryEnvelope::new("guest", 995, snapshot());
        std::fs::write(
            &path,
            format!(
                "{}\n{}\n",
                serde_json::to_string(&old).expect("encode old"),
                serde_json::to_string(&latest).expect("encode latest")
            ),
        )
        .expect("write fixture");
        let source = FileRawTelemetrySource::with_clock(
            &path,
            "guest",
            Duration::from_secs(60),
            Duration::from_secs(5),
            FixedClock(1_000),
        );

        assert_eq!(source.read(), Ok(latest));
        std::fs::remove_file(path).expect("remove fixture");
    }

    #[test]
    fn rejects_missing_stale_cross_vm_and_malformed_records() {
        for (name, contents) in [
            ("missing", String::new()),
            (
                "stale",
                serde_json::to_string(&RawTelemetryEnvelope::new("guest", 900, snapshot()))
                    .expect("encode stale"),
            ),
            (
                "cross-vm",
                serde_json::to_string(&RawTelemetryEnvelope::new("other", 995, snapshot()))
                    .expect("encode cross VM"),
            ),
            ("malformed", "{not-json".to_owned()),
        ] {
            let path = path(name);
            std::fs::write(&path, contents).expect("write fixture");
            let source = FileRawTelemetrySource::with_clock(
                &path,
                "guest",
                Duration::from_secs(60),
                Duration::from_secs(5),
                FixedClock(1_000),
            );
            assert!(source.read().is_err(), "{name} should fail");
            std::fs::remove_file(path).expect("remove fixture");
        }
    }
}
