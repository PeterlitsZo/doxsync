mod action;
mod packed;

pub use packed::PackedMessage;

pub(crate) use action::Action;

use crate::Result;

/// A structured doxsync message.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    actions: Vec<Action>,
}

impl Message {
    pub(crate) fn new(actions: Vec<Action>) -> Self {
        Self { actions }
    }

    pub(crate) fn actions(&self) -> &[Action] {
        &self.actions
    }

    pub fn from_packed(packed: PackedMessage) -> Result<Self> {
        packed::PackedMessageDecoder::default().decode(packed.bytes())
    }

    pub fn packed(&self) -> PackedMessage {
        packed::PackedMessageBuilder::default()
            .with_actions(&self.actions)
            .build()
            .expect("packing message failed")
    }
}

#[cfg(test)]
mod tests {
    use crate::Value;

    use super::*;

    #[test]
    fn test_pack_and_unpack() {
        // Case 1:
        // =====================================================================

        let message = Message::new(vec![
            Action::Snapshot { value: Value::int(42).unwrap() },
        ]);

        let packed = message.packed();
        assert_eq!(packed.bytes(), &[ 0x00, 0x01, 0x00, 0x0c, 0x2a ]);

        let unpacked = Message::from_packed(packed).unwrap();
        assert_eq!(unpacked.actions, &[
            Action::Snapshot { value: Value::int(42).unwrap() },
        ]);

        // Case 2:
        // =====================================================================

        let message = Message::new(vec![
            Action::Snapshot { value: Value::int(1).unwrap() },
        ]);

        let packed = message.packed();
        assert_eq!(packed.bytes(), &[ 0x00, 0x01, 0x00, 0x01 ]);

        let unpacked = Message::from_packed(packed).unwrap();
        assert_eq!(unpacked.actions, &[
            Action::Snapshot { value: Value::int(1).unwrap() },
        ]);

        // Case 3:
        // =====================================================================

        let message = Message::new(vec![
            Action::Snapshot { value: Value::int(0x1FFFFFF).unwrap() },
        ]);

        let packed = message.packed();
        assert_eq!(packed.bytes(), &[ 0x00, 0x01, 0x00, 0x0E, 0xFF, 0xFF, 0xFF, 0x01 ]);

        let unpacked = Message::from_packed(packed).unwrap();
        assert_eq!(unpacked.actions, &[
            Action::Snapshot { value: Value::int(0x1FFFFFF).unwrap() },
        ]);

        // Case 4:
        // =====================================================================

        let message = Message::new(vec![
            Action::Snapshot { value: Value::int(-42).unwrap() },
        ]);

        let packed = message.packed();
        assert_eq!(packed.bytes(), &[ 0x00, 0x01, 0x00, 0x1c, 0x29 ]);

        let unpacked = Message::from_packed(packed).unwrap();
        assert_eq!(unpacked.actions, &[
            Action::Snapshot { value: Value::int(-42).unwrap() },
        ]);

        // Case 5:
        // =====================================================================

        let message = Message::new(vec![
            Action::Snapshot { value: Value::int(-0x1FFFFFF).unwrap() },
        ]);

        let packed = message.packed();
        assert_eq!(packed.bytes(), &[ 0x00, 0x01, 0x00, 0x1E, 0xFE, 0xFF, 0xFF, 0x01 ]);

        let unpacked = Message::from_packed(packed).unwrap();
        assert_eq!(unpacked.actions, &[
            Action::Snapshot { value: Value::int(-0x1FFFFFF).unwrap() },
        ]);
    }
}
