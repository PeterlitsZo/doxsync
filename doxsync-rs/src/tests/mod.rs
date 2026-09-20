use crate::message::{Action, Path, PathSegment};
use crate::value;

use super::*;

#[track_caller]
fn assert_message(
    producer: &mut Producer,
    consumer: &mut Consumer,
    to_update: &Document,
    actions: &[Action],
) {
    // Produce the first diff message.
    let diff = producer
        .produce_diff_unpacked()
        .expect("producer should produce a diff message")
        .expect("diff should be Some(T)");
    assert_eq!(diff.actions(), actions);
    let diff = producer
        .pack_diff(diff)
        .expect("diff should be packed successfully");

    // Consume the diff message.
    consumer
        .consume_diff(diff)
        .expect("diff should be consumed successfully");

    // Verify the consumer document matches the initial document.
    let consumer_document = consumer
        .document()
        .expect("consumer should have a document after consuming the diff");
    assert_eq!(consumer_document, to_update);
}

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
    let diff = producer.pack_diff(diff).unwrap();

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
    let diff = producer.pack_diff(diff).unwrap();

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
    let diff = producer.pack_diff(diff).unwrap();

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
fn test_produce_then_consume_complex_document() {
    let document = Document::new(
        value!({
            "foo": 42,
            "bar": 43,
        })
        .unwrap(),
    );
    let mut producer = Producer::new(document.clone());
    let mut consumer = Consumer::new();
    assert_message(
        &mut producer,
        &mut consumer,
        &document,
        &[Action::Snapshot {
            value: value!({
                "foo": 42,
                "bar": 43,
            })
            .unwrap(),
        }],
    );

    // Case 1:
    // =========================================================================
    let document = Document::new(
        value!({
            "foo": 42,
            "bar": 43,
            "baz": 44,
        })
        .unwrap(),
    );
    producer.replace(document.clone());
    assert_message(
        &mut producer,
        &mut consumer,
        &document,
        &[Action::Add {
            path: Path::new(vec![PathSegment::key("baz")]),
            value: value!(44).unwrap(),
        }],
    );

    // Case 2:
    // =========================================================================
    let document = Document::new(
        value!({
            "foo": { "bar": { "baz": 42 } },
            "bar": 43,
            "baz": 44,
        })
        .unwrap(),
    );
    producer.replace(document.clone());
    assert_message(
        &mut producer,
        &mut consumer,
        &document,
        &[Action::Add {
            path: Path::new(vec![PathSegment::key("foo")]),
            value: value!({ "bar": { "baz": 42 } }).unwrap(),
        }],
    );

    // Case 3:
    // =========================================================================
    let document = Document::new(
        value!({
            "foo": { "bar": { "baz": 42 } },
            "bar": { "bar": { "baz": 42 } },
            "baz": 44,
        })
        .unwrap(),
    );
    producer.replace(document.clone());
    assert_message(
        &mut producer,
        &mut consumer,
        &document,
        &[Action::Copy {
            path: Path::new(vec![PathSegment::key("bar")]),
            from: Path::new(vec![PathSegment::key("foo")]),
        }],
    );

    // // Case 4:
    // // =========================================================================
    // let document = Document::new(
    //     value!({
    //         "foo": { "bar": { "baz": 42 } },
    //         "bar": { "bar": { "baz": 43 } },
    //         "baz": 44,
    //     })
    //     .unwrap(),
    // );
    // producer.replace(document.clone());
    // assert_message(
    //     &mut producer,
    //     &mut consumer,
    //     &document,
    //     &[Action::Add {
    //         path: Path::new(vec![
    //             PathSegment::key("bar"),
    //             PathSegment::key("bar"),
    //             PathSegment::key("baz"),
    //         ]),
    //         value: value!(43).unwrap(),
    //     }],
    // );
}
