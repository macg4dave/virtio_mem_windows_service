use crate::error::VirtioMemError;

pub const MIN_BLOCK_SIZE_BYTES: u64 = 1 << 20;

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
        self.validate_value(self.requested_bytes, "requested")?;
        self.validate_value(self.current_bytes, "current")
    }

    pub fn validate_target(self, target_bytes: u64) -> Result<(), VirtioMemError> {
        self.validate()?;
        self.validate_value(target_bytes, "target")
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
    fn valid_state() -> VirtioMemState {
        VirtioMemState {
            size_bytes: 8 * 1024 * 1024,
            block_size_bytes: BLOCK,
            requested_bytes: 4 * 1024 * 1024,
            current_bytes: 4 * 1024 * 1024,
        }
    }
    #[test]
    fn validates_state_and_target() {
        assert!(valid_state().validate_target(6 * 1024 * 1024).is_ok());
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
}
