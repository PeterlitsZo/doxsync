use crate::message::{Action, Path};
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
fn test_produce_then_consume_001() {
    let initial_docuemnt = Document::new(value!(42).unwrap());
    let mut producer = Producer::new(initial_docuemnt.clone());
    let mut consumer = Consumer::new();

    // Produce the first diff message.
    let diff = producer.produce_diff_unpacked().unwrap().unwrap();
    assert_eq!(diff.actions(), &[Action::snapshot(value!(42).unwrap())]);
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
    assert_eq!(diff.actions(), &[Action::snapshot(value!(43).unwrap())]);
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
        &[Action::snapshot(value!(3.1415926).unwrap())]
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
fn test_produce_then_consume_002() {
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
        &[Action::snapshot(
            value!({
                "foo": 42,
                "bar": 43,
            })
            .unwrap(),
        )],
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
        &[Action::add(
            Path::parse("baz").unwrap(),
            value!(44).unwrap(),
        )],
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
        &[Action::add(
            Path::parse("foo").unwrap(),
            value!({ "bar": { "baz": 42 } }).unwrap(),
        )],
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
        &[Action::copy(
            Path::parse("bar").unwrap(),
            Path::parse("foo").unwrap(),
        )],
    );

    // Case 4:
    // =========================================================================
    let document = Document::new(
        value!({
            "foo": { "bar": { "baz": 42 } },
            "bar": { "bar": { "baz": 43 } },
            "baz": 44,
        })
        .unwrap(),
    );
    producer.replace(document.clone());
    assert_message(
        &mut producer,
        &mut consumer,
        &document,
        &[Action::add(
            Path::parse("bar.bar.baz").unwrap(),
            value!(43).unwrap(),
        )],
    );

    // Case 5:
    // =========================================================================
    let document = Document::new(
        value!({
            "foo": { "bar": { "baz": 42 } },
            "bar": { "bar": { "baz": 43 } },
            "baz": 44,
            "users": [
                { "name": "Peterlits", "id": "user_1" },
                { "name": "Foobar", "id": "user_2" },
            ],
        })
        .unwrap(),
    );
    producer.replace(document.clone());
    assert_message(
        &mut producer,
        &mut consumer,
        &document,
        &[Action::add(
            Path::parse("users").unwrap(),
            value!([
                { "name": "Peterlits", "id": "user_1" },
                { "name": "Foobar", "id": "user_2" },
            ])
            .unwrap(),
        )],
    );

    // Case 6:
    // =========================================================================
    let document = Document::new(
        value!({
            "foo": { "bar": { "baz": 42 } },
            "bar": { "bar": { "baz": 43 } },
            "baz": 44,
            "users": [
                { "name": "Peterlits", "id": "user_1" },
                { "name": "Foobar", "id": "user_2" },
            ],
            "users_by_id": {}
        })
        .unwrap(),
    );
    producer.replace(document.clone());
    assert_message(
        &mut producer,
        &mut consumer,
        &document,
        &[Action::add(
            Path::parse("users_by_id").unwrap(),
            value!({}).unwrap(),
        )],
    );

    // Case 7:
    // =========================================================================
    let document = Document::new(
        value!({
            "foo": { "bar": { "baz": 42 } },
            "bar": { "bar": { "baz": 43 } },
            "baz": 44,
            "users": [
                { "name": "Peterlits", "id": "user_1" },
                { "name": "Foobar", "id": "user_2" },
            ],
            "users_by_id": {
                "user_1": { "name": "Peterlits", "id": "user_1" },
                "user_2": { "name": "Foobar", "id": "user_2" },
            }
        })
        .unwrap(),
    );
    producer.replace(document.clone());
    assert_message(
        &mut producer,
        &mut consumer,
        &document,
        &[
            Action::copy(
                Path::parse("users_by_id.user_1").unwrap(),
                Path::parse("users.0").unwrap(),
            ),
            Action::copy(
                Path::parse("users_by_id.user_2").unwrap(),
                Path::parse("users.1").unwrap(),
            ),
        ],
    );
}

#[test]
fn test_produce_then_consume_003() {
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
        &[Action::snapshot(
            value!({
                "foo": 42,
                "bar": 43,
            })
            .unwrap(),
        )],
    );

    // Case 1:
    // =========================================================================
    let document = Document::new(value!(42).unwrap());
    producer.replace(document.clone());
    assert_message(
        &mut producer,
        &mut consumer,
        &document,
        &[Action::copy(Path::empty(), Path::parse("foo").unwrap())],
    );

    // Case 2:
    // =========================================================================
    let document = Document::new(
        value!({
            "users": [{ "age": 42, "name": "Peterlits Zo" }]
        })
        .unwrap(),
    );
    producer.replace(document.clone());
    assert_message(
        &mut producer,
        &mut consumer,
        &document,
        &[Action::snapshot(value!({
            "users": [{ "age": 42, "name": "Peterlits Zo" }]
        }).unwrap())],
    );

    // Case 2:
    // =========================================================================
    let document = Document::new(
        value!({
            "users": [{ "age": 25, "name": "Peterlits Zo" }]
        })
        .unwrap(),
    );
    producer.replace(document.clone());
    assert_message(
        &mut producer,
        &mut consumer,
        &document,
        &[Action::add(
            Path::parse("users.0.age").unwrap(),
            value!(25).unwrap(),
        )],
    );

    // Case 3:
    // =========================================================================
    let document = Document::new(
        value!({
            "users": [{ "age": 25, "name": "Peterlits Zo" }, 42]
        })
        .unwrap(),
    );
    producer.replace(document.clone());
    assert_message(
        &mut producer,
        &mut consumer,
        &document,
        &[Action::add(
            Path::parse("users.1").unwrap(),
            value!(42).unwrap(),
        )],
    );

    // Case 4:
    // =========================================================================
    let document = Document::new(
        value!({
            "users": [
                { "age": 25, "name": "Peterlits Zo" },
                { "age": 42, "name": "Foobar" },
            ]
        })
        .unwrap(),
    );
    producer.replace(document.clone());
    assert_message(
        &mut producer,
        &mut consumer,
        &document,
        &[Action::replace(
            Path::parse("users.1").unwrap(),
            value!({ "age": 42, "name": "Foobar" }).unwrap(),
        )],
    );

    // Case 5:
    // =========================================================================
    let document = Document::new(
        value!({
            "users": [
                { "age": 25, "name": "Peterlits Zo" },
                { "age": 42, "name": "Foobar II" },
            ]
        })
        .unwrap(),
    );
    producer.replace(document.clone());
    assert_message(
        &mut producer,
        &mut consumer,
        &document,
        &[Action::add(
            Path::parse("users.1.name").unwrap(),
            value!("Foobar II").unwrap(),
        )],
    );
}
