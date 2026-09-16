use std::{collections::BTreeMap, sync::Arc};

use crate::{
    Error, ErrorKind, Result, Value,
    message::{Action, Message, Path, PathSegment},
};

const TAG_POSINT: u8 = 0b0000;
const TAG_NEGINT: u8 = 0b0001;
const TAG_MAP: u8 = 0b0101;
const TAG_FLOAT: u8 = 0b0111;

const TAG_WIDTH: usize = 4;
const PAYLOAD_MASK: u8 = 0x0F;

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

mod float {
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
    metadata_limit: usize,
    actions_limit: usize,
}

impl Default for PackedMessageDecoder {
    fn default() -> Self {
        Self {
            metadata_limit: 65535,
            actions_limit: 65535,
        }
    }
}

impl PackedMessageDecoder {
    pub(super) fn decode(&self, bytes: &[u8]) -> Result<Message> {
        let mut bytes = bytes;
        let mut actions = Vec::new();

        // Unpack the metadata's length.
        let metadata_len = Self::unpack_varuint(&mut bytes)?;
        if metadata_len > self.metadata_limit as u64 {
            return Err(Error::new(ErrorKind::InvalidData, "metadata too large"));
        }

        // Now we do not support metadata, so it must be zero.
        if metadata_len != 0 {
            return Err(Error::new(ErrorKind::InvalidData, "unexpected metadata"));
        }

        // Unpack the actions.
        let actions_len = Self::unpack_varuint(&mut bytes)?;
        if actions_len > self.actions_limit as u64 {
            return Err(Error::new(ErrorKind::InvalidData, "actions too large"));
        }
        for _ in 0..actions_len {
            let action = Self::unpack_action(&mut bytes)?;
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

    fn unpack_action(bytes: &mut &[u8]) -> Result<Action> {
        let action_type = Self::unpack_varuint(bytes)?;
        match action_type {
            0 => {
                let value = Self::unpack_value(bytes)?;
                Ok(Action::Snapshot { value })
            }
            1 => {
                let path = Self::unpack_path(bytes)?;
                let value = Self::unpack_value(bytes)?;
                Ok(Action::Add { path, value })
            }
            2 => {
                let path = Self::unpack_path(bytes)?;
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

    fn unpack_value(bytes: &mut &[u8]) -> Result<Value> {
        // Get the first byte to determine the value type.
        let value_type = bytes.get(0).ok_or(Error::new(
            ErrorKind::InvalidData,
            "unexpected end of value",
        ))?;
        match value_type >> TAG_WIDTH {
            TAG_POSINT => {
                let value =
                    Self::unpack_posint(bytes).map_err(|e| e.with_context("unpack posint"))?;
                Ok(Value::inner_posint(value))
            }
            TAG_NEGINT => {
                let value =
                    Self::unpack_negint(bytes).map_err(|e| e.with_context("unpack negint"))?;
                Ok(Value::inner_negint(value))
            }
            TAG_FLOAT => {
                let value =
                    Self::unpack_float(bytes).map_err(|e| e.with_context("unpack float"))?;
                Ok(Value::inner_float(value))
            }
            TAG_MAP => {
                let value = Self::unpack_map(bytes).map_err(|e| e.with_context("unpack map"))?;
                Ok(Value::inner_map(value))
            }
            _ => Err(Error::new(ErrorKind::InvalidData, "invalid value type")),
        }
    }

    fn unpack_posint(bytes: &mut &[u8]) -> Result<u64> {
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
            return Ok(first_byte as u64);
        } else if first_byte == posint::BITS_8 {
            let bytes_to_parse = bytes
                .get(1..2)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u8::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 1]) });
            *bytes = bytes.get(2..).ok_or_else(unexpected_end_of_value)?;
            Ok(value as u64)
        } else if first_byte == posint::BITS_16 {
            let bytes_to_parse = bytes
                .get(1..3)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u16::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 2]) });
            *bytes = bytes.get(3..).ok_or_else(unexpected_end_of_value)?;
            Ok(value as u64)
        } else if first_byte == posint::BITS_32 {
            let bytes_to_parse = bytes
                .get(1..5)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u32::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 4]) });
            *bytes = bytes.get(5..).ok_or_else(unexpected_end_of_value)?;
            Ok(value as u64)
        } else {
            let bytes_to_parse = bytes
                .get(1..9)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u64::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 8]) });
            *bytes = bytes.get(9..).ok_or_else(unexpected_end_of_value)?;
            Ok(value as u64)
        }
    }

    fn unpack_negint(bytes: &mut &[u8]) -> Result<u64> {
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
            return Ok(first_byte as u64);
        } else if first_byte == negint::BITS_8 {
            let bytes_to_parse = bytes
                .get(1..2)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u8::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 1]) });
            *bytes = bytes.get(2..).ok_or_else(unexpected_end_of_value)?;
            Ok(value as u64)
        } else if first_byte == negint::BITS_16 {
            let bytes_to_parse = bytes
                .get(1..3)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u16::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 2]) });
            *bytes = bytes.get(3..).ok_or_else(unexpected_end_of_value)?;
            Ok(value as u64)
        } else if first_byte == negint::BITS_32 {
            let bytes_to_parse = bytes
                .get(1..5)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u32::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 4]) });
            *bytes = bytes.get(5..).ok_or_else(unexpected_end_of_value)?;
            Ok(value as u64)
        } else {
            let bytes_to_parse = bytes
                .get(1..9)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u64::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 8]) });
            *bytes = bytes.get(9..).ok_or_else(unexpected_end_of_value)?;
            Ok(value as u64)
        }
    }

    fn unpack_float(bytes: &mut &[u8]) -> Result<f64> {
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

        Ok(value)
    }

    fn unpack_map(bytes: &mut &[u8]) -> Result<BTreeMap<Arc<String>, Value>> {
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
            let key_len =
                Self::unpack_varuint(bytes).map_err(|e| e.with_context("unpack map key length"))?;
            let key_len = usize::try_from(key_len).map_err(|_| {
                Error::new(ErrorKind::InvalidData, "map key too large")
                    .with_metadata("index", index)
            })?;
            let key_bytes = bytes.get(..key_len).ok_or_else(|| {
                unexpected_end_of_value()
                    .with_metadata("index", index)
                    .with_metadata("key_len", key_len)
            })?;
            let key = std::str::from_utf8(key_bytes)
                .map_err(|_| {
                    Error::new(ErrorKind::InvalidData, "invalid UTF-8 map key")
                        .with_metadata("index", index)
                })?
                .to_owned();
            *bytes = bytes.get(key_len..).ok_or_else(unexpected_end_of_value)?;

            let item_value =
                Self::unpack_value(bytes).map_err(|e| e.with_context("unpack map value"))?;
            if value.insert(Arc::new(key), item_value).is_some() {
                return Err(Error::new(ErrorKind::InvalidData, "duplicate map key")
                    .with_metadata("index", index));
            }
        }

        Ok(value)
    }
}

/// A builder to build packed doxsync messages.
pub(super) struct PackedMessageBuilder<'a> {
    actions: Option<&'a Vec<Action>>,
    metadata_limit: usize,
    actions_limit: usize,
}

impl Default for PackedMessageBuilder<'_> {
    fn default() -> Self {
        Self {
            actions: None,
            metadata_limit: 65535,
            actions_limit: 65535,
        }
    }
}

impl<'a> PackedMessageBuilder<'a> {
    pub(super) fn with_actions(&mut self, actions: &'a Vec<Action>) -> &mut Self {
        self.actions = Some(actions);
        self
    }

    pub(super) fn build(&self) -> Result<PackedMessage> {
        let mut bytes = Vec::new();
        let actions = self
            .actions
            .ok_or(Error::new(ErrorKind::Internal, "actions not set"))?;

        // Now we do not need to pack metadata.
        Self::pack_varuint(&mut bytes, 0);

        // Now we pack the actions.
        if actions.len() > self.actions_limit {
            return Err(Error::new(ErrorKind::Internal, "too many actions"));
        }
        Self::pack_varuint(&mut bytes, actions.len() as u64);
        for action in actions {
            match action {
                Action::Snapshot { value } => {
                    Self::pack_varuint(&mut bytes, 0);
                    Self::pack_value(&mut bytes, value);
                }
                Action::Add { path, value } => {
                    Self::pack_varuint(&mut bytes, 1);
                    Self::pack_path(&mut bytes, path);
                    Self::pack_value(&mut bytes, value);
                }
                Action::Delete { path } => {
                    Self::pack_varuint(&mut bytes, 2);
                    Self::pack_path(&mut bytes, path);
                }
            }
        }

        Ok(PackedMessage { inner: bytes })
    }

    fn pack_varuint(bytes: &mut Vec<u8>, value: u64) {
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

    fn pack_value(bytes: &mut Vec<u8>, value: &Value) {
        use crate::ValueInner;

        match value.inner() {
            ValueInner::PosInt { inner } => Self::pack_posint(bytes, *inner),
            ValueInner::NegInt { inner } => Self::pack_negint(bytes, *inner),
            ValueInner::Float { inner } => Self::pack_float(bytes, *inner),
            ValueInner::Map { inner } => Self::pack_map(bytes, inner),
        }
    }

    fn pack_posint(bytes: &mut Vec<u8>, value: u64) {
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

    fn pack_negint(bytes: &mut Vec<u8>, value: u64) {
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

    fn pack_float(bytes: &mut Vec<u8>, value: f64) {
        bytes.push((TAG_FLOAT << TAG_WIDTH) | float::BITS_64);
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn pack_map(bytes: &mut Vec<u8>, value: &BTreeMap<Arc<String>, Value>) {
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
            Self::pack_varuint(bytes, key.len() as u64);
            bytes.extend_from_slice(key.as_bytes());
            Self::pack_value(bytes, value);
        }
    }

    fn pack_path(bytes: &mut Vec<u8>, path: &Path) {
        Self::pack_varuint(bytes, path.segments().len() as u64);
        for segment in path.segments() {
            match segment {
                PathSegment::Key(key) => {
                    Self::pack_varuint(bytes, (key.len() as u64) << 2 | 0b00);
                    bytes.extend_from_slice(key.as_bytes());
                }
                PathSegment::Index(index) => {
                    Self::pack_varuint(bytes, (*index as u64) << 2 | 0b01);
                }
            }
        }
    }
}
