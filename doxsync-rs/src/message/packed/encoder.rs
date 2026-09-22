use std::{collections::BTreeMap, sync::Arc};

use super::PackedMessage;
use crate::message::Message;
use crate::patch::{Action, Path, PathSegment};
use crate::protocol::{
    PoolPreparation,
    consts::*,
    layout::{action_tag, payload_width, tstr_ref_key},
};
use crate::state::ProducerStateTxn;
use crate::{Error, ErrorKind, Result, Value};

/// An encoder for packed doxsync messages.
pub(in crate::message) struct PackedMessageEncoder {
    actions_limit: usize,
}

impl Default for PackedMessageEncoder {
    fn default() -> Self {
        Self {
            actions_limit: DEFAULT_ACTIONS_LIMIT,
        }
    }
}

impl PackedMessageEncoder {
    pub(in crate::message) fn encode(
        &self,
        message: &Message,
        state_txn: &mut ProducerStateTxn,
    ) -> Result<PackedMessage> {
        let mut internal = PackedMessageEncoderInternal {
            actions: message.actions(),
            state_txn,
            actions_limit: self.actions_limit,
        };
        let mut bytes = Vec::new();
        let savepoint = internal.state_txn.savepoint();
        if let Err(error) = internal.encode(&mut bytes) {
            internal.state_txn.rollback_to(savepoint);
            return Err(error);
        }
        Ok(PackedMessage { inner: bytes })
    }
}

struct PackedMessageEncoderInternal<'a, 's> {
    actions: &'a [Action],
    state_txn: &'s mut ProducerStateTxn,
    actions_limit: usize,
}

impl<'a, 's> PackedMessageEncoderInternal<'a, 's> {
    fn encode(&mut self, bytes: &mut Vec<u8>) -> Result<()> {
        // Encode the metadata.
        self.encode_metadata(bytes)?;

        // Now we pack the actions.
        self.pack_varuint(bytes, self.actions.len() as u64);
        for action in self.actions {
            self.pack_varuint(bytes, action_tag(action));
            match action {
                Action::Snapshot { value } => {
                    self.pack_value(bytes, value);
                }
                Action::Add { path, value } => {
                    self.pack_path_by_key(bytes, &Arc::new(path.clone()))?;
                    self.pack_value(bytes, value);
                }
                Action::Replace { path, value } => {
                    self.pack_path_by_key(bytes, &Arc::new(path.clone()))?;
                    self.pack_value(bytes, value);
                }
                Action::Delete { path } => {
                    self.pack_path_by_key(bytes, &Arc::new(path.clone()))?;
                }
                Action::Copy { path, from } => {
                    self.pack_path_by_key(bytes, &Arc::new(path.clone()))?;
                    self.pack_path_by_key(bytes, &Arc::new(from.clone()))?;
                }
            }
        }

        Ok(())
    }

    fn encode_metadata(&mut self, bytes: &mut Vec<u8>) -> Result<()> {
        let mut preparation = PoolPreparation::new(self.state_txn, self.actions_limit);
        for action in self.actions {
            preparation.append(action)?;
        }
        let prepared = preparation.finish();
        let string_patch = prepared.strings;
        let path_patch = prepared.paths;

        self.pack_varuint(
            bytes,
            u64::from(!string_patch.is_empty()) + u64::from(!path_patch.is_empty()),
        );

        if !string_patch.is_empty() {
            self.pack_varuint(bytes, METADATA_STRINGS);
            self.pack_varuint(bytes, string_patch.len() as u64);
            for (key, string) in string_patch {
                self.pack_varuint(bytes, key as u64);
                self.pack_varuint(bytes, string.len() as u64);
                bytes.extend_from_slice(string.as_bytes());
            }
        }

        if !path_patch.is_empty() {
            self.pack_varuint(bytes, METADATA_PATHS);
            self.pack_varuint(bytes, path_patch.len() as u64);
            for (key, path) in path_patch {
                self.pack_varuint(bytes, key as u64);
                self.pack_path(bytes, &path);
            }
        }

        Ok(())
    }

    fn pack_path_by_key(&mut self, bytes: &mut Vec<u8>, path: &Arc<Path>) -> Result<()> {
        let key = self
            .state_txn
            .get_path_key(path)
            .ok_or_else(|| Error::new(ErrorKind::Internal, "prepared path missing"))?;
        self.pack_varuint(bytes, key as u64);
        Ok(())
    }

    fn pack_varuint(&mut self, bytes: &mut Vec<u8>, value: u64) {
        let mut value = value;
        let mut buf = [0u8; 10];
        let mut i = 0;

        if value == 0 {
            bytes.push(0);
            return;
        }
        while value > 0 {
            buf[i] = (value & 0x7F) as u8;
            if value > 0x7F {
                buf[i] |= 0x80;
            }
            value >>= 7;
            i += 1;
        }

        bytes.extend_from_slice(&buf[..i]);
    }

    fn pack_value(&mut self, bytes: &mut Vec<u8>, value: &Value) {
        use crate::ValueInner;

        match value.inner() {
            ValueInner::PosInt { inner } => self.pack_posint(bytes, *inner),
            ValueInner::NegInt { inner } => self.pack_negint(bytes, *inner),
            ValueInner::Null => self.pack_null(bytes),
            ValueInner::Bool { inner } => self.pack_bool(bytes, *inner),
            ValueInner::Float { inner } => self.pack_float(bytes, *inner),
            ValueInner::BStr { inner } => self.pack_bstr(bytes, inner),
            ValueInner::TStr { inner } => self.pack_tstr(bytes, inner),
            ValueInner::Array { inner } => self.pack_array(bytes, inner),
            ValueInner::Map { inner } => self.pack_map(bytes, inner),
        }
    }

    fn pack_posint(&mut self, bytes: &mut Vec<u8>, value: u64) {
        let width = payload_width(value, posint::INLINE);
        if width == 0 {
            // Pack small values directly as bytes.
            bytes.push((TAG_POSINT << TAG_WIDTH) | (value as u8));
            return;
        } else if width == 1 {
            // Pack 1-byte values with type indicator.
            bytes.push((TAG_POSINT << TAG_WIDTH) | posint::BITS_8);
            bytes.push(value as u8);
            return;
        } else if width == 2 {
            // Pack 2-byte values with type indicator.
            bytes.push((TAG_POSINT << TAG_WIDTH) | posint::BITS_16);
            bytes.extend_from_slice(&(value as u16).to_le_bytes());
            return;
        } else if width == 4 {
            // Pack 4-byte values with type indicator.
            bytes.push((TAG_POSINT << TAG_WIDTH) | posint::BITS_32);
            bytes.extend_from_slice(&(value as u32).to_le_bytes());
            return;
        } else {
            // Pack 8-byte values with type indicator.
            bytes.push((TAG_POSINT << TAG_WIDTH) | posint::BITS_64);
            bytes.extend_from_slice(&value.to_le_bytes());
            return;
        }
    }

    fn pack_negint(&mut self, bytes: &mut Vec<u8>, value: u64) {
        let width = payload_width(value, negint::INLINE);
        if width == 0 {
            // Pack small values directly as bytes.
            bytes.push((TAG_NEGINT << TAG_WIDTH) | (value as u8));
            return;
        } else if width == 1 {
            // Pack 1-byte values with type indicator.
            bytes.push((TAG_NEGINT << TAG_WIDTH) | negint::BITS_8);
            bytes.push(value as u8);
            return;
        } else if width == 2 {
            // Pack 2-byte values with type indicator.
            bytes.push((TAG_NEGINT << TAG_WIDTH) | negint::BITS_16);
            bytes.extend_from_slice(&(value as u16).to_le_bytes());
            return;
        } else if width == 4 {
            // Pack 4-byte values with type indicator.
            bytes.push((TAG_NEGINT << TAG_WIDTH) | negint::BITS_32);
            bytes.extend_from_slice(&(value as u32).to_le_bytes());
            return;
        } else {
            // Pack 8-byte values with type indicator.
            bytes.push((TAG_NEGINT << TAG_WIDTH) | negint::BITS_64);
            bytes.extend_from_slice(&value.to_le_bytes());
            return;
        }
    }

    fn pack_bstr(&mut self, bytes: &mut Vec<u8>, value: &[u8]) {
        let value_len = value.len();

        let width = payload_width(value_len as u64, bstr::INLINE);
        if width == 0 {
            bytes.push((TAG_BSTR << TAG_WIDTH) | (value_len as u8));
        } else if width == 1 {
            bytes.push((TAG_BSTR << TAG_WIDTH) | bstr::BITS_8);
            bytes.extend_from_slice(&(value_len as u8).to_le_bytes());
        } else if width == 2 {
            bytes.push((TAG_BSTR << TAG_WIDTH) | bstr::BITS_16);
            bytes.extend_from_slice(&(value_len as u16).to_le_bytes());
        } else if width == 4 {
            bytes.push((TAG_BSTR << TAG_WIDTH) | bstr::BITS_32);
            bytes.extend_from_slice(&(value_len as u32).to_le_bytes());
        } else {
            bytes.push((TAG_BSTR << TAG_WIDTH) | bstr::BITS_64);
            bytes.extend_from_slice(&(value_len as u64).to_le_bytes());
        }

        bytes.extend_from_slice(value);
    }

    fn pack_tstr(&mut self, bytes: &mut Vec<u8>, value: &Arc<String>) {
        if let Some(key) = tstr_ref_key(value.len(), self.state_txn.get_string_key(value)) {
            let width = payload_width(key as u64, posint::INLINE);
            if width == 0 {
                bytes.push((TAG_TSTR_REF << TAG_WIDTH) | key as u8);
            } else {
                let payload = match width {
                    1 => posint::BITS_8,
                    2 => posint::BITS_16,
                    _ => posint::BITS_32,
                };
                bytes.push((TAG_TSTR_REF << TAG_WIDTH) | payload);
                bytes.extend_from_slice(&key.to_le_bytes()[..width]);
            }
            return;
        }

        let value = value.as_bytes();
        let value_len = value.len();

        let width = payload_width(value_len as u64, tstr::INLINE);
        if width == 0 {
            bytes.push((TAG_TSTR << TAG_WIDTH) | (value_len as u8));
        } else if width == 1 {
            bytes.push((TAG_TSTR << TAG_WIDTH) | tstr::BITS_8);
            bytes.extend_from_slice(&(value_len as u8).to_le_bytes());
        } else if width == 2 {
            bytes.push((TAG_TSTR << TAG_WIDTH) | tstr::BITS_16);
            bytes.extend_from_slice(&(value_len as u16).to_le_bytes());
        } else if width == 4 {
            bytes.push((TAG_TSTR << TAG_WIDTH) | tstr::BITS_32);
            bytes.extend_from_slice(&(value_len as u32).to_le_bytes());
        } else {
            bytes.push((TAG_TSTR << TAG_WIDTH) | tstr::BITS_64);
            bytes.extend_from_slice(&(value_len as u64).to_le_bytes());
        }

        bytes.extend_from_slice(value);
    }

    fn pack_float(&mut self, bytes: &mut Vec<u8>, value: f64) {
        bytes.push((TAG_FLOAT << TAG_WIDTH) | float::BITS_64);
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn pack_null(&mut self, bytes: &mut Vec<u8>) {
        bytes.push((TAG_FLOAT << TAG_WIDTH) | float::NULL);
    }

    fn pack_bool(&mut self, bytes: &mut Vec<u8>, value: bool) {
        let payload = if value { float::TRUE } else { float::FALSE };
        bytes.push((TAG_FLOAT << TAG_WIDTH) | payload);
    }

    fn pack_array(&mut self, bytes: &mut Vec<u8>, value: &[Value]) {
        let value_len = value.len();

        let width = payload_width(value_len as u64, array::INLINE);
        if width == 0 {
            bytes.push((TAG_ARRAY << TAG_WIDTH) | (value_len as u8));
        } else if width == 1 {
            bytes.push((TAG_ARRAY << TAG_WIDTH) | array::BITS_8);
            bytes.extend_from_slice(&(value_len as u8).to_le_bytes());
        } else if width == 2 {
            bytes.push((TAG_ARRAY << TAG_WIDTH) | array::BITS_16);
            bytes.extend_from_slice(&(value_len as u16).to_le_bytes());
        } else if width == 4 {
            bytes.push((TAG_ARRAY << TAG_WIDTH) | array::BITS_32);
            bytes.extend_from_slice(&(value_len as u32).to_le_bytes());
        } else {
            bytes.push((TAG_ARRAY << TAG_WIDTH) | array::BITS_64);
            bytes.extend_from_slice(&(value_len as u64).to_le_bytes());
        }

        for value in value {
            self.pack_value(bytes, value);
        }
    }

    fn pack_map(&mut self, bytes: &mut Vec<u8>, value: &BTreeMap<Arc<String>, Value>) {
        let value_len = value.len();

        let width = payload_width(value_len as u64, map::INLINE);
        if width == 0 {
            bytes.push((TAG_MAP << TAG_WIDTH) | (value_len as u8));
        } else if width == 1 {
            bytes.push((TAG_MAP << TAG_WIDTH) | map::BITS_8);
            bytes.extend_from_slice(&(value_len as u8).to_le_bytes());
        } else if width == 2 {
            bytes.push((TAG_MAP << TAG_WIDTH) | map::BITS_16);
            bytes.extend_from_slice(&(value_len as u16).to_le_bytes());
        } else if width == 4 {
            bytes.push((TAG_MAP << TAG_WIDTH) | map::BITS_32);
            bytes.extend_from_slice(&(value_len as u32).to_le_bytes());
        } else {
            bytes.push((TAG_MAP << TAG_WIDTH) | map::BITS_64);
            bytes.extend_from_slice(&(value_len as u64).to_le_bytes());
        }

        for (key, value) in value.iter() {
            if let Some(key) = self.state_txn.get_string_key(key) {
                self.pack_varuint(bytes, (key as u64) << 2 | 0b00);
            } else {
                self.pack_varuint(bytes, (key.len() as u64) << 2 | 0b01);
                bytes.extend_from_slice(key.as_bytes());
            }
            self.pack_value(bytes, value);
        }
    }

    fn pack_path(&mut self, bytes: &mut Vec<u8>, path: &Path) {
        self.pack_varuint(bytes, path.segments().len() as u64);
        for segment in path.segments() {
            match segment {
                PathSegment::Key(key) => {
                    self.pack_varuint(bytes, (key.len() as u64) << 2 | 0b00);
                    bytes.extend_from_slice(key.as_bytes());
                }
                PathSegment::Index(index) => {
                    self.pack_varuint(bytes, (*index as u64) << 2 | 0b01);
                }
            }
        }
    }
}
