# @doxsync/inspector

Private workspace bindings for the independent `doxsync-inspector` Rust crate.
The crate has no dependency on the core engine and builds a separate WASM module.
The core SDK does not include this package or expose `MessageInspector`.

## Usage

Build from `doxsync-js/` with `pnpm build`, or run `pnpm playground` to build the
application's dependencies and start it. In Vite:

```ts
import { init, MessageInspector } from "@doxsync/inspector";
import wasmUrl from "@doxsync/inspector/inspector.wasm?url";
await init(wasmUrl);
const inspector = new MessageInspector();
try {
  for (const bytes of messages) {
    const { protocol, metadataCount, actions, wire } = inspector.inspect(bytes);
    console.log(protocol, metadataCount, actions, wire);
  }
} finally {
  inspector.free();
}
```

For Node, pass bytes read from `dist/wasm/doxsync_inspector_bg.wasm` to `init`.
Initialization is shared across concurrent callers and can be retried on failure.

Feed every message once, in order, starting at the protocol declaration. The
inspector owns string, path, and binary pools and rolls back all state on parse
failure. Reset a stream by freeing and creating an inspector. Save returned
reports for history; do not replay old messages into the live inspector.

`actions` contains decoded action types, typed paths, and values (including bigint,
Decimal, and Uint8Array). It does not apply actions to a document.

`wire` is an ordered tree of fields. Each has `label`, zero-based `offset`, byte
`length`, exact `hex`, `explanation`, and `children`. `annotation` is a short listing comment; `prefix`
groups that many leading children on the same line. Remaining children are
indented below it. The playground uses these hints to render plain annotated hex. Children partition a compound
field's range; all leaves together cover the complete message. Empty literals
may have zero-length spans. Metadata and action count prefixes are included.

Supported encodings include protocols 1–4, all metadata and action types, inline
and pooled keys/values, little-endian scalar widths, LEB128, IEEE 754 floats,
Decimal coefficients/scales, and every AOO column mode. For `c0 06 03`:

| Bytes | Meaning |
| --- | --- |
| `c0` | AOO tag (high nibble `1100`), reserved low nibble `0000` |
| `06` | Unsigned LEB128 row count: 6 |
| `03` | Unsigned LEB128 column count: 3 |

Columns then contain a key descriptor, mode byte, and mode-specific data. Reports
explain string slot resolution, sparse bitmap bits, ZigZag decoding, and cumulative
delta reconstruction. No producer-side optimization decision is inferred: the
report describes the encoding actually present in the message.

## Limits and maintenance

This is a diagnostic tool, not a substitute for consumer validation. It does not
check document-dependent action validity. To bound diagnostic expansion it limits
messages to 16 MiB, value nesting to 128, and reports to 500,000 fields; these are
inspector limits, not new protocol restrictions. Large reports remain more costly
than normal decoding because they retain bytes and explanations at each level.

Maintain this independent parser alongside protocol changes. Its source of truth
is the implemented format in the core protocol and packed encoder/decoder files;
`WIRE.md` lists supported versions. Smoke-check with real producer messages and
compare decoded snapshots with the consumer, plus full leaf-byte coverage.
