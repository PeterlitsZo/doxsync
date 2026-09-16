use std::{collections::BTreeMap, sync::Arc};

use crate::message::{Action, Path, PathSegment};

use super::*;

#[test]
fn test_produce_then_consume_simple_document() {
    let initial_docuemnt = Document::new(Value::int(42).unwrap());
    let mut producer = Producer::new(initial_docuemnt.clone());
    let mut consumer = Consumer::new();

    // Produce the first diff message.
    let diff = producer.produce_diff().unwrap().unwrap();
    assert_eq!(
        diff.actions(),
        &[Action::Snapshot {
            value: Value::int(42).unwrap()
        }]
    );

    // Consume the diff message.
    consumer.consume_diff(diff).unwrap();

    // Verify the consumer document matches the initial document.
    let consumer_document = consumer.document().unwrap();
    assert_eq!(*consumer_document, initial_docuemnt);

    // Update the producer document and produce a new diff message.
    let updated_document = Document::new(Value::int(43).unwrap());
    producer.replace(updated_document.clone());
    let diff = producer.produce_diff().unwrap().unwrap();
    assert_eq!(
        diff.actions(),
        &[Action::Snapshot {
            value: Value::int(43).unwrap()
        }]
    );

    // Consume the diff message.
    consumer.consume_diff(diff).unwrap();

    // Verify the consumer document matches the updated document.
    let consumer_document = consumer.document().unwrap();
    assert_eq!(*consumer_document, updated_document);

    // Update multiple times.
    producer.replace(Document::new(Value::int(-42).unwrap()));
    producer.replace(Document::new(Value::float(3.1415926).unwrap()));

    // Consume the diff message.
    let diff = producer.produce_diff().unwrap().unwrap();
    assert_eq!(
        diff.actions(),
        &[Action::Snapshot {
            value: Value::float(3.1415926).unwrap()
        }]
    );

    // Consume the diff message.
    consumer.consume_diff(diff).unwrap();

    // Verify the consumer document matches the updated document.
    let consumer_document = consumer.document().unwrap();
    assert_eq!(
        *consumer_document,
        Document::new(Value::float(3.1415926).unwrap())
    );
}

#[test]
fn test_produce_then_consume_mapping_document() {
    let docuemnt = Document::new(
        Value::map(BTreeMap::from([
            (Arc::new("foo".to_string()), Value::int(42).unwrap()),
            (Arc::new("bar".to_string()), Value::int(43).unwrap()),
        ]))
        .unwrap(),
    );
    let mut producer = Producer::new(docuemnt.clone());
    let mut consumer = Consumer::new();

    // Produce the first diff message.
    let diff = producer.produce_diff().unwrap().unwrap();
    assert_eq!(
        diff.actions(),
        &[Action::Snapshot {
            value: Value::map(BTreeMap::from([
                (Arc::new("foo".to_string()), Value::int(42).unwrap()),
                (Arc::new("bar".to_string()), Value::int(43).unwrap()),
            ]))
            .unwrap()
        }]
    );

    // Consume the diff message.
    consumer.consume_diff(diff).unwrap();

    // Verify the consumer document matches the initial document.
    let consumer_document = consumer.document().unwrap();
    assert_eq!(*consumer_document, docuemnt);

    // Modify the document and produce a new diff message.
    let modified_docuemnt = docuemnt
        .modify(|value| {
            *value = value.as_map_and_modify(|m| {
                m.insert(Arc::new("baz".to_string()), Value::int(44).unwrap());
                Ok(())
            })?;
            Ok(())
        })
        .unwrap();
    producer.replace(modified_docuemnt.clone());
    let diff = producer.produce_diff().unwrap().unwrap();

    assert_eq!(
        diff.actions(),
        &[Action::Add {
            path: Path::new(vec![PathSegment::key("baz")]),
            value: Value::int(44).unwrap(),
        }]
    );

    // Consume the diff message.
    consumer.consume_diff(diff).unwrap();

    // Verify the consumer document matches the modified document.
    let consumer_document = consumer.document().unwrap();
    assert_eq!(*consumer_document, modified_docuemnt);
}

#[test]
fn test_consume_diff_applies_actions_atomically_and_in_order() {
    let empty_map = Value::map(BTreeMap::new()).unwrap();
    let mut consumer = Consumer::new();
    consumer
        .consume_diff(Message::new(vec![Action::Snapshot {
            value: empty_map.clone(),
        }]))
        .unwrap();

    consumer
        .consume_diff(Message::new(vec![
            Action::Add {
                path: Path::new(vec![PathSegment::key("foo")]),
                value: Value::int(42).unwrap(),
            },
            Action::Delete {
                path: Path::new(vec![PathSegment::key("foo")]),
            },
        ]))
        .unwrap();
    assert_eq!(consumer.document().unwrap().value(), empty_map);

    let invalid_diff = Message::new(vec![
        Action::Add {
            path: Path::new(vec![PathSegment::key("foo")]),
            value: Value::int(42).unwrap(),
        },
        Action::Delete {
            path: Path::new(vec![PathSegment::key("missing")]),
        },
    ]);
    assert!(consumer.consume_diff(invalid_diff).is_err());
    assert_eq!(consumer.document().unwrap().value(), empty_map);
}
