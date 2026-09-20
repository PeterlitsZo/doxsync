use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use super::{PackedMessage, consts::*};
use crate::message::{Action, Message, Path, PathSegment};
use crate::state::{
    InsertPathResult, InsertStringPoolResult, PATH_KEY_BYTES_LIMIT, PATH_PATCH_BYTES_LIMIT,
    PATH_POOL_CAPACITY, PATH_SEGMENTS_LIMIT, ProducerStateTxn, STRING_POOL_CAPACITY,
};
use crate::{Error, ErrorKind, Result, Value, ValueKind};

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

struct MessageMetadata {
    strings: Vec<Arc<String>>,
    paths: Vec<Arc<Path>>,
}

impl MessageMetadata {
    fn validate_path(path: &Path) -> Result<()> {
        if path.segments().len() > PATH_SEGMENTS_LIMIT {
            return Err(Error::new(ErrorKind::InvalidData, "too many path segments"));
        }
        let mut remaining = PATH_KEY_BYTES_LIMIT;
        for segment in path.segments() {
            match segment {
                PathSegment::Key(key) => {
                    remaining = remaining
                        .checked_sub(key.len())
                        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "path keys too large"))?;
                }
                PathSegment::Index(index) => {
                    if u64::try_from(*index).map_or(true, |value| value > u64::MAX >> 2) {
                        return Err(Error::new(ErrorKind::InvalidData, "path index too large"));
                    }
                }
            }
        }
        Ok(())
    }

    fn collect(
        actions: &[Action],
        state_txn: &ProducerStateTxn,
        actions_limit: usize,
    ) -> Result<Self> {
        fn collect_strings(
            value: &Value,
            seen: &mut BTreeSet<Arc<String>>,
            strings: &mut Vec<Arc<String>>,
        ) -> Result<()> {
            match value.kind() {
                ValueKind::Array => {
                    for value in value.as_array().expect("array") {
                        collect_strings(value, seen, strings)?;
                    }
                }
                ValueKind::Map => {
                    for (key, value) in value.as_map().expect("map") {
                        if seen.insert(key.clone()) {
                            if seen.len() > STRING_POOL_CAPACITY {
                                return Err(Error::new(
                                    ErrorKind::InvalidData,
                                    "too many distinct map keys",
                                ));
                            }
                            strings.push(key.clone());
                        }
                        collect_strings(value, seen, strings)?;
                    }
                }
                _ => {}
            }
            Ok(())
        }

        if actions.len() > actions_limit {
            return Err(Error::new(ErrorKind::InvalidData, "too many actions"));
        }

        let mut strings = Vec::new();
        let mut string_seen = BTreeSet::new();
        let mut paths = Vec::new();
        let mut path_seen = BTreeSet::new();
        for action in actions {
            match action {
                Action::Snapshot { value } => {
                    collect_strings(value, &mut string_seen, &mut strings)?
                }
                Action::Add { path, value } | Action::Replace { path, value } => {
                    collect_strings(value, &mut string_seen, &mut strings)?;
                    if path_seen.insert(path) {
                        paths.push(Arc::new(path.clone()));
                    }
                }
                Action::Delete { path } => {
                    if path_seen.insert(path) {
                        paths.push(Arc::new(path.clone()));
                    }
                }
                Action::Copy { path, from } => {
                    if path_seen.insert(path) {
                        paths.push(Arc::new(path.clone()));
                    }
                    if path_seen.insert(from) {
                        paths.push(Arc::new(from.clone()));
                    }
                }
            }
            if paths.len() > PATH_POOL_CAPACITY {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "too many distinct paths",
                ));
            }
        }
        // Validate all paths, including hits, before changing either pool.
        for path in &paths {
            Self::validate_path(path)?;
        }
        // Measure only path definitions needed by the resource limit. No bytes
        // are allocated and existing pool entries are not touched.
        let mut definition_bytes = 0usize;
        for path in &paths {
            if state_txn.get_path_key(path).is_none() {
                let mut len = varuint_len(path.segments().len() as u64);
                for segment in path.segments() {
                    len += match segment {
                        PathSegment::Key(key) => varuint_len((key.len() as u64) << 2) + key.len(),
                        PathSegment::Index(index) => varuint_len((*index as u64) << 2 | 1),
                    };
                }
                definition_bytes = definition_bytes
                    .checked_add(len)
                    .ok_or_else(|| Error::new(ErrorKind::InvalidData, "path patch too large"))?;
                if definition_bytes > PATH_PATCH_BYTES_LIMIT {
                    return Err(Error::new(ErrorKind::InvalidData, "path patch too large"));
                }
            }
        }
        Ok(Self { strings, paths })
    }
}

fn varuint_len(value: u64) -> usize {
    ((64 - value.leading_zeros()) as usize).max(1).div_ceil(7)
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
            match action {
                Action::Snapshot { value } => {
                    self.pack_varuint(bytes, ACTION_SNAPSHOT as u64);
                    self.pack_value(bytes, value);
                }
                Action::Add { path, value } => {
                    self.pack_varuint(bytes, ACTION_ADD as u64);
                    self.pack_path_by_key(bytes, &Arc::new(path.clone()))?;
                    self.pack_value(bytes, value);
                }
                Action::Replace { path, value } => {
                    self.pack_varuint(bytes, ACTION_REPLACE as u64);
                    self.pack_path_by_key(bytes, &Arc::new(path.clone()))?;
                    self.pack_value(bytes, value);
                }
                Action::Delete { path } => {
                    self.pack_varuint(bytes, ACTION_DELETE as u64);
                    self.pack_path_by_key(bytes, &Arc::new(path.clone()))?;
                }
                Action::Copy { path, from } => {
                    self.pack_varuint(bytes, ACTION_COPY as u64);
                    self.pack_path_by_key(bytes, &Arc::new(path.clone()))?;
                    self.pack_path_by_key(bytes, &Arc::new(from.clone()))?;
                }
            }
        }

        Ok(())
    }

    fn encode_metadata(&mut self, bytes: &mut Vec<u8>) -> Result<()> {
        let MessageMetadata { strings, paths } =
            MessageMetadata::collect(self.actions, self.state_txn, self.actions_limit)?;

        let mut string_patch = Vec::new();
        for string in strings {
            self.state_txn.hit_string_pool_if_exists(&string);
            if self.state_txn.get_string_key(&string).is_none() {
                match self.state_txn.insert_string_pool(&string) {
                    InsertStringPoolResult::Inserted { key }
                    | InsertStringPoolResult::Replaced { key } => string_patch.push((key, string)),
                    InsertStringPoolResult::Existing { .. } => unreachable!("new string"),
                }
            }
        }

        let mut path_patch = Vec::new();
        for path in paths {
            self.state_txn.hit_path_pool_if_exists(&path);
            if self.state_txn.get_path_key(&path).is_none() {
                let mut definition = Vec::new();
                self.pack_path(&mut definition, &path);
                match self.state_txn.insert_path_pool(&path) {
                    InsertPathResult::Inserted { key } | InsertPathResult::Replaced { key } => {
                        path_patch.push((key, definition))
                    }
                    InsertPathResult::Existing { .. } => unreachable!("new path"),
                }
            }
        }

        self.pack_varuint(
            bytes,
            u64::from(!string_patch.is_empty()) + u64::from(!path_patch.is_empty()),
        );

        if !string_patch.is_empty() {
            self.pack_varuint(bytes, 0);
            self.pack_varuint(bytes, string_patch.len() as u64);
            for (key, string) in string_patch {
                self.pack_varuint(bytes, key as u64);
                self.pack_varuint(bytes, string.len() as u64);
                bytes.extend_from_slice(string.as_bytes());
            }
        }

        if !path_patch.is_empty() {
            self.pack_varuint(bytes, 1);
            self.pack_varuint(bytes, path_patch.len() as u64);
            for (key, definition) in path_patch {
                self.pack_varuint(bytes, key as u64);
                bytes.extend_from_slice(&definition);
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
        if value <= posint::INLINE as u64 {
            // Pack small values directly as bytes.
            bytes.push((TAG_POSINT << TAG_WIDTH) | (value as u8));
            return;
        } else if value < (1 << 8) {
            // Pack 1-byte values with type indicator.
            bytes.push((TAG_POSINT << TAG_WIDTH) | posint::BITS_8);
            bytes.push(value as u8);
            return;
        } else if value < (1 << 16) {
            // Pack 2-byte values with type indicator.
            bytes.push((TAG_POSINT << TAG_WIDTH) | posint::BITS_16);
            bytes.extend_from_slice(&(value as u16).to_le_bytes());
            return;
        } else if value < (1 << 32) {
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
        if value <= negint::INLINE as u64 {
            // Pack small values directly as bytes.
            bytes.push((TAG_NEGINT << TAG_WIDTH) | (value as u8));
            return;
        } else if value < (1 << 8) {
            // Pack 1-byte values with type indicator.
            bytes.push((TAG_NEGINT << TAG_WIDTH) | negint::BITS_8);
            bytes.push(value as u8);
            return;
        } else if value < (1 << 16) {
            // Pack 2-byte values with type indicator.
            bytes.push((TAG_NEGINT << TAG_WIDTH) | negint::BITS_16);
            bytes.extend_from_slice(&(value as u16).to_le_bytes());
            return;
        } else if value < (1 << 32) {
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

        if value_len <= bstr::INLINE as usize {
            bytes.push((TAG_BSTR << TAG_WIDTH) | (value_len as u8));
        } else if value_len < (1 << 8) {
            bytes.push((TAG_BSTR << TAG_WIDTH) | bstr::BITS_8);
            bytes.extend_from_slice(&(value_len as u8).to_le_bytes());
        } else if value_len < (1 << 16) {
            bytes.push((TAG_BSTR << TAG_WIDTH) | bstr::BITS_16);
            bytes.extend_from_slice(&(value_len as u16).to_le_bytes());
        } else if value_len < (1 << 32) {
            bytes.push((TAG_BSTR << TAG_WIDTH) | bstr::BITS_32);
            bytes.extend_from_slice(&(value_len as u32).to_le_bytes());
        } else {
            bytes.push((TAG_BSTR << TAG_WIDTH) | bstr::BITS_64);
            bytes.extend_from_slice(&(value_len as u64).to_le_bytes());
        }

        bytes.extend_from_slice(value);
    }

    fn pack_tstr(&mut self, bytes: &mut Vec<u8>, value: &str) {
        let value = value.as_bytes();
        let value_len = value.len();

        if value_len <= tstr::INLINE as usize {
            bytes.push((TAG_TSTR << TAG_WIDTH) | (value_len as u8));
        } else if value_len < (1 << 8) {
            bytes.push((TAG_TSTR << TAG_WIDTH) | tstr::BITS_8);
            bytes.extend_from_slice(&(value_len as u8).to_le_bytes());
        } else if value_len < (1 << 16) {
            bytes.push((TAG_TSTR << TAG_WIDTH) | tstr::BITS_16);
            bytes.extend_from_slice(&(value_len as u16).to_le_bytes());
        } else if value_len < (1 << 32) {
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

        if value_len <= array::INLINE as usize {
            bytes.push((TAG_ARRAY << TAG_WIDTH) | (value_len as u8));
        } else if value_len < (1 << 8) {
            bytes.push((TAG_ARRAY << TAG_WIDTH) | array::BITS_8);
            bytes.extend_from_slice(&(value_len as u8).to_le_bytes());
        } else if value_len < (1 << 16) {
            bytes.push((TAG_ARRAY << TAG_WIDTH) | array::BITS_16);
            bytes.extend_from_slice(&(value_len as u16).to_le_bytes());
        } else if value_len < (1 << 32) {
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

        if value_len <= map::INLINE as usize {
            bytes.push((TAG_MAP << TAG_WIDTH) | (value_len as u8));
        } else if value_len < (1 << 8) {
            bytes.push((TAG_MAP << TAG_WIDTH) | map::BITS_8);
            bytes.extend_from_slice(&(value_len as u8).to_le_bytes());
        } else if value_len < (1 << 16) {
            bytes.push((TAG_MAP << TAG_WIDTH) | map::BITS_16);
            bytes.extend_from_slice(&(value_len as u16).to_le_bytes());
        } else if value_len < (1 << 32) {
            bytes.push((TAG_MAP << TAG_WIDTH) | map::BITS_32);
            bytes.extend_from_slice(&(value_len as u32).to_le_bytes());
        } else {
            bytes.push((TAG_MAP << TAG_WIDTH) | map::BITS_64);
            bytes.extend_from_slice(&(value_len as u64).to_le_bytes());
        }

        for (key, value) in value.iter() {
            let key = self
                .state_txn
                .get_string_key(key)
                .expect("string key not found");
            self.pack_varuint(bytes, key as u64);
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
