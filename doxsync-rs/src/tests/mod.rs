use crate::message::Action;

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
