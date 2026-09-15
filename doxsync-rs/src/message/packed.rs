use crate::{
    Error, ErrorKind, Result, Value,
    message::{Action, Message},
};

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
            _ => Err(Error::new(ErrorKind::InvalidData, "invalid action type")),
        }
    }

    fn unpack_value(bytes: &mut &[u8]) -> Result<Value> {
        // Get the first byte to determine the value type.
        let value_type = bytes.get(0).ok_or(Error::new(
            ErrorKind::InvalidData,
            "unexpected end of value",
        ))?;
        match value_type >> 4 {
            0 => {
                let value =
                    Self::unpack_posint(bytes).map_err(|e| e.with_context("unpack posint"))?;
                Ok(Value::inner_posint(value))
            }
            1 => {
                let value =
                    Self::unpack_negint(bytes).map_err(|e| e.with_context("unpack negint"))?;
                Ok(Value::inner_negint(value))
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
        let first_byte = *first_byte & 0x0F;

        if first_byte <= 11 {
            *bytes = bytes
                .get(1..)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            return Ok(first_byte as u64);
        } else if first_byte == 12 {
            let bytes_to_parse = bytes
                .get(1..2)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u8::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 1]) });
            *bytes = bytes.get(2..).ok_or_else(unexpected_end_of_value)?;
            Ok(value as u64)
        } else if first_byte == 13 {
            let bytes_to_parse = bytes
                .get(1..3)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u16::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 2]) });
            *bytes = bytes.get(3..).ok_or_else(unexpected_end_of_value)?;
            Ok(value as u64)
        } else if first_byte == 14 {
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
        let first_byte = *first_byte & 0x0F;

        if first_byte <= 11 {
            *bytes = bytes
                .get(1..)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            return Ok(first_byte as u64);
        } else if first_byte == 12 {
            let bytes_to_parse = bytes
                .get(1..2)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u8::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 1]) });
            *bytes = bytes.get(2..).ok_or_else(unexpected_end_of_value)?;
            Ok(value as u64)
        } else if first_byte == 13 {
            let bytes_to_parse = bytes
                .get(1..3)
                .ok_or_else(|| unexpected_end_of_value().with_metadata("first_byte", first_byte))?;
            let bytes_ptr = bytes_to_parse as *const [u8] as *const u8;
            let value = u16::from_le_bytes(unsafe { *(bytes_ptr as *const [u8; 2]) });
            *bytes = bytes.get(3..).ok_or_else(unexpected_end_of_value)?;
            Ok(value as u64)
        } else if first_byte == 14 {
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
        }
    }

    fn pack_posint(bytes: &mut Vec<u8>, value: u64) {
        if value <= 11 {
            // Pack small values directly as bytes.
            bytes.push((0u8 << 4) | (value as u8));
            return;
        } else if value <= 255 {
            // Pack 1-byte values with type indicator.
            bytes.push((0u8 << 4) | 12);
            bytes.push(value as u8);
            return;
        } else if value < (1 << 16) {
            // Pack 2-byte values with type indicator.
            bytes.push((0u8 << 4) | 13);
            bytes.extend_from_slice(&(value as u16).to_le_bytes());
            return;
        } else if value < (1 << 32) {
            // Pack 4-byte values with type indicator.
            bytes.push((0u8 << 4) | 14);
            bytes.extend_from_slice(&(value as u32).to_le_bytes());
            return;
        } else {
            // Pack 8-byte values with type indicator.
            bytes.push((0u8 << 4) | 15);
            bytes.extend_from_slice(&value.to_le_bytes());
            return;
        }
    }

    fn pack_negint(bytes: &mut Vec<u8>, value: u64) {
        if value <= 11 {
            // Pack small values directly as bytes.
            bytes.push((1u8 << 4) | (value as u8));
            return;
        } else if value <= 255 {
            // Pack 1-byte values with type indicator.
            bytes.push((1u8 << 4) | 12);
            bytes.push(value as u8);
            return;
        } else if value < (1 << 16) {
            // Pack 2-byte values with type indicator.
            bytes.push((1u8 << 4) | 13);
            bytes.extend_from_slice(&(value as u16).to_le_bytes());
            return;
        } else if value < (1 << 32) {
            // Pack 4-byte values with type indicator.
            bytes.push((1u8 << 4) | 14);
            bytes.extend_from_slice(&(value as u32).to_le_bytes());
            return;
        } else {
            // Pack 8-byte values with type indicator.
            bytes.push((1u8 << 4) | 15);
            bytes.extend_from_slice(&value.to_le_bytes());
            return;
        }
    }
}
