use std::sync::Arc;

use crate::message::{Action, Path, PathSegment};
use crate::value;

use super::*;

#[test]
fn test_produce_then_consume_simple_document() {
    let initial_docuemnt = Document::new(value!(42).unwrap());
    let mut producer = Producer::new(initial_docuemnt.clone());
    let mut consumer = Consumer::new();

    // Produce the first diff message.
    let diff = producer.produce_diff_unpacked().unwrap().unwrap();
    assert_eq!(
        diff.actions(),
        &[Action::Snapshot {
            value: value!(42).unwrap()
        }]
    );
    let diff = producer.pack_diff(diff);

    // Consume the diff message.
    consumer.consume_diff(diff).unwrap();

    // Verify the consumer document matches the initial document.
    let consumer_document = consumer.document().unwrap();
    assert_eq!(*consumer_document, initial_docuemnt);

    // Update the producer document and produce a new diff message.
    let updated_document = Document::new(value!(43).unwrap());
    producer.replace(updated_document.clone());
    let diff = producer.produce_diff_unpacked().unwrap().unwrap();
    assert_eq!(
        diff.actions(),
        &[Action::Snapshot {
            value: value!(43).unwrap()
        }]
    );
    let diff = producer.pack_diff(diff);

    // Consume the diff message.
    consumer.consume_diff(diff).unwrap();

    // Verify the consumer document matches the updated document.
    let consumer_document = consumer.document().unwrap();
    assert_eq!(*consumer_document, updated_document);

    // Update multiple times.
    producer.replace(Document::new(value!(-42).unwrap()));
    producer.replace(Document::new(value!(3.1415926).unwrap()));

    // Consume the diff message.
    let diff = producer.produce_diff_unpacked().unwrap().unwrap();
    assert_eq!(
        diff.actions(),
        &[Action::Snapshot {
            value: value!(3.1415926).unwrap()
        }]
    );
    let diff = producer.pack_diff(diff);

    // Consume the diff message.
    consumer.consume_diff(diff).unwrap();

    // Verify the consumer document matches the updated document.
    let consumer_document = consumer.document().unwrap();
    assert_eq!(
        *consumer_document,
        Document::new(value!(3.1415926).unwrap())
    );
}

#[test]
fn test_produce_then_consume_mapping_document() {
    let docuemnt = Document::new(
        value!({
            "foo": 42,
            "bar": 43,
        })
        .unwrap(),
    );
    let mut producer = Producer::new(docuemnt.clone());
    let mut consumer = Consumer::new();

    // Produce the first diff message.
    let diff = producer.produce_diff_unpacked().unwrap().unwrap();
    assert_eq!(
        diff.actions(),
        &[Action::Snapshot {
            value: value!({
                "foo": 42,
                "bar": 43,
            })
            .unwrap()
        }]
    );
    let diff = producer.pack_diff(diff);

    // Consume the diff message.
    consumer.consume_diff(diff).unwrap();

    // Verify the consumer document matches the initial document.
    let consumer_document = consumer.document().unwrap();
    assert_eq!(*consumer_document, docuemnt);

    // Modify the document and produce a new diff message.
    let modified_docuemnt = docuemnt
        .modify(|value| {
            *value = value.as_map_and_modify(|m| {
                m.insert(Arc::new("baz".to_string()), value!(44).unwrap());
                Ok(())
            })?;
            Ok(())
        })
        .unwrap();
    producer.replace(modified_docuemnt.clone());
    let diff = producer.produce_diff_unpacked().unwrap().unwrap();
    assert_eq!(
        diff.actions(),
        &[Action::Add {
            path: Path::new(vec![PathSegment::key("baz")]),
            value: value!(44).unwrap(),
        }]
    );
    let diff = producer.pack_diff(diff);

    // Consume the diff message.
    consumer.consume_diff(diff).unwrap();

    // Verify the consumer document matches the modified document.
    let consumer_document = consumer.document().unwrap();
    assert_eq!(*consumer_document, modified_docuemnt);
}
