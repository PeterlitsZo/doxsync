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
    use std::collections::BTreeMap;

    use crate::Value;

    use super::*;

    fn hex_to_bytes(hex: &str) -> Vec<u8> {
        let mut hex_post = String::new();
        let hex_chars: Vec<_> = hex.chars().collect();
        let mut i = 0;
        loop {
            let byte = hex_chars[i];
            match byte {
                ' ' | '\n' => {}
                '/' => {
                    // Comment begin with '//'...
                    i += 1;
                    if i >= hex_chars.len() {
                        break;
                    }
                    let next = hex_chars[i];
                    if next != '/' {
                        panic!("expected '//' comment to end at index {}", i);
                    }
                    i += 1;
                    while i < hex_chars.len() {
                        let byte = hex_chars[i];
                        if byte == '\n' {
                            break;
                        }
                        i += 1;
                    }
                    continue;
                }
                _ => hex_post.push(byte),
            }
            i += 1;
            if i >= hex_chars.len() {
                break;
            }
        }
        hex::decode(hex_post).unwrap()
    }

    #[test]
    fn test_pack_and_unpack() {
        // Case 1:
        // =====================================================================

        let message = Message::new(vec![Action::Snapshot {
            value: Value::int(42).unwrap(),
        }]);

        let packed = message.packed();
        assert_eq!(packed.bytes(), hex_to_bytes("00 01 00 0c 2a"));

        let unpacked = Message::from_packed(packed).unwrap();
        assert_eq!(
            unpacked.actions,
            &[Action::Snapshot {
                value: Value::int(42).unwrap()
            },]
        );

        // Case 2:
        // =====================================================================

        let message = Message::new(vec![Action::Snapshot {
            value: Value::int(1).unwrap(),
        }]);

        let packed = message.packed();
        assert_eq!(packed.bytes(), hex_to_bytes("00 01 00 01"));

        let unpacked = Message::from_packed(packed).unwrap();
        assert_eq!(
            unpacked.actions,
            &[Action::Snapshot {
                value: Value::int(1).unwrap()
            },]
        );

        // Case 3:
        // =====================================================================

        let message = Message::new(vec![Action::Snapshot {
            value: Value::int(0x1FFFFFF).unwrap(),
        }]);

        let packed = message.packed();
        assert_eq!(packed.bytes(), hex_to_bytes("00 01 00 0E FF FF FF 01"));

        let unpacked = Message::from_packed(packed).unwrap();
        assert_eq!(
            unpacked.actions,
            &[Action::Snapshot {
                value: Value::int(0x1FFFFFF).unwrap()
            },]
        );

        // Case 4:
        // =====================================================================

        let message = Message::new(vec![Action::Snapshot {
            value: Value::int(-42).unwrap(),
        }]);

        let packed = message.packed();
        assert_eq!(packed.bytes(), hex_to_bytes("00 01 00 1c 29"));

        let unpacked = Message::from_packed(packed).unwrap();
        assert_eq!(
            unpacked.actions,
            &[Action::Snapshot {
                value: Value::int(-42).unwrap()
            },]
        );

        // Case 5:
        // =====================================================================

        let message = Message::new(vec![Action::Snapshot {
            value: Value::int(-0x1FFFFFF).unwrap(),
        }]);

        let packed = message.packed();
        assert_eq!(packed.bytes(), hex_to_bytes("00 01 00 1E FE FF FF 01"));

        let unpacked = Message::from_packed(packed).unwrap();
        assert_eq!(
            unpacked.actions,
            &[Action::Snapshot {
                value: Value::int(-0x1FFFFFF).unwrap()
            },]
        );

        // Case 6:
        // =====================================================================

        let message = Message::new(vec![Action::Snapshot {
            value: Value::float(3.1415926).unwrap(),
        }]);

        let packed = message.packed();
        assert_eq!(
            packed.bytes(),
            hex_to_bytes("00 01 00 7b 4a d8 12 4d fb 21 09 40")
        );

        let unpacked = Message::from_packed(packed).unwrap();
        assert_eq!(
            unpacked.actions,
            &[Action::Snapshot {
                value: Value::float(3.1415926).unwrap()
            },]
        );

        // Case 7:
        // =====================================================================

        let mut map = BTreeMap::new();
        map.insert("answer".to_owned(), Value::int(42).unwrap());
        map.insert("pi".to_owned(), Value::float(3.1415926).unwrap());
        let value = Value::map(map).unwrap();
        let message = Message::new(vec![Action::Snapshot {
            value: value.clone(),
        }]);

        let packed = message.packed();
        assert_eq!(
            packed.bytes(),
            hex_to_bytes(indoc::indoc! { r#"
                00
                01
                  00
                    52
                      06 61 6e 73 77 65 72       // key "answer"
                      0c 2a                      // value int(42)
                      02 70 69                   // key "pi"
                      7b 4a d8 12 4d fb 21 09 40 // value float(3.1415926)
            "# })
        );

        let unpacked = Message::from_packed(packed).unwrap();
        assert_eq!(unpacked.actions, &[Action::Snapshot { value },]);
    }
}
