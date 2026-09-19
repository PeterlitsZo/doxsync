use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::message::{Action, Message, Path, PathSegment};
use crate::state::{
    ConsumerStateTxn, InsertPathResult, InsertStringResult, PATH_KEY_BYTES_LIMIT,
    PATH_PATCH_BYTES_LIMIT, PATH_POOL_CAPACITY, PATH_SEGMENTS_LIMIT, ProducerStateTxn,
    STRING_POOL_CAPACITY,
};
use crate::{Error, ErrorKind, Result, Value, ValueKind};

const TAG_POSINT: u8 = 0b0000;
const TAG_NEGINT: u8 = 0b0001;
const TAG_BSTR: u8 = 0b0010;
const TAG_TSTR: u8 = 0b0011;
const TAG_ARRAY: u8 = 0b0100;
const TAG_MAP: u8 = 0b0101;
const TAG_FLOAT: u8 = 0b0111;

const TAG_WIDTH: usize = 4;
const PAYLOAD_MASK: u8 = 0x0F;

const ACTION_SNAPSHOT: u8 = 0;
const ACTION_ADD: u8 = 1;
const ACTION_DELETE: u8 = 2;

mod posint {
    pub(crate) const INLINE: u8 = 11;
    pub(crate) const BITS_8: u8 = 12;
    pub(crate) const BITS_16: u8 = 13;
    pub(crate) const BITS_32: u8 = 14;
    pub(crate) const BITS_64: u8 = 15;
}

mod negint {
    pub(crate) const INLINE: u8 = 11;
    pub(crate) const BITS_8: u8 = 12;
    pub(crate) const BITS_16: u8 = 13;
    pub(crate) const BITS_32: u8 = 14;
    pub(crate) const BITS_64: u8 = 15;
}

mod bstr {
    pub(crate) const INLINE: u8 = 11;
    pub(crate) const BITS_8: u8 = 12;
    pub(crate) const BITS_16: u8 = 13;
    pub(crate) const BITS_32: u8 = 14;
    pub(crate) const BITS_64: u8 = 15;
}

mod tstr {
    pub(crate) const INLINE: u8 = 11;
    pub(crate) const BITS_8: u8 = 12;
    pub(crate) const BITS_16: u8 = 13;
    pub(crate) const BITS_32: u8 = 14;
    pub(crate) const BITS_64: u8 = 15;
}

mod array {
    pub(crate) const INLINE: u8 = 11;
    pub(crate) const BITS_8: u8 = 12;
    pub(crate) const BITS_16: u8 = 13;
    pub(crate) const BITS_32: u8 = 14;
    pub(crate) const BITS_64: u8 = 15;
}

mod float {
    pub(crate) const FALSE: u8 = 4;
    pub(crate) const TRUE: u8 = 5;
    pub(crate) const NULL: u8 = 6;
    pub(crate) const BITS_64: u8 = 11;
}

mod map {
    pub(crate) const INLINE: u8 = 11;
    pub(crate) const BITS_8: u8 = 12;
    pub(crate) const BITS_16: u8 = 13;
    pub(crate) const BITS_32: u8 = 14;
    pub(crate) const BITS_64: u8 = 15;
}

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

/// A decoder for packed doxsync messages.
pub(super) struct PackedMessageDecoder {
    actions_limit: usize,
}

impl Default for PackedMessageDecoder {
    fn default() -> Self {
        Self {
            actions_limit: 65535,
        }
    }
}

impl PackedMessageDecoder {
    pub(super) fn decode(&self, bytes: &[u8], state_txn: &mut ConsumerStateTxn) -> Result<Message> {
        let savepoint = state_txn.savepoint();
        match self.decode_inner(bytes, state_txn) {
            Ok(message) => Ok(message),
            Err(error) => {
                state_txn.rollback_to(savepoint);
                Err(error)
            }
        }
    }

    fn decode_inner(&self, bytes: &[u8], state: &mut ConsumerStateTxn) -> Result<Message> {
        let mut bytes = bytes;
        let mut actions = Vec::new();

        // Unpack the metadata's length.
        let metadata_len = Self::unpack_varuint(&mut bytes)?;

        // Unpack and apply the metadata instructions.
        let mut seen = BTreeSet::new();
        for index in 0..metadata_len {
            let instruction = Self::unpack_varuint(&mut bytes)
                .map_err(|e| e.with_context("unpack metadata instruction"))?;
            if !seen.insert(instruction) {
                return Err(
                    Error::new(ErrorKind::InvalidData, "duplicate metadata instruction")
                        .with_metadata("metadata_index", index),
                );
            }
            match instruction {
                0 => {
                    let patch = Self::unpack_string_pool_patch(&mut bytes)
                        .map_err(|e| e.with_context("unpack string pool patch"))?;
                    state.apply_string_pool_patch(patch);
                }
                1 => {
                    let patch = Self::unpack_path_pool_patch(&mut bytes).map_err(|e| {
                        e.with_context("unpack path pool patch")
                            .with_metadata("metadata_index", index)
                    })?;
                    state.apply_path_pool_patch(patch);
                }
                _ => {
                    return Err(
                        Error::new(ErrorKind::InvalidData, "invalid metadata instruction")
                            .with_metadata("index", index)
                            .with_metadata("instruction", instruction),
                    );
                }
            }
        }

        // Unpack the actions.
        let actions_len = Self::unpack_varuint(&mut bytes)?;
        if actions_len > self.actions_limit as u64 {
            return Err(Error::new(ErrorKind::InvalidData, "actions too large"));
        }
        for index in 0..actions_len {
            let action = Self::unpack_action(&mut bytes, state)
                .map_err(|e| e.with_metadata("action_index", index))?;
            actions.push(action);
        }

        // Make sure we've consumed all the bytes.
        if !bytes.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "unexpected trailing bytes",
            ));
        }

        Ok(Message { actions })
    }

    fn unpack_string_pool_patch(bytes: &mut &[u8]) -> Result<Vec<(u32, Arc<String>)>> {
        fn unexpected_end_of_patch() -> Error {
            Error::new(
                ErrorKind::InvalidData,
                "unexpected end of string pool patch",
            )
        }

        let patch_len = Self::unpack_varuint(bytes)?;
        if patch_len > STRING_POOL_CAPACITY as u64 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "string pool patch too large",
            ));
        }
        let mut seen = BTreeSet::new();
        let mut patch = Vec::new();
        for index in 0..patch_len {
            let key = Self::unpack_varuint(bytes)
                .map_err(|e| e.with_context("unpack string pool key"))?;
            let key = u32::try_from(key).map_err(|_| {
                Error::new(ErrorKind::InvalidData, "string pool key too large")
                    .with_metadata("index", index)
            })?;
            if key as usize >= STRING_POOL_CAPACITY || !seen.insert(key) {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "invalid or duplicate string pool key",
                )
                .with_metadata("index", index));
            }
            let value_len = Self::unpack_varuint(bytes)
                .map_err(|e| e.with_context("unpack string pool value length"))?;
            let value_len = usize::try_from(value_len).map_err(|_| {
                Error::new(ErrorKind::InvalidData, "string pool value too large")
                    .with_metadata("index", index)
            })?;
            let value_bytes = bytes.get(..value_len).ok_or_else(|| {
                unexpected_end_of_patch()
                    .with_metadata("index", index)
                    .with_metadata("value_len", value_len)
            })?;
            let value = std::str::from_utf8(value_bytes)
                .map_err(|_| {
                    Error::new(ErrorKind::InvalidData, "invalid UTF-8 string pool value")
                        .with_metadata("index", index)
                })?
                .to_owned();
            *bytes = bytes.get(value_len..).ok_or_else(unexpected_end_of_patch)?;
            patch.push((key, Arc::new(value)));
        }

        Ok(patch)
    }

    fn unpack_path_pool_patch(bytes: &mut &[u8]) -> Result<Vec<(u32, Arc<Path>)>> {
        let count = Self::unpack_varuint(bytes)?;
        if count > PATH_POOL_CAPACITY as u64 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "path pool patch too large",
            ));
        }
        let mut patch = Vec::new();
        let mut seen = BTreeSet::new();
        let mut remaining = PATH_PATCH_BYTES_LIMIT;
        for index in 0..count {
            let id =
                Self::unpack_varuint(bytes).map_err(|e| e.with_metadata("entry_index", index))?;
            if id >= PATH_POOL_CAPACITY as u64 || !seen.insert(id) {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "invalid or duplicate path pool key",
                )
                .with_metadata("entry_index", index)
                .with_metadata("path_id", id));
            }
            // Restrict the input before parsing so oversized definitions cannot allocate first.
            let mut limited = &bytes[..bytes.len().min(remaining)];
            let before = limited.len();
            let path = Self::unpack_path(&mut limited).map_err(|e| {
                e.with_metadata("entry_index", index)
                    .with_metadata("path_id", id)
            })?;
            let used = before - limited.len();
            remaining -= used;
            *bytes = &bytes[used..];
            patch.push((id as u32, Arc::new(path)));
        }
        Ok(patch)
    }

    fn unpack_path_reference(bytes: &mut &[u8], state: &ConsumerStateTxn) -> Result<Path> {
        let id = Self::unpack_varuint(bytes)?;
        if id >= PATH_POOL_CAPACITY as u64 {
            return Err(Error::new(ErrorKind::InvalidData, "path id out of range")
                .with_metadata("path_id", id));
        }
        state
            .get_path(id as u32)
            .map(|path| path.as_ref().clone())
            .ok_or_else(|| {
                Error::new(ErrorKind::InvalidData, "undefined path id").with_metadata("path_id", id)
            })
    }

    fn unpack_varuint(bytes: &mut &[u8]) -> Result<u64> {
        let mut value: u128 = 0;
        let mut shift: u64 = 0;

        loop {
            // Read the next byte.
            let byte = bytes.get(0).ok_or(Error::new(
                ErrorKind::InvalidData,
                "unexpected end of varuint",
            ))?;
            *bytes = &bytes[1..];

            // Consume the byte.
            value |= (*byte as u128 & 0x7f) << shift;

            // Stop if the continuation bit is not set.
            if *byte & 0x80 == 0 {
                break;
            }

            // Check that we haven't overflowed the value and update state.
            if shift >= 64 {
                return Err(Error::new(ErrorKind::InvalidData, "varuint overflow"));
            }
            shift += 7;
        }

        // Be sure the value fits in a u64.
        if value > u64::MAX as u128 {
            return Err(Error::new(ErrorKind::InvalidData, "varuint overflow"));
        }

        Ok(value as u64)
    }

    fn unpack_action(bytes: &mut &[u8], state: &ConsumerStateTxn) -> Result<Action> {
        let action_type = Self::unpack_varuint(bytes)?;
        match action_type as u8 {
            ACTION_SNAPSHOT => {
                let value = Self::unpack_value(bytes, state)?;
                Ok(Action::Snapshot { value })
            }
            ACTION_ADD => {
                let path = Self::unpack_path_reference(bytes, state)?;
                let value = Self::unpack_value(bytes, state)?;
                Ok(Action::Add { path, value })
            }
            ACTION_DELETE => {
                let path = Self::unpack_path_reference(bytes, state)?;
                Ok(Action::Delete { path })
            }
            _ => Err(Error::new(ErrorKind::InvalidData, "invalid action type")),
        }
    }

    fn unpack_path(bytes: &mut &[u8]) -> Result<Path> {
        fn unexpected_end_of_path() -> Error {
            Error::new(ErrorKind::InvalidData, "unexpected end of path")
        }

        let segments_len = Self::unpack_varuint(bytes)?;
        if segments_len > PATH_SEGMENTS_LIMIT as u64 {
            return Err(Error::new(ErrorKind::InvalidData, "too many path segments"));
        }
        let mut key_bytes_remaining = PATH_KEY_BYTES_LIMIT;
        let mut segments = Vec::new();
        for index in 0..segments_len {
            let segment =
                Self::unpack_varuint(bytes).map_err(|e| e.with_context("unpack path segment"))?;
            let segment_value = segment >> 2;
            match segment & 0b11 {
                0b00 => {
                    let key_len = usize::try_from(segment_value).map_err(|_| {
                        Error::new(ErrorKind::InvalidData, "path key too large")
                            .with_metadata("index", index)
                    })?;
                    key_bytes_remaining = key_bytes_remaining
                        .checked_sub(key_len)
                        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "path keys too large"))?;
                    let key_bytes = bytes.get(..key_len).ok_or_else(|| {
                        unexpected_end_of_path()
                            .with_metadata("index", index)
                            .with_metadata("key_len", key_len)
                    })?;
                    let key = std::str::from_utf8(key_bytes)
                        .map_err(|_| {
                            Error::new(ErrorKind::InvalidData, "invalid UTF-8 path key")
                                .with_metadata("index", index)
                        })?
                        .to_owned();
                    *bytes = bytes.get(key_len..).ok_or_else(unexpected_end_of_path)?;
                    segments.push(PathSegment::key(key));
                }
                0b01 => {
                    let item_index = usize::try_from(segment_value).map_err(|_| {
                        Error::new(ErrorKind::InvalidData, "path index too large")
                            .with_metadata("index", index)
                    })?;
                    segments.push(PathSegment::index(item_index));
                }
                _ => {
                    return Err(
                        Error::new(ErrorKind::InvalidData, "invalid path segment type")
                            .with_metadata("index", index),
                    );
                }
            }
        }

        Ok(Path::new(segments))
    }

    #[rustfmt::skip]
    fn unpack_value(bytes: &mut &[u8], state: &ConsumerStateTxn) -> Result<Value> {
        // Get the first byte to determine the value type.
        let value_type = bytes.get(0).ok_or(Error::new(
            ErrorKind::InvalidData,
            "unexpected end of value",
        ))?;
        match value_type >> TAG_WIDTH {
            TAG_POSINT => Self::unpack_posint(bytes).map_err(|e| e.with_context("unpack posint")),
            TAG_NEGINT => Self::unpack_negint(bytes).map_err(|e| e.with_context("unpack negint")),
            TAG_BSTR => Self::unpack_bstr(bytes).map_err(|e| e.with_context("unpack binary string")),
            TAG_TSTR => Self::unpack_tstr(bytes).map_err(|e| e.with_context("unpack text string")),
            TAG_ARRAY => Self::unpack_array(bytes, state).map_err(|e| e.with_context("unpack array")),
            TAG_FLOAT => Self::unpack_simple_or_float(bytes).map_err(|e| e.with_context("unpack simple value or float")),
            TAG_MAP => Self::unpack_map(bytes, state).map_err(|e| e.with_context("unpack map")),
            _ => Err(Error::new(ErrorKind::InvalidData, "invalid value type")),
        }
    }

    fn unpack_posint(bytes: &mut &[u8]) -> Result<Value> {
        fn unexpected_end_of_value() -> Error {
            Error::new(ErrorKind::InvalidData, "unexpected end of value")
        }

        let first_byte = bytes.get(0).ok_or_else(|| {
            unexpected_end_of_value().with_metadata("cause", "unexpected end of value")
        })?;
        let first_byte = *first_byte & PAYLOAD_MASK;

        if first_byte <= posint::INLINE {
            *bytes = bytes
                .get(1..)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            return Ok(Value::inner_posint(first_byte as u64));
        } else if first_byte == posint::BITS_8 {
            let bytes_to_parse = bytes
                .get(1..2)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u8::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 1]) });
            *bytes = bytes.get(2..).ok_or_else(unexpected_end_of_value)?;
            Ok(Value::inner_posint(value as u64))
        } else if first_byte == posint::BITS_16 {
            let bytes_to_parse = bytes
                .get(1..3)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u16::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 2]) });
            *bytes = bytes.get(3..).ok_or_else(unexpected_end_of_value)?;
            Ok(Value::inner_posint(value as u64))
        } else if first_byte == posint::BITS_32 {
            let bytes_to_parse = bytes
                .get(1..5)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u32::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 4]) });
            *bytes = bytes.get(5..).ok_or_else(unexpected_end_of_value)?;
            Ok(Value::inner_posint(value as u64))
        } else {
            let bytes_to_parse = bytes
                .get(1..9)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u64::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 8]) });
            *bytes = bytes.get(9..).ok_or_else(unexpected_end_of_value)?;
            Ok(Value::inner_posint(value))
        }
    }

    fn unpack_negint(bytes: &mut &[u8]) -> Result<Value> {
        fn unexpected_end_of_value() -> Error {
            Error::new(ErrorKind::InvalidData, "unexpected end of value")
        }

        let first_byte = bytes.get(0).ok_or_else(|| {
            unexpected_end_of_value().with_metadata("cause", "first byte not found")
        })?;
        let first_byte = *first_byte & PAYLOAD_MASK;

        if first_byte <= negint::INLINE {
            *bytes = bytes
                .get(1..)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            return Ok(Value::inner_negint(first_byte as u64));
        } else if first_byte == negint::BITS_8 {
            let bytes_to_parse = bytes
                .get(1..2)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u8::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 1]) });
            *bytes = bytes.get(2..).ok_or_else(unexpected_end_of_value)?;
            Ok(Value::inner_negint(value as u64))
        } else if first_byte == negint::BITS_16 {
            let bytes_to_parse = bytes
                .get(1..3)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u16::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 2]) });
            *bytes = bytes.get(3..).ok_or_else(unexpected_end_of_value)?;
            Ok(Value::inner_negint(value as u64))
        } else if first_byte == negint::BITS_32 {
            let bytes_to_parse = bytes
                .get(1..5)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u32::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 4]) });
            *bytes = bytes.get(5..).ok_or_else(unexpected_end_of_value)?;
            Ok(Value::inner_negint(value as u64))
        } else {
            let bytes_to_parse = bytes
                .get(1..9)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u64::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 8]) });
            *bytes = bytes.get(9..).ok_or_else(unexpected_end_of_value)?;
            Ok(Value::inner_negint(value))
        }
    }

    fn unpack_bstr(bytes: &mut &[u8]) -> Result<Value> {
        fn unexpected_end_of_value() -> Error {
            Error::new(ErrorKind::InvalidData, "unexpected end of value")
        }

        let first_byte = bytes.get(0).ok_or_else(|| {
            unexpected_end_of_value().with_metadata("cause", "first byte not found")
        })?;
        let first_byte_payload = *first_byte & PAYLOAD_MASK;

        let value_len =
            if first_byte_payload <= bstr::INLINE {
                *bytes = bytes.get(1..).ok_or_else(|| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?;
                first_byte_payload as u64
            } else if first_byte_payload == bstr::BITS_8 {
                let bytes_to_parse = bytes.get(1..2).ok_or_else(|| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?;
                let value = u8::from_le_bytes(bytes_to_parse.try_into().map_err(|_| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?);
                *bytes = bytes.get(2..).ok_or_else(unexpected_end_of_value)?;
                value as u64
            } else if first_byte_payload == bstr::BITS_16 {
                let bytes_to_parse = bytes.get(1..3).ok_or_else(|| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?;
                let value = u16::from_le_bytes(bytes_to_parse.try_into().map_err(|_| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?);
                *bytes = bytes.get(3..).ok_or_else(unexpected_end_of_value)?;
                value as u64
            } else if first_byte_payload == bstr::BITS_32 {
                let bytes_to_parse = bytes.get(1..5).ok_or_else(|| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?;
                let value = u32::from_le_bytes(bytes_to_parse.try_into().map_err(|_| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?);
                *bytes = bytes.get(5..).ok_or_else(unexpected_end_of_value)?;
                value as u64
            } else {
                let bytes_to_parse = bytes.get(1..9).ok_or_else(|| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?;
                let value = u64::from_le_bytes(bytes_to_parse.try_into().map_err(|_| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?);
                *bytes = bytes.get(9..).ok_or_else(unexpected_end_of_value)?;
                value
            };

        let value_len = usize::try_from(value_len)
            .map_err(|_| Error::new(ErrorKind::InvalidData, "binary string too large"))?;
        let value = bytes
            .get(..value_len)
            .ok_or_else(unexpected_end_of_value)?
            .to_vec();
        *bytes = bytes.get(value_len..).ok_or_else(unexpected_end_of_value)?;

        Ok(Value::inner_bstr(value))
    }

    fn unpack_tstr(bytes: &mut &[u8]) -> Result<Value> {
        fn unexpected_end_of_value() -> Error {
            Error::new(ErrorKind::InvalidData, "unexpected end of value")
        }

        let first_byte = bytes.get(0).ok_or_else(|| {
            unexpected_end_of_value().with_metadata("cause", "first byte not found")
        })?;
        let first_byte_payload = *first_byte & PAYLOAD_MASK;

        let value_len =
            if first_byte_payload <= tstr::INLINE {
                *bytes = bytes.get(1..).ok_or_else(|| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?;
                first_byte_payload as u64
            } else if first_byte_payload == tstr::BITS_8 {
                let bytes_to_parse = bytes.get(1..2).ok_or_else(|| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?;
                let value = u8::from_le_bytes(bytes_to_parse.try_into().map_err(|_| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?);
                *bytes = bytes.get(2..).ok_or_else(unexpected_end_of_value)?;
                value as u64
            } else if first_byte_payload == tstr::BITS_16 {
                let bytes_to_parse = bytes.get(1..3).ok_or_else(|| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?;
                let value = u16::from_le_bytes(bytes_to_parse.try_into().map_err(|_| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?);
                *bytes = bytes.get(3..).ok_or_else(unexpected_end_of_value)?;
                value as u64
            } else if first_byte_payload == tstr::BITS_32 {
                let bytes_to_parse = bytes.get(1..5).ok_or_else(|| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?;
                let value = u32::from_le_bytes(bytes_to_parse.try_into().map_err(|_| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?);
                *bytes = bytes.get(5..).ok_or_else(unexpected_end_of_value)?;
                value as u64
            } else {
                let bytes_to_parse = bytes.get(1..9).ok_or_else(|| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?;
                let value = u64::from_le_bytes(bytes_to_parse.try_into().map_err(|_| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?);
                *bytes = bytes.get(9..).ok_or_else(unexpected_end_of_value)?;
                value
            };

        let value_len = usize::try_from(value_len)
            .map_err(|_| Error::new(ErrorKind::InvalidData, "text string too large"))?;
        let value_bytes = bytes.get(..value_len).ok_or_else(unexpected_end_of_value)?;
        let value = std::str::from_utf8(value_bytes)
            .map_err(|_| Error::new(ErrorKind::InvalidData, "invalid UTF-8 text string"))?
            .to_owned();
        *bytes = bytes.get(value_len..).ok_or_else(unexpected_end_of_value)?;

        Ok(Value::inner_tstr(value))
    }

    fn unpack_simple_or_float(bytes: &mut &[u8]) -> Result<Value> {
        fn unexpected_end_of_value() -> Error {
            Error::new(ErrorKind::InvalidData, "unexpected end of value")
        }

        let first_byte = bytes.get(0).ok_or_else(unexpected_end_of_value)?;
        let first_byte_payload = *first_byte & PAYLOAD_MASK;

        match first_byte_payload {
            float::FALSE => {
                *bytes = bytes.get(1..).ok_or_else(unexpected_end_of_value)?;
                Ok(Value::inner_bool(false))
            }
            float::TRUE => {
                *bytes = bytes.get(1..).ok_or_else(unexpected_end_of_value)?;
                Ok(Value::inner_bool(true))
            }
            float::NULL => {
                *bytes = bytes.get(1..).ok_or_else(unexpected_end_of_value)?;
                Ok(Value::inner_null())
            }
            float::BITS_64 => Self::unpack_float(bytes),
            _ => Err(
                Error::new(ErrorKind::InvalidData, "invalid simple value or float")
                    .with_metadata("first_byte", first_byte),
            ),
        }
    }

    fn unpack_float(bytes: &mut &[u8]) -> Result<Value> {
        fn unexpected_end_of_value() -> Error {
            Error::new(ErrorKind::InvalidData, "unexpected end of value")
        }

        let first_byte = bytes.get(0).ok_or_else(|| {
            unexpected_end_of_value().with_metadata("cause", "first byte not found")
        })?;
        let first_byte_payload = *first_byte & PAYLOAD_MASK;

        if first_byte_payload != float::BITS_64 {
            return Err(unexpected_end_of_value().with_metadata("first_byte_payload", first_byte));
        }

        let bytes_to_parse = bytes
            .get(1..9)
            .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
        let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
        let value = f64::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 8]) });
        *bytes = bytes.get(9..).ok_or_else(unexpected_end_of_value)?;

        Ok(Value::inner_float(value))
    }

    fn unpack_array(bytes: &mut &[u8], state: &ConsumerStateTxn) -> Result<Value> {
        fn unexpected_end_of_value() -> Error {
            Error::new(ErrorKind::InvalidData, "unexpected end of value")
        }

        let first_byte = bytes.get(0).ok_or_else(|| {
            unexpected_end_of_value().with_metadata("cause", "first byte not found")
        })?;
        let first_byte_payload = *first_byte & PAYLOAD_MASK;

        let value_len =
            if first_byte_payload <= array::INLINE {
                *bytes = bytes.get(1..).ok_or_else(|| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?;
                first_byte_payload as u64
            } else if first_byte_payload == array::BITS_8 {
                let bytes_to_parse = bytes.get(1..2).ok_or_else(|| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?;
                let value = u8::from_le_bytes(bytes_to_parse.try_into().map_err(|_| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?);
                *bytes = bytes.get(2..).ok_or_else(unexpected_end_of_value)?;
                value as u64
            } else if first_byte_payload == array::BITS_16 {
                let bytes_to_parse = bytes.get(1..3).ok_or_else(|| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?;
                let value = u16::from_le_bytes(bytes_to_parse.try_into().map_err(|_| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?);
                *bytes = bytes.get(3..).ok_or_else(unexpected_end_of_value)?;
                value as u64
            } else if first_byte_payload == array::BITS_32 {
                let bytes_to_parse = bytes.get(1..5).ok_or_else(|| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?;
                let value = u32::from_le_bytes(bytes_to_parse.try_into().map_err(|_| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?);
                *bytes = bytes.get(5..).ok_or_else(unexpected_end_of_value)?;
                value as u64
            } else {
                let bytes_to_parse = bytes.get(1..9).ok_or_else(|| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?;
                let value = u64::from_le_bytes(bytes_to_parse.try_into().map_err(|_| {
                    unexpected_end_of_value().with_metadata("first_byte", first_byte)
                })?);
                *bytes = bytes.get(9..).ok_or_else(unexpected_end_of_value)?;
                value
            };

        let mut value = Vec::new();
        for index in 0..value_len {
            let item_value = Self::unpack_value(bytes, state).map_err(|e| {
                e.with_context("unpack array value")
                    .with_metadata("index", index)
            })?;
            value.push(item_value);
        }

        Ok(Value::inner_array(value))
    }

    fn unpack_map(bytes: &mut &[u8], state: &ConsumerStateTxn) -> Result<Value> {
        fn unexpected_end_of_value() -> Error {
            Error::new(ErrorKind::InvalidData, "unexpected end of value")
        }

        let first_byte = bytes.get(0).ok_or_else(|| {
            unexpected_end_of_value().with_metadata("cause", "first byte not found")
        })?;
        let first_byte_payload = *first_byte & PAYLOAD_MASK;

        let value_len = if first_byte_payload <= map::INLINE {
            *bytes = bytes
                .get(1..)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            first_byte_payload as u64
        } else if first_byte_payload == map::BITS_8 {
            let bytes_to_parse = bytes
                .get(1..2)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u8::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 1]) });
            *bytes = bytes.get(2..).ok_or_else(unexpected_end_of_value)?;
            value as u64
        } else if first_byte_payload == map::BITS_16 {
            let bytes_to_parse = bytes
                .get(1..3)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u16::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 2]) });
            *bytes = bytes.get(3..).ok_or_else(unexpected_end_of_value)?;
            value as u64
        } else if first_byte_payload == map::BITS_32 {
            let bytes_to_parse = bytes
                .get(1..5)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u32::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 4]) });
            *bytes = bytes.get(5..).ok_or_else(unexpected_end_of_value)?;
            value as u64
        } else {
            let bytes_to_parse = bytes
                .get(1..9)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u64::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 8]) });
            *bytes = bytes.get(9..).ok_or_else(unexpected_end_of_value)?;
            value
        };

        let mut value = BTreeMap::new();
        for index in 0..value_len {
            let key = Self::unpack_varuint(bytes).map_err(|e| e.with_context("unpack map key"))?;
            let key = u32::try_from(key).map_err(|_| {
                Error::new(ErrorKind::InvalidData, "map key too large")
                    .with_metadata("index", index)
            })?;
            let key = state.get_string(key).ok_or_else(|| {
                Error::new(ErrorKind::InvalidData, "map key not found in string pool")
                    .with_metadata("index", index)
                    .with_metadata("key", key)
            })?;

            let item_value =
                Self::unpack_value(bytes, state).map_err(|e| e.with_context("unpack map value"))?;
            if value.insert(key.clone(), item_value).is_some() {
                return Err(Error::new(ErrorKind::InvalidData, "duplicate map key")
                    .with_metadata("index", index));
            }
        }

        Ok(Value::inner_map(value))
    }
}

/// A builder to build packed doxsync messages.
pub(super) struct PackedMessageBuilder<'a, 's> {
    actions: Option<&'a Vec<Action>>,
    state_txn: Option<&'s mut ProducerStateTxn>,
    actions_limit: usize,
}

impl Default for PackedMessageBuilder<'_, '_> {
    fn default() -> Self {
        Self {
            actions: None,
            state_txn: None,
            actions_limit: 65535,
        }
    }
}

impl<'a, 's> PackedMessageBuilder<'a, 's> {
    pub(super) fn with_actions(&mut self, actions: &'a Vec<Action>) -> &mut Self {
        self.actions = Some(actions);
        self
    }

    pub(super) fn with_state_txn(&mut self, state_txn: &'s mut ProducerStateTxn) -> &mut Self {
        self.state_txn = Some(state_txn);
        self
    }

    pub(super) fn build(&mut self) -> Result<PackedMessage> {
        let actions = self
            .actions
            .ok_or(Error::new(ErrorKind::Internal, "actions not set"))?;
        let mut state_txn = None;
        std::mem::swap(&mut state_txn, &mut self.state_txn);
        let Some(state_txn) = state_txn else {
            return Err(Error::new(ErrorKind::Internal, "state_txn not set"));
        };

        let mut internal = PackedMessageBuilderInternal {
            actions,
            state_txn,
            actions_limit: self.actions_limit,
        };
        let mut bytes = Vec::new();
        let savepoint = internal.state_txn.savepoint();
        if let Err(error) = internal.build(&mut bytes) {
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
        if actions.len() > actions_limit {
            return Err(Error::new(ErrorKind::InvalidData, "too many actions"));
        }
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
        let mut strings = Vec::new();
        let mut string_seen = BTreeSet::new();
        let mut paths = Vec::new();
        let mut path_seen = BTreeSet::new();
        for action in actions {
            match action {
                Action::Snapshot { value } => {
                    collect_strings(value, &mut string_seen, &mut strings)?
                }
                Action::Add { path, value } => {
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

pub(super) fn validate_message(actions: &[Action], state_txn: &ProducerStateTxn) -> Result<()> {
    MessageMetadata::collect(
        actions,
        state_txn,
        PackedMessageBuilder::default().actions_limit,
    )
    .map(|_| ())
}

struct PackedMessageBuilderInternal<'a, 's> {
    actions: &'a Vec<Action>,
    state_txn: &'s mut ProducerStateTxn,
    actions_limit: usize,
}

impl<'a, 's> PackedMessageBuilderInternal<'a, 's> {
    fn build(&mut self, bytes: &mut Vec<u8>) -> Result<()> {
        // Do build the metadata.
        self.build_metadata(bytes)?;

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
                    self.pack_path_reference(bytes, path)?;
                    self.pack_value(bytes, value);
                }
                Action::Delete { path } => {
                    self.pack_varuint(bytes, ACTION_DELETE as u64);
                    self.pack_path_reference(bytes, path)?;
                }
            }
        }

        Ok(())
    }

    fn build_metadata(&mut self, bytes: &mut Vec<u8>) -> Result<()> {
        let MessageMetadata { strings, paths } =
            MessageMetadata::collect(self.actions, self.state_txn, self.actions_limit)?;
        // Touch every hit first; new entries can then evict only unused old entries.
        for string in &strings {
            if self.state_txn.get_string_key(string).is_some() {
                self.state_txn.insert_string(string.clone());
            }
        }
        let mut string_patch = Vec::new();
        for string in strings {
            if self.state_txn.get_string_key(&string).is_none() {
                match self.state_txn.insert_string(string.clone()) {
                    InsertStringResult::Inserted { key } | InsertStringResult::Replaced { key } => {
                        string_patch.push((key, string))
                    }
                    InsertStringResult::Existing { .. } => unreachable!("new string"),
                }
            }
        }
        for path in &paths {
            if self.state_txn.get_path_key(path).is_some() {
                self.state_txn.insert_path(path.clone());
            }
        }
        let mut path_patch = Vec::new();
        for path in paths {
            if self.state_txn.get_path_key(&path).is_none() {
                let mut definition = Vec::new();
                self.pack_path(&mut definition, &path);
                match self.state_txn.insert_path(path) {
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

    fn pack_path_reference(&mut self, bytes: &mut Vec<u8>, path: &Path) -> Result<()> {
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
