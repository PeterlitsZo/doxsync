mod decoder;
mod encoder;

pub(super) use decoder::PackedMessageDecoder;
pub(super) use encoder::PackedMessageEncoder;

/// A packed doxsync message.
pub struct PackedMessage {
    /// The packed message bytes.
    inner: Vec<u8>,
}

impl PackedMessage {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            inner: bytes.to_vec(),
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.inner
    }
}
