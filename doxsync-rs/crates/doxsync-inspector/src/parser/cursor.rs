use crate::{Error, Result};

/// Owns the only byte position. No protocol state or rendering decisions live here.
pub(super) struct Cursor<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) pos: usize,
}

impl<'a> Cursor<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    pub(super) fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| Error::new(self.pos, "Length overflow"))?;
        let bytes = self.bytes.get(self.pos..end).ok_or_else(|| {
            Error::new(self.pos, format!("Truncated field ({len} bytes required)"))
        })?;
        self.pos = end;
        Ok(bytes)
    }
}
