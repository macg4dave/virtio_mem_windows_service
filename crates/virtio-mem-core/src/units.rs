//! Checked conversions at external memory-unit boundaries.

pub const BYTES_PER_KIB: u64 = 1 << 10;

/// Convert an external KiB value to the canonical byte unit.
pub fn kibibytes_to_bytes(value_kib: u64) -> Option<u64> {
    value_kib.checked_mul(BYTES_PER_KIB)
}
/// Convert canonical bytes to an exact external KiB value.
pub fn bytes_to_kibibytes(value_bytes: u64) -> Option<u64> {
    value_bytes
        .is_multiple_of(BYTES_PER_KIB)
        .then_some(value_bytes / BYTES_PER_KIB)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_kibibytes_with_checked_arithmetic() {
        assert_eq!(kibibytes_to_bytes(2), Some(2048));
        assert_eq!(kibibytes_to_bytes(u64::MAX), None);
    }

    #[test]
    fn converts_only_exact_byte_values_to_kibibytes() {
        assert_eq!(bytes_to_kibibytes(2048), Some(2));
        assert_eq!(bytes_to_kibibytes(2047), None);
        assert_eq!(bytes_to_kibibytes(u64::MAX), None);
    }

    #[test]
    fn exact_round_trip_preserves_maximum_representable_kibibytes() {
        let value = u64::MAX / BYTES_PER_KIB;
        assert_eq!(
            bytes_to_kibibytes(kibibytes_to_bytes(value).unwrap()),
            Some(value)
        );
    }
}
