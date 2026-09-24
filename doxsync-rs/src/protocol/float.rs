//! Shared lossless floating-point width selection for encoding and cost planning.

/// Protocols before 2 always use binary64. NaNs retain their original payload.
pub(crate) fn payload_width(value: f64, protocol: u32) -> usize {
    if protocol < 2 || value.is_nan() {
        return 8;
    }
    if half::f16::from_f64(value).to_f64().to_bits() == value.to_bits() {
        2
    } else if ((value as f32) as f64).to_bits() == value.to_bits() {
        4
    } else {
        8
    }
}
