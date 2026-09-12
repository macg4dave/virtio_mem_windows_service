use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use virtio_mem_core::RawTelemetryEnvelope;

use crate::qga::GuestFileReader;

pub const MAX_RAW_TELEMETRY_RECORD_BYTES: usize = 64 * 1024;
pub const MAX_RAW_TELEMETRY_FILE_BYTES: u64 = 1024 * 1024;
const RETIRED_SESSION_LIMIT: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawTelemetryRead {
    Fresh(RawTelemetryEnvelope),
    Unchanged(RawTelemetryEnvelope),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RawTelemetryError {
    #[error("{0}")]
    TransportUnavailable(String),
    #[error("{0}")]
    InvalidEvidence(String),
}

impl RawTelemetryError {
    pub fn invalidates_history(&self) -> bool {
        matches!(self, Self::InvalidEvidence(_))
    }
}

pub trait RawTelemetrySource {
    fn read(&self) -> Result<RawTelemetryRead, RawTelemetryError>;
}

impl RawTelemetrySource for Box<dyn RawTelemetrySource> {
    fn read(&self) -> Result<RawTelemetryRead, RawTelemetryError> {
        (**self).read()
    }
}

pub trait UnixClock {
    fn now_unix_millis(&self) -> Result<u64, String>;
}

#[derive(Debug, Clone, Copy)]
pub struct SystemUnixClock;

impl UnixClock for SystemUnixClock {
    fn now_unix_millis(&self) -> Result<u64, String> {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?;
        u64::try_from(duration.as_millis())
            .map_err(|_| "system Unix timestamp does not fit in u64 milliseconds".to_owned())
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplayState {
    last: Option<RawTelemetryEnvelope>,
    retired_sessions: VecDeque<String>,
}

#[derive(Debug)]
pub struct FileRawTelemetrySource<T = SystemUnixClock> {
    path: PathBuf,
    replay_state_path: PathBuf,
    expected_vm_name: String,
    expected_service_name: String,
    max_age: Duration,
    future_tolerance: Duration,
    clock: T,
    replay: Mutex<ReplayState>,
}

#[derive(Debug)]
pub struct QgaFileRawTelemetrySource<R, T = SystemUnixClock> {
    reader: R,
    guest_path: String,
    replay_state_path: PathBuf,
    expected_vm_name: String,
    expected_service_name: String,
    max_age: Duration,
    future_tolerance: Duration,
    clock: T,
    replay: Mutex<ReplayState>,
}

impl<R> QgaFileRawTelemetrySource<R, SystemUnixClock> {
    pub fn new(
        reader: R,
        guest_path: impl Into<String>,
        replay_state_path: impl Into<PathBuf>,
        expected_vm_name: impl Into<String>,
        expected_service_name: impl Into<String>,
        max_age: Duration,
        future_tolerance: Duration,
    ) -> Self {
        Self::with_clock(
            reader,
            guest_path,
            replay_state_path,
            expected_vm_name,
            expected_service_name,
            max_age,
            future_tolerance,
            SystemUnixClock,
        )
    }
}

impl<R, T> QgaFileRawTelemetrySource<R, T> {
    #[allow(clippy::too_many_arguments)]
    pub fn with_clock(
        reader: R,
        guest_path: impl Into<String>,
        replay_state_path: impl Into<PathBuf>,
        expected_vm_name: impl Into<String>,
        expected_service_name: impl Into<String>,
        max_age: Duration,
        future_tolerance: Duration,
        clock: T,
    ) -> Self {
        Self {
            reader,
            guest_path: guest_path.into(),
            replay_state_path: replay_state_path.into(),
            expected_vm_name: expected_vm_name.into(),
            expected_service_name: expected_service_name.into(),
            max_age,
            future_tolerance,
            clock,
            replay: Mutex::new(ReplayState::default()),
        }
    }
}

impl FileRawTelemetrySource<SystemUnixClock> {
    pub fn new(
        path: impl Into<PathBuf>,
        expected_vm_name: impl Into<String>,
        expected_service_name: impl Into<String>,
        max_age: Duration,
        future_tolerance: Duration,
    ) -> Self {
        let path = path.into();
        Self::with_clock(
            &path,
            expected_vm_name,
            expected_service_name,
            max_age,
            future_tolerance,
            SystemUnixClock,
        )
        .with_replay_state_path(default_replay_state_path(&path))
    }
}

impl<T> FileRawTelemetrySource<T> {
    pub fn with_clock(
        path: impl Into<PathBuf>,
        expected_vm_name: impl Into<String>,
        expected_service_name: impl Into<String>,
        max_age: Duration,
        future_tolerance: Duration,
        clock: T,
    ) -> Self {
        Self {
            path: path.into(),
            replay_state_path: PathBuf::new(),
            expected_vm_name: expected_vm_name.into(),
            expected_service_name: expected_service_name.into(),
            max_age,
            future_tolerance,
            clock,
            replay: Mutex::new(ReplayState::default()),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn with_replay_state_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.replay_state_path = path.into();
        self
    }
}

impl<T: UnixClock> RawTelemetrySource for FileRawTelemetrySource<T> {
    fn read(&self) -> Result<RawTelemetryRead, RawTelemetryError> {
        let metadata = std::fs::metadata(&self.path).map_err(|error| {
            RawTelemetryError::TransportUnavailable(format!(
                "inspect raw telemetry {}: {error}",
                self.path.display()
            ))
        })?;
        if metadata.len() > MAX_RAW_TELEMETRY_FILE_BYTES {
            return Err(RawTelemetryError::InvalidEvidence(format!(
                "raw telemetry {} exceeds {} byte file limit",
                self.path.display(),
                MAX_RAW_TELEMETRY_FILE_BYTES
            )));
        }
        let mut contents = String::new();
        std::fs::File::open(&self.path)
            .map_err(|error| {
                RawTelemetryError::TransportUnavailable(format!(
                    "open raw telemetry {}: {error}",
                    self.path.display()
                ))
            })?
            .take(MAX_RAW_TELEMETRY_FILE_BYTES + 1)
            .read_to_string(&mut contents)
            .map_err(|error| {
                RawTelemetryError::TransportUnavailable(format!(
                    "read raw telemetry {}: {error}",
                    self.path.display()
                ))
            })?;
        if contents.len() as u64 > MAX_RAW_TELEMETRY_FILE_BYTES {
            return Err(RawTelemetryError::InvalidEvidence(format!(
                "raw telemetry {} exceeds {} byte file limit",
                self.path.display(),
                MAX_RAW_TELEMETRY_FILE_BYTES
            )));
        }
        if !contents.is_empty() && !contents.ends_with('\n') {
            return Err(RawTelemetryError::InvalidEvidence(format!(
                "raw telemetry {} ends with a partial record",
                self.path.display()
            )));
        }
        validate_contents(
            &contents,
            &self.expected_vm_name,
            &self.expected_service_name,
            self.max_age,
            self.future_tolerance,
            &self.clock,
            &self.replay_state_path,
            &self.replay,
        )
        .map_err(RawTelemetryError::InvalidEvidence)
    }
}

impl<R: GuestFileReader, T: UnixClock> RawTelemetrySource for QgaFileRawTelemetrySource<R, T> {
    fn read(&self) -> Result<RawTelemetryRead, RawTelemetryError> {
        let bytes = self
            .reader
            .read_file(&self.guest_path, MAX_RAW_TELEMETRY_FILE_BYTES as usize)
            .map_err(RawTelemetryError::TransportUnavailable)?;
        let contents = String::from_utf8(bytes).map_err(|error| {
            RawTelemetryError::InvalidEvidence(format!(
                "QGA raw telemetry is not valid UTF-8: {error}"
            ))
        })?;
        validate_contents(
            &contents,
            &self.expected_vm_name,
            &self.expected_service_name,
            self.max_age,
            self.future_tolerance,
            &self.clock,
            &self.replay_state_path,
            &self.replay,
        )
        .map_err(RawTelemetryError::InvalidEvidence)
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_contents<T: UnixClock>(
    contents: &str,
    expected_vm_name: &str,
    expected_service_name: &str,
    max_age: Duration,
    future_tolerance: Duration,
    clock: &T,
    replay_state_path: &Path,
    replay: &Mutex<ReplayState>,
) -> Result<RawTelemetryRead, String> {
    if contents.len() as u64 > MAX_RAW_TELEMETRY_FILE_BYTES {
        return Err(format!(
            "raw telemetry exceeds {} byte file limit",
            MAX_RAW_TELEMETRY_FILE_BYTES
        ));
    }
    if !contents.is_empty() && !contents.ends_with('\n') {
        return Err("raw telemetry ends with a partial record".to_owned());
    }
    let record = contents
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| "raw telemetry has no records".to_owned())?;
    if record.len() > MAX_RAW_TELEMETRY_RECORD_BYTES {
        return Err(format!(
            "latest raw telemetry record exceeds {} byte limit",
            MAX_RAW_TELEMETRY_RECORD_BYTES
        ));
    }
    let envelope: RawTelemetryEnvelope = serde_json::from_str(record)
        .map_err(|error| format!("parse latest raw telemetry record: {error}"))?;
    let now = clock.now_unix_millis()?;
    envelope
        .validate_for(
            expected_vm_name,
            expected_service_name,
            now,
            duration_millis(max_age)?,
            duration_millis(future_tolerance)?,
        )
        .map_err(|error| error.to_string())?;
    let mut replay = replay
        .lock()
        .map_err(|_| "raw telemetry replay state lock is poisoned".to_owned())?;
    if replay.last.is_none() && replay.retired_sessions.is_empty() {
        *replay = load_replay_state(replay_state_path)?;
    }
    if replay
        .retired_sessions
        .iter()
        .any(|session| session == &envelope.session_id)
    {
        return Err(format!(
            "raw telemetry reuses retired session {}",
            envelope.session_id
        ));
    }
    if let Some(previous) = &replay.last {
        if &envelope == previous {
            return Ok(RawTelemetryRead::Unchanged(envelope));
        }
        envelope
            .validate_successor(previous)
            .map_err(|error| error.to_string())?;
        if envelope.session_id != previous.session_id {
            let retired = previous.session_id.clone();
            replay.retired_sessions.push_back(retired);
            if replay.retired_sessions.len() > RETIRED_SESSION_LIMIT {
                replay.retired_sessions.pop_front();
            }
        }
    }
    replay.last = Some(envelope.clone());
    persist_replay_state(replay_state_path, &replay)?;
    Ok(RawTelemetryRead::Fresh(envelope))
}

fn default_replay_state_path(telemetry_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.ack.json", telemetry_path.display()))
}

fn load_replay_state(path: &Path) -> Result<ReplayState, String> {
    let path = if path.as_os_str().is_empty() {
        return Ok(ReplayState::default());
    } else {
        path
    };
    let contents = match std::fs::read(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ReplayState::default())
        }
        Err(error) => return Err(format!("read raw telemetry acknowledgement: {error}")),
    };
    if contents.len() > MAX_RAW_TELEMETRY_RECORD_BYTES {
        return Err("raw telemetry acknowledgement exceeds size limit".to_owned());
    }
    serde_json::from_slice(&contents)
        .map_err(|error| format!("parse raw telemetry acknowledgement: {error}"))
}

fn persist_replay_state(path: &Path, state: &ReplayState) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Ok(());
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create acknowledgement directory: {error}"))?;
    }
    let bytes = serde_json::to_vec(state)
        .map_err(|error| format!("encode raw telemetry acknowledgement: {error}"))?;
    let temporary = PathBuf::from(format!("{}.tmp-{}", path.display(), std::process::id()));
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| format!("open raw telemetry acknowledgement: {error}"))?;
    file.write_all(&bytes)
        .map_err(|error| format!("write raw telemetry acknowledgement: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("flush raw telemetry acknowledgement: {error}"))?;
    drop(file);
    std::fs::rename(&temporary, path).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        format!("publish raw telemetry acknowledgement: {error}")
    })?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("flush raw telemetry acknowledgement directory: {error}"))?;
    }
    Ok(())
}

fn duration_millis(duration: Duration) -> Result<u64, String> {
    u64::try_from(duration.as_millis())
        .map_err(|_| "raw telemetry duration is too large".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use virtio_mem_core::MemoryTelemetrySnapshot;

    struct FixedGuestFile(Vec<u8>);

    impl GuestFileReader for FixedGuestFile {
        fn read_file(&self, _path: &str, maximum_bytes: usize) -> Result<Vec<u8>, String> {
            if self.0.len() > maximum_bytes {
                Err("fixture exceeds bound".to_owned())
            } else {
                Ok(self.0.clone())
            }
        }
    }

    struct FixedClock(u64);

    impl UnixClock for FixedClock {
        fn now_unix_millis(&self) -> Result<u64, String> {
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

    fn envelope(
        session: &str,
        observed: u64,
        monotonic: u64,
        sequence: u64,
    ) -> RawTelemetryEnvelope {
        RawTelemetryEnvelope::new(
            "guest",
            "VirtioMemService",
            session,
            observed,
            monotonic,
            sequence,
            snapshot(),
        )
    }

    #[test]
    fn reads_latest_fresh_matching_record() {
        let path = path("fresh");
        let old = envelope("session-a", 900_000, 10, 0);
        let latest = envelope("session-a", 995_000, 20, 1);
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
            "VirtioMemService",
            Duration::from_secs(60),
            Duration::from_secs(5),
            FixedClock(1_000_000),
        );

        assert_eq!(source.read(), Ok(RawTelemetryRead::Fresh(latest)));
        std::fs::remove_file(path).expect("remove fixture");
    }

    #[test]
    fn qga_file_transport_uses_the_same_durable_replay_contract() {
        let acknowledgement = path("qga-ack");
        let record = envelope("session-qga", 995_000, 20, 0);
        let bytes = format!(
            "{}\n",
            serde_json::to_string(&record).expect("encode record")
        )
        .into_bytes();
        let first = QgaFileRawTelemetrySource::with_clock(
            FixedGuestFile(bytes.clone()),
            r"C:\ProgramData\VirtioMemService\telemetry.json",
            &acknowledgement,
            "guest",
            "VirtioMemService",
            Duration::from_secs(60),
            Duration::from_secs(5),
            FixedClock(1_000_000),
        );
        assert_eq!(first.read(), Ok(RawTelemetryRead::Fresh(record.clone())));

        let restarted = QgaFileRawTelemetrySource::with_clock(
            FixedGuestFile(bytes),
            r"C:\ProgramData\VirtioMemService\telemetry.json",
            &acknowledgement,
            "guest",
            "VirtioMemService",
            Duration::from_secs(60),
            Duration::from_secs(5),
            FixedClock(1_000_000),
        );
        assert_eq!(restarted.read(), Ok(RawTelemetryRead::Unchanged(record)));
        std::fs::remove_file(acknowledgement).expect("remove acknowledgement");
    }

    #[test]
    fn rejects_missing_stale_cross_vm_and_malformed_records() {
        for (name, contents) in [
            ("missing", String::new()),
            (
                "stale",
                format!(
                    "{}\n",
                    serde_json::to_string(&envelope("session-a", 900_000, 1, 0))
                        .expect("encode stale")
                ),
            ),
            (
                "cross-vm",
                format!(
                    "{}\n",
                    serde_json::to_string(&RawTelemetryEnvelope::new(
                        "other",
                        "VirtioMemService",
                        "session-a",
                        995_000,
                        1,
                        0,
                        snapshot()
                    ))
                    .expect("encode cross VM")
                ),
            ),
            ("malformed", "{not-json\n".to_owned()),
        ] {
            let path = path(name);
            std::fs::write(&path, contents).expect("write fixture");
            let source = FileRawTelemetrySource::with_clock(
                &path,
                "guest",
                "VirtioMemService",
                Duration::from_secs(60),
                Duration::from_secs(5),
                FixedClock(1_000_000),
            );
            assert!(
                matches!(source.read(), Err(RawTelemetryError::InvalidEvidence(_))),
                "{name} should reject invalid evidence"
            );
            std::fs::remove_file(path).expect("remove fixture");
        }
    }

    #[test]
    fn classifies_transport_failure_separately_from_invalid_evidence() {
        let missing_path = path("transport-unavailable");
        let _ = std::fs::remove_file(&missing_path);
        let file = FileRawTelemetrySource::with_clock(
            &missing_path,
            "guest",
            "VirtioMemService",
            Duration::from_secs(60),
            Duration::from_secs(5),
            FixedClock(1_000_000),
        );
        assert!(matches!(
            file.read(),
            Err(RawTelemetryError::TransportUnavailable(_))
        ));

        let qga = QgaFileRawTelemetrySource::with_clock(
            FixedGuestFile(vec![b'x'; MAX_RAW_TELEMETRY_FILE_BYTES as usize + 1]),
            r"C:\ProgramData\VirtioMemService\telemetry.json",
            path("transport-unavailable-ack"),
            "guest",
            "VirtioMemService",
            Duration::from_secs(60),
            Duration::from_secs(5),
            FixedClock(1_000_000),
        );
        assert!(matches!(
            qga.read(),
            Err(RawTelemetryError::TransportUnavailable(_))
        ));
    }

    #[test]
    fn distinguishes_unchanged_snapshot_and_rejects_non_monotonic_or_retired_sessions() {
        let path = path("replay");
        let source = FileRawTelemetrySource::with_clock(
            &path,
            "guest",
            "VirtioMemService",
            Duration::from_secs(60),
            Duration::from_secs(5),
            FixedClock(1_000_000),
        );
        let first = envelope("session-a", 995_000, 10, 0);
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string(&first).expect("encode first")),
        )
        .expect("write first");
        assert_eq!(source.read(), Ok(RawTelemetryRead::Fresh(first.clone())));
        assert_eq!(
            source.read(),
            Ok(RawTelemetryRead::Unchanged(first)),
            "an unchanged atomic snapshot is normal between producer updates"
        );

        let non_monotonic = envelope("session-a", 996_000, 10, 1);
        std::fs::write(
            &path,
            format!(
                "{}\n",
                serde_json::to_string(&non_monotonic).expect("encode non-monotonic")
            ),
        )
        .expect("write non-monotonic");
        assert!(source.read().is_err());

        let restarted = envelope("session-b", 996_000, 1, 0);
        std::fs::write(
            &path,
            format!(
                "{}\n",
                serde_json::to_string(&restarted).expect("encode restart")
            ),
        )
        .expect("write restart");
        assert_eq!(source.read(), Ok(RawTelemetryRead::Fresh(restarted)));

        let retired = envelope("session-a", 997_000, 20, 2);
        std::fs::write(
            &path,
            format!(
                "{}\n",
                serde_json::to_string(&retired).expect("encode retired")
            ),
        )
        .expect("write retired");
        assert!(source.read().is_err());
        std::fs::remove_file(path).expect("remove fixture");
    }

    #[test]
    fn rejects_partial_and_oversized_delivery() {
        for (name, contents) in [
            ("partial", "{}".to_owned()),
            (
                "oversized-record",
                format!("{}\n", "x".repeat(MAX_RAW_TELEMETRY_RECORD_BYTES + 1)),
            ),
            (
                "oversized-file",
                format!("{}\n", "\n".repeat(MAX_RAW_TELEMETRY_FILE_BYTES as usize)),
            ),
        ] {
            let path = path(name);
            std::fs::write(&path, contents).expect("write bounded-delivery fixture");
            let source = FileRawTelemetrySource::with_clock(
                &path,
                "guest",
                "VirtioMemService",
                Duration::from_secs(60),
                Duration::from_secs(5),
                FixedClock(1_000_000),
            );
            assert!(source.read().is_err(), "{name} should fail");
            std::fs::remove_file(path).expect("remove fixture");
        }
    }

    #[test]
    fn durable_acknowledgement_recognizes_unchanged_snapshot_after_reader_restart() {
        let path = path("durable-replay");
        let acknowledgement = PathBuf::from(format!("{}.ack", path.display()));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&acknowledgement);
        let first = envelope("session-a", 995_000, 10, 0);
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string(&first).expect("encode")),
        )
        .expect("write fixture");

        let first_reader = FileRawTelemetrySource::with_clock(
            &path,
            "guest",
            "VirtioMemService",
            Duration::from_secs(60),
            Duration::from_secs(5),
            FixedClock(1_000_000),
        )
        .with_replay_state_path(&acknowledgement);
        assert_eq!(
            first_reader.read(),
            Ok(RawTelemetryRead::Fresh(first.clone()))
        );

        let restarted_reader = FileRawTelemetrySource::with_clock(
            &path,
            "guest",
            "VirtioMemService",
            Duration::from_secs(60),
            Duration::from_secs(5),
            FixedClock(1_000_000),
        )
        .with_replay_state_path(&acknowledgement);
        assert_eq!(
            restarted_reader.read(),
            Ok(RawTelemetryRead::Unchanged(first))
        );

        std::fs::remove_file(path).expect("remove fixture");
        std::fs::remove_file(acknowledgement).expect("remove acknowledgement");
    }
}
