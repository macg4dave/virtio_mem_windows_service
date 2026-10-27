//! Read-only Linux pressure and paging counters. Missing kernel interfaces
//! remain missing values; cumulative counters never masquerade as rates.
use std::time::Instant;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Pressure {
    pub(super) some_avg10: Option<f64>,
    pub(super) full_avg10: Option<f64>,
    pub(super) swap_in_pages_s: Option<f64>,
    pub(super) swap_out_pages_s: Option<f64>,
    pub(super) major_faults_s: Option<f64>,
}

#[derive(Default)]
pub(super) struct Meter {
    previous: Option<(Instant, [u64; 3])>,
}

impl Meter {
    pub(super) fn sample(&mut self) -> Pressure {
        let mut pressure = Pressure::default();
        if let Ok(text) = std::fs::read_to_string("/proc/pressure/memory") {
            pressure.some_avg10 = psi(&text, "some");
            pressure.full_avg10 = psi(&text, "full");
        }
        let counters = std::fs::read_to_string("/proc/vmstat")
            .ok()
            .and_then(|text| counters(&text));
        if let Some(counters) = counters {
            let now = Instant::now();
            if let Some((before, previous)) = self.previous.replace((now, counters)) {
                let seconds = (now - before).as_secs_f64();
                pressure.swap_in_pages_s = rate(previous[0], counters[0], seconds);
                pressure.swap_out_pages_s = rate(previous[1], counters[1], seconds);
                pressure.major_faults_s = rate(previous[2], counters[2], seconds);
            }
        } else {
            self.previous = None;
        }
        pressure
    }
}

fn psi(text: &str, kind: &str) -> Option<f64> {
    let mut words = text
        .lines()
        .find(|line| line.split_whitespace().next() == Some(kind))?
        .split_whitespace();
    words
        .find_map(|word| word.strip_prefix("avg10=")?.parse::<f64>().ok())
        .filter(|value| value.is_finite() && (0.0..=100.0).contains(value))
}

fn counters(text: &str) -> Option<[u64; 3]> {
    let value = |key| {
        text.lines().find_map(|line| {
            let mut words = line.split_whitespace();
            (words.next()? == key)
                .then(|| words.next()?.parse().ok())
                .flatten()
        })
    };
    Some([value("pswpin")?, value("pswpout")?, value("pgmajfault")?])
}

fn rate(previous: u64, current: u64, seconds: f64) -> Option<f64> {
    (seconds > 0.0)
        .then(|| {
            current
                .checked_sub(previous)
                .map(|delta| delta as f64 / seconds)
        })
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_counters_keep_units_and_reject_resets() {
        assert_eq!(
            counters("pswpin 20\npswpout 30\npgmajfault 40\n"),
            Some([20, 30, 40])
        );
        assert_eq!(counters("pswpin 20\n"), None);
        assert_eq!(rate(20, 30, 2.0), Some(5.0));
        assert_eq!(rate(30, 20, 2.0), None);
        assert_eq!(rate(20, 30, 0.0), None);
        let text = "some avg10=3.25 avg60=2.00 total=55\nfull avg10=0.50 avg60=0 total=1";
        assert_eq!(psi(text, "some"), Some(3.25));
        assert_eq!(psi(text, "full"), Some(0.5));
        assert_eq!(psi("some avg10=NaN", "some"), None);
    }
}
