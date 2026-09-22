use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use super::PackedMessage;
use crate::message::Message;
use crate::patch::{Action, Path, PathSegment};
use crate::protocol::consts::*;
use crate::state::{
    ConsumerStateTxn, PATH_KEY_BYTES_LIMIT, PATH_PATCH_BYTES_LIMIT, PATH_POOL_CAPACITY,
    PATH_SEGMENTS_LIMIT, STRING_POOL_CAPACITY,
};
use crate::{Error, ErrorKind, Result, Value};

/// A decoder for packed doxsync messages.
pub(in crate::message) struct PackedMessageDecoder {
    actions_limit: usize,
}

impl Default for PackedMessageDecoder {
    fn default() -> Self {
        Self {
            actions_limit: DEFAULT_ACTIONS_LIMIT,
        }
    }
}

impl PackedMessageDecoder {
    pub(in crate::message) fn decode(
        &self,
        packed: PackedMessage,
        state_txn: &mut ConsumerStateTxn,
    ) -> Result<Message> {
        let savepoint = state_txn.savepoint();
        match self.decode_inner(packed.bytes(), state_txn) {
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
                METADATA_STRINGS => {
                    let patch = Self::unpack_string_pool_patch(&mut bytes)
                        .map_err(|e| e.with_context("unpack string pool patch"))?;
                    state.apply_string_pool_patch(patch);
                }
                METADATA_PATHS => {
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

    fn unpack_path_by_key(bytes: &mut &[u8], state: &ConsumerStateTxn) -> Result<Path> {
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
                let path = Self::unpack_path_by_key(bytes, state)?;
                let value = Self::unpack_value(bytes, state)?;
                Ok(Action::Add { path, value })
            }
            ACTION_REPLACE => {
                let path = Self::unpack_path_by_key(bytes, state)?;
                let value = Self::unpack_value(bytes, state)?;
                Ok(Action::Replace { path, value })
            }
            ACTION_DELETE => {
                let path = Self::unpack_path_by_key(bytes, state)?;
                Ok(Action::Delete { path })
            }
            ACTION_COPY => {
                let path = Self::unpack_path_by_key(bytes, state)?;
                let from = Self::unpack_path_by_key(bytes, state)?;
                Ok(Action::Copy { path, from })
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
            TAG_TSTR_REF => Self::unpack_tstr_ref(bytes, state).map_err(|e| e.with_context("unpack text string reference")),
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

    fn unpack_tstr_ref(bytes: &mut &[u8], state: &ConsumerStateTxn) -> Result<Value> {
        let first_byte = bytes.first().copied().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "unexpected end of text string reference",
            )
        })?;
        let payload = first_byte & PAYLOAD_MASK;
        let (key, encoded_len) = if payload <= posint::INLINE {
            (payload as u64, 1)
        } else {
            let width = 1usize << (payload - posint::BITS_8);
            let key_bytes = bytes.get(1..1 + width).ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "unexpected end of text string reference",
                )
            })?;
            let mut buffer = [0u8; 8];
            buffer[..width].copy_from_slice(key_bytes);
            (u64::from_le_bytes(buffer), 1 + width)
        };
        let key = u32::try_from(key)
            .map_err(|_| Error::new(ErrorKind::InvalidData, "text string reference too large"))?;
        let string = state.get_string(key).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "text string not found in string pool",
            )
            .with_metadata("key", key)
        })?;
        *bytes = &bytes[encoded_len..];
        Ok(Value::inner_tstr(string.as_ref().clone()))
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
            let key_payload = key >> 2;
            let key = match key & 0b11 {
                0b00 => {
                    let key = u32::try_from(key_payload).map_err(|_| {
                        Error::new(ErrorKind::InvalidData, "map key too large")
                            .with_metadata("index", index)
                    })?;
                    state.get_string(key).cloned().ok_or_else(|| {
                        Error::new(ErrorKind::InvalidData, "map key not found in string pool")
                            .with_metadata("index", index)
                            .with_metadata("key", key)
                    })?
                }
                0b01 => {
                    let key_len = usize::try_from(key_payload).map_err(|_| {
                        Error::new(ErrorKind::InvalidData, "map key too large")
                            .with_metadata("index", index)
                    })?;
                    let key_bytes = bytes.get(..key_len).ok_or_else(|| {
                        unexpected_end_of_value()
                            .with_metadata("index", index)
                            .with_metadata("key_len", key_len)
                    })?;
                    let key = std::str::from_utf8(key_bytes).map_err(|_| {
                        Error::new(ErrorKind::InvalidData, "invalid UTF-8 map key")
                            .with_metadata("index", index)
                    })?;
                    let key = Arc::new(key.to_owned());
                    *bytes = bytes.get(key_len..).ok_or_else(unexpected_end_of_value)?;
                    key
                }
                _ => {
                    return Err(Error::new(ErrorKind::InvalidData, "invalid map key type")
                        .with_metadata("index", index));
                }
            };

            let item_value =
                Self::unpack_value(bytes, state).map_err(|e| e.with_context("unpack map value"))?;
            if value.insert(key, item_value).is_some() {
                return Err(Error::new(ErrorKind::InvalidData, "duplicate map key")
                    .with_metadata("index", index));
            }
        }

        Ok(Value::inner_map(value))
    }
}
