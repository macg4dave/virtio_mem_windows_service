use crate::error::VirtioMemError;

pub const MIN_BLOCK_SIZE_BYTES: u64 = 1 << 20;

/// A virtio-mem target must never be requested that would leave less than
/// this much of the device's declared size unplugged. This keeps a resize
/// request from ever consuming the full device, independent of operator
/// configuration.
pub const MIN_HEADROOM_BYTES: u64 = 1 << 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtioMemState {
    pub size_bytes: u64,
    pub block_size_bytes: u64,
    pub requested_bytes: u64,
    pub current_bytes: u64,
}

impl VirtioMemState {
    pub fn validate(self) -> Result<(), VirtioMemError> {
        if self.size_bytes == 0 {
            return Err(VirtioMemError::ZeroSize);
        }
        if self.block_size_bytes < MIN_BLOCK_SIZE_BYTES {
            return Err(VirtioMemError::BlockSizeTooSmall {
                actual: self.block_size_bytes,
                minimum: MIN_BLOCK_SIZE_BYTES,
            });
        }
        if !self.block_size_bytes.is_power_of_two() {
            return Err(VirtioMemError::BlockSizeNotPowerOfTwo(
                self.block_size_bytes,
            ));
        }
        if self.size_bytes < self.block_size_bytes
            || !self.size_bytes.is_multiple_of(self.block_size_bytes)
        {
            return Err(VirtioMemError::SizeNotAligned {
                size: self.size_bytes,
                block: self.block_size_bytes,
            });
        }
        self.validate_value(self.requested_bytes, "requested")?;
        self.validate_value(self.current_bytes, "current")
    }

    pub fn validate_target(self, target_bytes: u64) -> Result<(), VirtioMemError> {
        self.validate()?;
        self.validate_value(target_bytes, "target")?;
        if self.size_bytes.saturating_sub(target_bytes) < MIN_HEADROOM_BYTES {
            return Err(VirtioMemError::TargetLacksHeadroom {
                target: target_bytes,
                size: self.size_bytes,
                minimum_headroom: MIN_HEADROOM_BYTES,
            });
        }
        Ok(())
    }

    fn validate_value(self, value_bytes: u64, name: &'static str) -> Result<(), VirtioMemError> {
        if value_bytes == 0 || value_bytes > self.size_bytes {
            return Err(VirtioMemError::ValueOutsideSize {
                name,
                value: value_bytes,
                size: self.size_bytes,
            });
        }
        if !value_bytes.is_multiple_of(self.block_size_bytes) {
            return Err(VirtioMemError::ValueNotAligned {
                name,
                value: value_bytes,
                block: self.block_size_bytes,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const BLOCK: u64 = 2 * 1024 * 1024;
    const GIB: u64 = 1 << 30;
    fn valid_state() -> VirtioMemState {
        VirtioMemState {
            size_bytes: 8 * GIB,
            block_size_bytes: BLOCK,
            requested_bytes: 4 * GIB,
            current_bytes: 4 * GIB,
        }
    }
    #[test]
    fn validates_state_and_target() {
        assert!(valid_state().validate_target(6 * GIB).is_ok());
    }
    #[test]
    fn rejects_invalid_values() {
        assert!(matches!(
            VirtioMemState {
                block_size_bytes: MIN_BLOCK_SIZE_BYTES - 1,
                ..valid_state()
            }
            .validate(),
            Err(VirtioMemError::BlockSizeTooSmall { .. })
        ));
        assert!(matches!(
            VirtioMemState {
                requested_bytes: BLOCK + 1,
                ..valid_state()
            }
            .validate(),
            Err(VirtioMemError::ValueNotAligned {
                name: "requested",
                ..
            })
        ));
        assert!(matches!(
            valid_state().validate_target(0),
            Err(VirtioMemError::ValueOutsideSize { name: "target", .. })
        ));
    }
    #[test]
    fn rejects_targets_that_would_consume_the_full_device() {
        assert!(matches!(
            valid_state().validate_target(8 * GIB),
            Err(VirtioMemError::TargetLacksHeadroom { .. })
        ));
        assert!(matches!(
            valid_state().validate_target(8 * GIB - MIN_HEADROOM_BYTES + BLOCK),
            Err(VirtioMemError::TargetLacksHeadroom { .. })
        ));
        assert!(valid_state()
            .validate_target(8 * GIB - MIN_HEADROOM_BYTES)
            .is_ok());
    }

    #[test]
    fn rejects_device_sizes_that_cannot_be_represented_in_blocks() {
        assert!(matches!(
            VirtioMemState {
                size_bytes: 8 * GIB + 1,
                ..valid_state()
            }
            .validate(),
            Err(VirtioMemError::SizeNotAligned { .. })
        ));
        assert!(matches!(
            VirtioMemState {
                size_bytes: BLOCK - 1,
                ..valid_state()
            }
            .validate(),
            Err(VirtioMemError::SizeNotAligned { .. })
        ));
    }

    #[test]
    fn rejects_maximum_values_that_break_alignment() {
        assert!(matches!(
            VirtioMemState {
                size_bytes: u64::MAX,
                block_size_bytes: BLOCK,
                requested_bytes: BLOCK,
                current_bytes: BLOCK,
            }
            .validate(),
            Err(VirtioMemError::SizeNotAligned { .. })
        ));
    }
}
