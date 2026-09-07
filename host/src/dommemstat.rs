//! Freshness-qualified `virsh dommemstat` telemetry.
//!
//! Libvirt's `actual` field is balloon state, not whole-guest memory and not
//! virtio-mem allocation. Policy uses `unused` as free memory and `available`
//! as the total-like bound; alias-scoped live XML remains allocation authority.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use virtio_mem_core::MemoryStats;

use crate::runtime::GuestStatsSource;
use crate::virsh::VirshCommand;

const KIB: u64 = 1024;

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

#[derive(Debug, PartialEq, Eq)]
pub struct DomMemStatSnapshot {
    pub stats: MemoryStats,
    pub balloon_actual_bytes: u64,
    pub last_update_unix_seconds: u64,
    pub observed_unix_seconds: u64,
}

pub struct DomMemStatSource<C, T = SystemUnixClock> {
    command: C,
    vm_name: String,
    clock: T,
    max_age: Duration,
    future_tolerance: Duration,
    previous_last_update: RefCell<Option<u64>>,
}

impl<C> DomMemStatSource<C, SystemUnixClock> {
    pub fn new(
        command: C,
        vm_name: impl Into<String>,
        max_age: Duration,
        future_tolerance: Duration,
    ) -> Self {
        Self::with_clock(command, vm_name, SystemUnixClock, max_age, future_tolerance)
    }
}

impl<C, T> DomMemStatSource<C, T> {
    pub fn with_clock(
        command: C,
        vm_name: impl Into<String>,
        clock: T,
        max_age: Duration,
        future_tolerance: Duration,
    ) -> Self {
        Self {
            command,
            vm_name: vm_name.into(),
            clock,
            max_age,
            future_tolerance,
            previous_last_update: RefCell::new(None),
        }
    }
}

impl<C: VirshCommand, T: UnixClock> DomMemStatSource<C, T> {
    pub fn snapshot(&self) -> Result<DomMemStatSnapshot, String> {
        let output = self
            .command
            .run(&["dommemstat".to_owned(), self.vm_name.clone()])
            .map_err(|error| error.to_string())?;
        let observed = self.clock.now_unix_seconds()?;
        let previous = *self.previous_last_update.borrow();
        let snapshot = parse_dommemstat_at(
            &output,
            observed,
            self.max_age.as_secs(),
            self.future_tolerance.as_secs(),
            previous,
        )?;
        self.previous_last_update
            .replace(Some(snapshot.last_update_unix_seconds));
        Ok(snapshot)
    }
}

impl<C: VirshCommand, T: UnixClock> GuestStatsSource for DomMemStatSource<C, T> {
    fn get_memory_stats(&self) -> Result<MemoryStats, String> {
        self.snapshot().map(|snapshot| snapshot.stats)
    }
}

pub fn parse_dommemstat_at(
    output: &str,
    observed_unix_seconds: u64,
    max_age_seconds: u64,
    future_tolerance_seconds: u64,
    previous_last_update: Option<u64>,
) -> Result<DomMemStatSnapshot, String> {
    if max_age_seconds == 0 {
        return Err("dommemstat maximum age must be positive".to_owned());
    }
    let mut fields = HashMap::new();
    for (index, line) in output.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let name = parts
            .next()
            .ok_or_else(|| format!("dommemstat line {} is missing a field name", index + 1))?;
        let value = parts
            .next()
            .ok_or_else(|| format!("dommemstat field {name} is missing a value"))?;
        if parts.next().is_some() {
            return Err(format!("dommemstat field {name} has trailing data"));
        }
        let value: u64 = value
            .parse()
            .map_err(|_| format!("dommemstat field {name} has a non-numeric value: {value}"))?;
        if fields.insert(name.to_owned(), value).is_some() {
            return Err(format!("dommemstat response repeats field {name}"));
        }
    }

    let actual_kib = required(&fields, "actual")?;
    let unused_kib = required(&fields, "unused")?;
    let available_kib = required(&fields, "available")?;
    let last_update = required(&fields, "last-update")?;

    if last_update > observed_unix_seconds.saturating_add(future_tolerance_seconds) {
        return Err(format!(
            "dommemstat last-update {last_update} is in the future relative to observation {observed_unix_seconds}"
        ));
    }
    if observed_unix_seconds.saturating_sub(last_update) > max_age_seconds {
        return Err(format!(
            "dommemstat last-update {last_update} is stale at observation {observed_unix_seconds}"
        ));
    }
    if let Some(previous) = previous_last_update {
        if last_update <= previous {
            return Err(format!(
                "dommemstat last-update did not advance beyond {previous}"
            ));
        }
    }

    let balloon_actual_bytes = kib_to_bytes(actual_kib, "actual")?;
    let free_bytes = kib_to_bytes(unused_kib, "unused")?;
    let available_bytes = kib_to_bytes(available_kib, "available")?;
    if free_bytes > available_bytes {
        return Err(format!(
            "dommemstat reported unused={unused_kib}KiB above available={available_kib}KiB"
        ));
    }

    Ok(DomMemStatSnapshot {
        stats: MemoryStats {
            free_bytes,
            available_bytes,
            total_bytes: available_bytes,
        },
        balloon_actual_bytes,
        last_update_unix_seconds: last_update,
        observed_unix_seconds,
    })
}

fn required(fields: &HashMap<String, u64>, name: &'static str) -> Result<u64, String> {
    fields
        .get(name)
        .copied()
        .ok_or_else(|| format!("dommemstat response is missing the required '{name}' field"))
}

fn kib_to_bytes(value: u64, name: &'static str) -> Result<u64, String> {
    value
        .checked_mul(KIB)
        .ok_or_else(|| format!("dommemstat '{name}' value overflows bytes"))
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;
    use crate::virsh::VirshError;

    struct Fake {
        outputs: RefCell<VecDeque<&'static str>>,
    }
    impl VirshCommand for Fake {
        fn run(&self, arguments: &[String]) -> Result<String, VirshError> {
            assert_eq!(arguments, &["dommemstat".to_owned(), "guest".to_owned()]);
            self.outputs
                .borrow_mut()
                .pop_front()
                .map(str::to_owned)
                .ok_or_else(|| VirshError::Failed {
                    status: "fixture exhausted".to_owned(),
                    stderr: String::new(),
                })
        }
    }

    struct FakeClock {
        values: RefCell<VecDeque<u64>>,
    }
    impl UnixClock for FakeClock {
        fn now_unix_seconds(&self) -> Result<u64, String> {
            self.values
                .borrow_mut()
                .pop_front()
                .ok_or_else(|| "clock fixture exhausted".to_owned())
        }
    }

    fn parse(output: &str) -> Result<DomMemStatSnapshot, String> {
        parse_dommemstat_at(output, 1_000, 60, 5, None)
    }

    #[test]
    fn preserves_available_and_unused_above_balloon_actual() {
        let snapshot = parse("actual 100\nunused 200\navailable 300\nlast-update 990\nrss 400\n")
            .expect("balloon actual does not bound guest counters");
        assert_eq!(snapshot.balloon_actual_bytes, 100 * KIB);
        assert_eq!(snapshot.stats.free_bytes, 200 * KIB);
        assert_eq!(snapshot.stats.available_bytes, 300 * KIB);
        assert_eq!(snapshot.stats.total_bytes, 300 * KIB);
    }

    #[test]
    fn rejects_missing_malformed_duplicate_and_inconsistent_fields() {
        assert!(parse("actual 100\nunused 40\navailable 80\n").is_err());
        assert!(parse("actual 100\nunused 40\nlast-update 990\n").is_err());
        assert!(parse("actual 100\nunused nope\navailable 80\nlast-update 990\n").is_err());
        assert!(parse("actual 100\nunused 40 extra\navailable 80\nlast-update 990\n").is_err());
        assert!(
            parse("actual 100\nactual 100\nunused 40\navailable 80\nlast-update 990\n").is_err()
        );
        assert!(parse("actual 100\nunused 90\navailable 80\nlast-update 990\n").is_err());
    }

    #[test]
    fn rejects_missing_stale_future_and_nonadvancing_last_update() {
        let base = "actual 100\nunused 40\navailable 80\n";
        assert!(parse(base).is_err());
        assert!(
            parse_dommemstat_at(&format!("{base}last-update 939\n"), 1_000, 60, 5, None).is_err()
        );
        assert!(
            parse_dommemstat_at(&format!("{base}last-update 1006\n"), 1_000, 60, 5, None).is_err()
        );
        assert!(
            parse_dommemstat_at(&format!("{base}last-update 990\n"), 1_000, 60, 5, Some(990))
                .is_err()
        );
        assert!(
            parse_dommemstat_at(&format!("{base}last-update 989\n"), 1_000, 60, 5, Some(990))
                .is_err()
        );
        assert!(
            parse_dommemstat_at(&format!("{base}last-update 995\n"), 1_000, 60, 5, Some(990))
                .is_ok()
        );
    }

    #[test]
    fn source_tracks_advancement_with_an_injected_clock() {
        let source = DomMemStatSource::with_clock(
            Fake {
                outputs: RefCell::new(VecDeque::from([
                    "actual 100\nunused 40\navailable 80\nlast-update 990\n",
                    "actual 100\nunused 41\navailable 80\nlast-update 995\n",
                    "actual 100\nunused 42\navailable 80\nlast-update 995\n",
                ])),
            },
            "guest",
            FakeClock {
                values: RefCell::new(VecDeque::from([1_000, 1_001, 1_002])),
            },
            Duration::from_secs(60),
            Duration::from_secs(5),
        );
        assert!(source.get_memory_stats().is_ok());
        assert!(source.get_memory_stats().is_ok());
        assert!(source.get_memory_stats().is_err());
    }

    #[test]
    fn rejects_values_that_overflow_bytes() {
        assert!(
            parse("actual 18014398509481984\nunused 1\navailable 2\nlast-update 990\n").is_err()
        );
        assert!(parse(
            "actual 100\nunused 18014398509481984\navailable 18014398509481984\nlast-update 990\n"
        )
        .is_err());
        assert!(
            parse("actual 100\nunused 1\navailable 18014398509481984\nlast-update 990\n").is_err()
        );
    }
}
