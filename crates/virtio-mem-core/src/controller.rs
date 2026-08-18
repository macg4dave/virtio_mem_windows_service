use crate::error::MemoryStatsError;
use crate::stats::MemoryStats;

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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn plans_and_bounds_one_block_requests() {
        assert_eq!(
            plan_resize(&stats(1), 16, 16, config()).expect("valid policy"),
            ResizeDecision::Request {
                requested_bytes: 20
            }
        );
        assert_eq!(
            plan_resize(&stats(7), 16, 16, config()).expect("valid policy"),
            ResizeDecision::Request {
                requested_bytes: 12
            }
        );
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
    fn honours_thresholds_and_pending_convergence() {
        assert_eq!(
            plan_resize(&stats(2), 16, 16, config()).expect("valid policy"),
            ResizeDecision::NoChange
        );
        assert_eq!(
            plan_resize(&stats(6), 16, 16, config()).expect("valid policy"),
            ResizeDecision::NoChange
        );
        assert_eq!(
            plan_resize(&stats(1), 20, 16, config()).expect("valid policy"),
            ResizeDecision::WaitForConvergence
        );
    }

    #[test]
    fn rejects_invalid_configuration() {
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
