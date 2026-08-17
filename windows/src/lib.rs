use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Deserialize)]
struct GuestMemoryStat {
    stat: String,
    value: u64,
}

#[derive(Debug, Deserialize)]
struct GuestMemoryStatsResponse {
    #[serde(rename = "return")]
    stats: Vec<GuestMemoryStat>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct MemoryStats {
    pub free_bytes: u64,
    pub available_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryControllerConfig {
    pub min_memory_bytes: u64,
    pub max_memory_bytes: u64,
    pub lower_threshold_bytes: u64,
    pub upper_threshold_bytes: u64,
    pub block_size_bytes: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ResizeDecision {
    NoChange,
    WaitForConvergence,
    Request { requested_bytes: u64 },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MemoryStatsError {
    #[error("invalid QEMU Guest Agent response: {0}")]
    InvalidJson(String),
    #[error("QEMU Guest Agent response is missing {0}")]
    MissingStat(&'static str),
    #[error("QEMU Guest Agent response has inconsistent memory values")]
    InconsistentValues,
    #[error("memory controller configuration is invalid: {0}")]
    InvalidConfiguration(&'static str),
}

impl MemoryControllerConfig {
    pub fn validate(self) -> Result<(), MemoryStatsError> {
        if self.block_size_bytes == 0 {
            return Err(MemoryStatsError::InvalidConfiguration(
                "block size must be greater than zero",
            ));
        }
        if self.min_memory_bytes > self.max_memory_bytes {
            return Err(MemoryStatsError::InvalidConfiguration(
                "minimum memory must not exceed maximum memory",
            ));
        }
        if self.lower_threshold_bytes > self.upper_threshold_bytes {
            return Err(MemoryStatsError::InvalidConfiguration(
                "lower threshold must not exceed upper threshold",
            ));
        }
        if !self.min_memory_bytes.is_multiple_of(self.block_size_bytes)
            || !self.max_memory_bytes.is_multiple_of(self.block_size_bytes)
        {
            return Err(MemoryStatsError::InvalidConfiguration(
                "memory limits must be aligned to the block size",
            ));
        }
        Ok(())
    }
}

pub fn plan_resize(
    stats: &MemoryStats,
    requested_bytes: u64,
    current_bytes: u64,
    config: MemoryControllerConfig,
) -> Result<ResizeDecision, MemoryStatsError> {
    config.validate()?;

    if requested_bytes != current_bytes {
        return Ok(ResizeDecision::WaitForConvergence);
    }
    if current_bytes < config.min_memory_bytes || current_bytes > config.max_memory_bytes {
        return Err(MemoryStatsError::InconsistentValues);
    }

    if stats.free_bytes < config.lower_threshold_bytes {
        let target = current_bytes
            .saturating_add(config.block_size_bytes)
            .min(config.max_memory_bytes);
        return Ok(if target == current_bytes {
            ResizeDecision::NoChange
        } else {
            ResizeDecision::Request {
                requested_bytes: target,
            }
        });
    }

    if stats.free_bytes > config.upper_threshold_bytes {
        let target = current_bytes
            .saturating_sub(config.block_size_bytes)
            .max(config.min_memory_bytes);
        return Ok(if target == current_bytes {
            ResizeDecision::NoChange
        } else {
            ResizeDecision::Request {
                requested_bytes: target,
            }
        });
    }

    Ok(ResizeDecision::NoChange)
}

pub fn parse_memory_stats(response: &str) -> Result<MemoryStats, MemoryStatsError> {
    let response: GuestMemoryStatsResponse = serde_json::from_str(response)
        .map_err(|error| MemoryStatsError::InvalidJson(error.to_string()))?;

    let find = |name: &'static str| {
        response
            .stats
            .iter()
            .find(|entry| entry.stat == name)
            .map(|entry| entry.value)
            .ok_or(MemoryStatsError::MissingStat(name))
    };

    let free_bytes = find("stat-free")?;
    let total_bytes = find("stat-total")?;
    let available_bytes = response
        .stats
        .iter()
        .find(|entry| entry.stat == "stat-available")
        .map_or(free_bytes, |entry| entry.value);

    if free_bytes > total_bytes || available_bytes > total_bytes {
        return Err(MemoryStatsError::InconsistentValues);
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

    #[test]
    fn parses_expected_memory_stats() {
        let stats = parse_memory_stats(
            r#"{"return":[
                {"stat":"stat-free","value":2147483648},
                {"stat":"stat-total","value":8589934592},
                {"stat":"stat-available","value":3221225472}
            ]}"#,
        )
        .expect("valid response should parse");

        assert_eq!(
            stats,
            MemoryStats {
                free_bytes: 2_147_483_648,
                available_bytes: 3_221_225_472,
                total_bytes: 8_589_934_592,
            }
        );
    }

    #[test]
    fn falls_back_to_free_when_available_is_missing() {
        let stats = parse_memory_stats(
            r#"{"return":[
                {"stat":"stat-free","value":100},
                {"stat":"stat-total","value":200}
            ]}"#,
        )
        .expect("response without available should parse");

        assert_eq!(stats.available_bytes, stats.free_bytes);
    }

    #[test]
    fn rejects_missing_required_stat() {
        let error = parse_memory_stats(r#"{"return":[{"stat":"stat-free","value":100}]}"#)
            .expect_err("missing total should fail");

        assert_eq!(error, MemoryStatsError::MissingStat("stat-total"));
    }

    #[test]
    fn rejects_inconsistent_values() {
        let error = parse_memory_stats(
            r#"{"return":[
                {"stat":"stat-free","value":201},
                {"stat":"stat-total","value":200}
            ]}"#,
        )
        .expect_err("free greater than total should fail");

        assert_eq!(error, MemoryStatsError::InconsistentValues);
    }

    fn config() -> MemoryControllerConfig {
        MemoryControllerConfig {
            min_memory_bytes: 8,
            max_memory_bytes: 28,
            lower_threshold_bytes: 2,
            upper_threshold_bytes: 6,
            block_size_bytes: 4,
        }
    }

    fn stats(free_bytes: u64) -> MemoryStats {
        MemoryStats {
            free_bytes,
            available_bytes: free_bytes,
            total_bytes: 16,
        }
    }

    #[test]
    fn requests_one_aligned_block_when_memory_is_low() {
        assert_eq!(
            plan_resize(&stats(1), 16, 16, config()).expect("valid policy"),
            ResizeDecision::Request {
                requested_bytes: 20
            }
        );
    }

    #[test]
    fn requests_one_block_removal_when_memory_is_high() {
        assert_eq!(
            plan_resize(&stats(7), 16, 16, config()).expect("valid policy"),
            ResizeDecision::Request {
                requested_bytes: 12
            }
        );
    }

    #[test]
    fn does_not_resize_at_threshold_boundaries() {
        assert_eq!(
            plan_resize(&stats(2), 16, 16, config()).expect("valid policy"),
            ResizeDecision::NoChange
        );
        assert_eq!(
            plan_resize(&stats(6), 16, 16, config()).expect("valid policy"),
            ResizeDecision::NoChange
        );
    }

    #[test]
    fn waits_until_previous_request_converges() {
        assert_eq!(
            plan_resize(&stats(1), 20, 16, config()).expect("valid policy"),
            ResizeDecision::WaitForConvergence
        );
    }

    #[test]
    fn clamps_requests_to_safe_limits() {
        assert_eq!(
            plan_resize(&stats(1), 28, 28, config()).expect("valid policy"),
            ResizeDecision::NoChange
        );
        assert_eq!(
            plan_resize(&stats(7), 8, 8, config()).expect("valid policy"),
            ResizeDecision::NoChange
        );
    }

    #[test]
    fn rejects_unaligned_limits() {
        let mut invalid = config();
        invalid.max_memory_bytes = 27;

        assert_eq!(
            invalid.validate(),
            Err(MemoryStatsError::InvalidConfiguration(
                "memory limits must be aligned to the block size"
            ))
        );
    }
}
