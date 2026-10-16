# doxsync-inspector

Independent stateful disassembly of doxsync protocol versions 1–4. This private
crate has no dependency on `doxsync` and does not alter the synchronization engine.
It diagnoses wire encoding and resolves pool references; it does not apply actions
or validate their paths against a consumer document.

## Rust API

`Inspector::default()` starts an empty ordered stream. Feed each complete message
once, beginning with its protocol declaration. `inspect(&[u8])` returns
`Result<Inspection, Error>`:

- `Inspection.actions` contains typed `Action` variants, with `Path`, `ArrayEdit`,
  and recursive `Value` data. Matching an action does not require JSON indexing.
- `Value::Integer` holds an `i128`, covering the wire's signed integer range.
  `Value::Decimal` holds an exact `Decimal { coefficient, scale }`. Delta decoding
  uses checked integer arithmetic directly.
- `Inspection.wire` is an ordered tree of `Field` spans. Children partition each
  compound span. Leaves explain their bytes; `annotation` and `prefix` describe
  how to render the compact annotated hex listing.
- `Error` implements `std::error::Error` and exposes `offset` and `message`.
  Its display form is `Byte <offset>: <message>`.

A parse uses a working copy of the stream state and commits it only after the
whole message passes validation, including the trailing-byte check. Any error
leaves the protocol and all three pools unchanged. `state()` exposes a serializable
checkpoint; clone it for `Inspector::new(state)` to resume another inspector.

## Module boundaries

| Module | Responsibility |
| --- | --- |
| `lib.rs` | Public exports and transactional stream inspection |
| `model.rs` | Typed actions, paths, values, and exact decimals |
| `state.rs` | Protocol declaration and string, path, binary pools |
| `error.rs` | Byte-positioned public errors |
| `field.rs` | Public byte-span and listing model |
| `parser/mod.rs` | Message ordering and protocol requirements |
| `parser/cursor.rs` | Checked byte slicing and cursor position |
| `parser/primitives.rs` | LEB128, ZigZag, UTF-8, header payloads |
| `parser/references.rs` | Key descriptors, paths, and pool references |
| `parser/metadata.rs` | Protocol declarations and pool patches |
| `parser/actions.rs` | Action payloads and array-edit validation |
| `parser/value.rs` | Tagged scalar and recursive container decoding |
| `parser/aoo.rs` | Column orchestration, dense/sparse values, integer/decimal deltas |
| `parser/trace.rs` | Field construction and listing annotations |
| `parser/limits.rs` | Protocol bounds and diagnostic resource budgets |
| `wasm.rs` | Thin WASM adapter and JSON serialization |

Decoded data never depends on parsing an explanation string. Key provenance is
carried explicitly, and presentation is derived from the decoded fields. JSON in
metadata comments is only formatting; protocol state and action data are typed.

## JavaScript boundary

The WASM export remains `MessageInspector`; `inspect` returns JSON. The native
Rust API now exposes typed actions and errors instead of `serde_json::Value` and
`String`, respectively. The serialized JavaScript schema is unchanged.

Action values use explicit `{ "kind": ..., "value": ... }` wrappers throughout,
including maps and arrays, so user keys cannot collide with diagnostic markers.
Integers, exact decimals, and floats serialize as strings to preserve precision
and non-finite values. The JS wrapper converts these into ordinary SDK value types.

Build and UI integration are owned by `doxsync-js/packages/inspector`. See that
package's README for the field schema, examples, limits, and protocol-maintenance
contract. Changes to the wire grammar belong in the corresponding parser module;
changes to the listing presentation belong in annotations and the frontend
formatter, without affecting protocol state.
