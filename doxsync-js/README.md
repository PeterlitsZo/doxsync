# @doxsync/core

ESM bindings for the doxsync Rust synchronization engine, for modern browsers
and Node.js 22+. Rust owns documents, diffing, message encoding, decoding, and
state. JavaScript provides loading and a small synchronous API.

## Build from this repository

Install Rust, Node.js 22.12+ (or 24+), and `wasm-pack`, then run:

```sh
rustup target add wasm32-unknown-unknown
cd doxsync-js
npm ci
npm run build
node examples/node.mjs
```

`npm run build` runs `wasm-pack` in release mode with the locked Cargo
dependencies and the `wasm` feature, then builds the TypeScript wrapper with Vite in library mode.
`vite-plugin-dts` generates type declarations in `dist/`. The runtime dependency
`decimal.js` supplies exact JS decimal values. To create a local npm
tarball, run `npm pack`; it builds first and includes the WASM artifact.
Installing that tarball does not require Rust or wasm-pack.

The Rust crate continues to support native use without JS dependencies:

```sh
cargo build --manifest-path ../doxsync-rs/Cargo.toml
cargo build --manifest-path ../doxsync-rs/Cargo.toml \
  --target wasm32-unknown-unknown --features wasm --release
```

The second command produces the Rust WASM artifact. Use `npm run build` to
also generate the JS glue required to load it.

## Synchronize a document

After installing the package from the local tarball:

```js
import { init, Producer, Consumer, supportedProtocols } from "@doxsync/core";

await init();
const producer = new Producer({ count: 1n, ratio: 0.5 }, supportedProtocols());
const consumer = new Consumer();

try {
  consumer.consumeDiff(producer.produceDiff()); // First message is a snapshot.
  producer.replace({ count: 2n, ratio: 0.75 });

  const message = producer.produceDiff();
  if (message !== undefined) consumer.consumeDiff(message);

  console.log(consumer.document()); // { count: 2n, ratio: 0.75 }
  console.log(producer.produceDiff()); // undefined: no changes
} finally {
  producer.free();
  consumer.free();
}
```

Each `consumeDiff` call takes one complete `Uint8Array` message. Transport is
up to the application. Messages must be delivered reliably, exactly once, and
in order to the corresponding consumer: the producer advances its baseline
when it returns a message. Retain those bytes until delivery; calling
`produceDiff` again does not retransmit them. For a new connection without
shared state, create a new producer and consumer and start with a snapshot.

`replace` copies and validates the input before changing the producer. It
does not emit a message. `document()` returns `undefined` before a snapshot,
and an independent JS value afterwards (including `null` for a null document).
Mutating an input, returned document, or returned message does not change the
engine's stored document. Invalid messages leave consumer state unchanged.

Call `free()` when finished to release WASM resources promptly. It is
idempotent; business methods throw after release. The generated bindings also
provide garbage-collection cleanup, but its timing is not deterministic.

## WASM initialization

Call and await `init()` before creating either class or calling
`supportedProtocols()`. Initialization is the
only asynchronous operation. Concurrent calls share the first call's promise
and source; successful initialization is reused, and failed initialization
can be retried.

- Browsers fetch the WASM next to the generated JS. Serve `dist/` over HTTP;
  retain the `dist/wasm/` directory and serve `.wasm` as `application/wasm`.
  When loading without a bundler, provide an import map for `decimal.js`;
  `examples/browser.html` maps it to the local npm installation.
- Node.js uses the package's conditional export to read the same packaged
  WASM from disk. Browser entry points contain no Node.js imports.
- `init(source)` also accepts a URL string, `URL`, `ArrayBuffer`, `Uint8Array`,
  or compiled `WebAssembly.Module`. Strings are URLs, not filesystem paths.
  Node.js additionally reads `file:` URLs from disk.
- Bundlers must deploy the WASM asset. If their asset handling changes its
  location, pass the deployed URL or bytes explicitly; the package exports
  the asset as `@doxsync/core/doxsync.wasm` for resolution by build tools.

For a browser example, serve this directory and open `/examples/browser.html`:

```sh
python3 -m http.server 8000
```

The browser demo changes the producer randomly every 1.5 seconds and delivers
messages after a simulated 1-second delay. It shows both documents, highlights
updated lines, and lists the actual message bytes and delivery status. Adjust
the delay or update interval, pause automatic changes, step manually, or reset
the session. Pausing lets queued messages finish; changing latency never
reorders messages. Some iterations deliberately leave the document unchanged
to demonstrate that no message is produced.

Browsers must support WebAssembly, BigInt, ESM, and private class fields. No
CommonJS entry point or polyfills are supplied.

## Values and errors

| JavaScript | doxsync |
| --- | --- |
| `null`, `boolean`, `string` | Null, Bool, UTF-8 text |
| `number` | Float, including `-0`, NaN, and infinities |
| `bigint` | Int in `[-2^64, 2^64 - 1]` |
| `Decimal` from `decimal.js` | Exact Decimal with a 96-bit coefficient and scale 0–28 |
| `Uint8Array` (including Node Buffer) | Binary string, copied |
| Dense arrays | Arrays, copied recursively |
| Plain or null-prototype objects | Maps with string keys |

`42` and `42n` are different document values. Integers from Rust always read
back as `bigint`, including small integers. Floating-point NaN values remain
NaN, but JS does not promise to preserve NaN payload bits.

Maps use own enumerable string properties, not inherited or non-enumerable
properties. Output maps are ordinary objects; keys such as `__proto__` are
defined as safe own data properties. Arrays serialize their indexed elements;
extra named properties are not part of the document. Object identity and
insertion order are not preserved. Shared references are copied by value.

Unsupported values are rejected: `undefined`, functions, symbols (including
enumerable symbol map keys), sparse arrays, cyclic references, accessor
properties used as document values, out-of-range integers, unpaired UTF-16
surrogates in strings or keys, and objects such as Date, Map, Set, or class
instances other than Decimal. Use values from the same JS realm as the library;
custom prototypes and proxies are not supported as a document model.

Rust and conversion errors are JS `Error` instances with `kind` equal to
`Internal`, `InvalidData`, or `UnexpectedType`. Initialization, lifecycle,
and host JavaScript exceptions may be ordinary errors without `kind`.
TypeScript declarations include `SyncValue`, `InitSource`, and `DoxsyncError`.

```js
try {
  consumer.consumeDiff(receivedBytes);
} catch (error) {
  console.error(error.kind, error.message);
}
```

The JS package exposes `init`, `supportedProtocols`, `Producer`, `Consumer`, and `Decimal`
at runtime.
Rust's lower-level Document, Value, and Message APIs remain internal to this
binding, and the wire protocol is shared with native Rust.

### Decimal values

```js
import { init, Producer, Consumer, Decimal, supportedProtocols } from "@doxsync/core";

await init();
const producer = new Producer({ price: new Decimal("12.34") }, supportedProtocols());
const consumer = new Consumer();
try {
  consumer.consumeDiff(producer.produceDiff());
  console.log(consumer.document().price.toFixed(2)); // "12.34"
} finally {
  producer.free();
  consumer.free();
}
```

Pass decimal strings to the Decimal constructor to avoid introducing binary
floating-point rounding before synchronization. Instances imported directly from
`decimal.js` or created by `Decimal.clone()` are also accepted. Values must be
finite and exactly representable using a sign and a 96-bit unsigned coefficient
divided by `10^scale`, with scale from 0 through 28. Invalid values throw `InvalidData`;
conversion never silently rounds. Input `number` values remain Float values.

Protocol 2 returns decimal.js instances, constructed independently of the user's
global decimal.js configuration with 29 significant digits of arithmetic
precision and an exponent range of -100 through 100. Their initial values are
exact. Arithmetic follows decimal.js precision rules; synchronization does not
implement arithmetic or recover precision already lost by the caller.

Rust preserves scale: `Decimal::from_str_exact("1.20")` remains distinct from
`Decimal::from_str_exact("1.2")` when wrapped with `Value::decimal`. It also
provides `as_decimal`, `as_decimal_and_modify`, and `value!` conversions for owned
or borrowed Decimal values. Negative zero is normalized without changing scale.
JS decimal.js instances do not retain trailing zeros: a Rust value of `1.20`
read through `document()` and sent back through JS may become `1.2`.

With a protocol 1 producer (`new Producer(value, [1])`), decimals arrive as
ordinary strings. Rust preserves the source scale in those strings; JS can only
send the scale still present in its decimal.js value. Strings are never
automatically parsed as decimals. Switching between a Decimal and its identical
fallback string produces no change under protocol 1.

### Protocol negotiation

Call `supportedProtocols()` after `await init()` to get all protocol versions
supported by the local build, currently `[1, 2]`. Each call returns an independent
ordinary `number[]`; modifying it does not affect subsequent calls or the engine.
No producer or consumer instance is needed. Rust callers can use
`doxsync::supported_protocols() -> &'static [u32]` without initialization.

For peers running in separate processes, have the consumer advertise its list
and pass that received list to the producer. The examples above use the local
list because both peers share the same build. This query reports supported
versions, not the version negotiated for an individual stream.

The second `Producer` constructor argument is required: pass an array of protocol
versions supported by the consumer, such as `[1]`. Versions must be integers in
`[0, 2^32 - 1]`. The producer selects the highest version supported by both peers;
order and duplicates do not matter. Versions `1` and `2` are implemented.
Empty or incompatible lists, omitted arguments, and invalid values throw an error
with `kind: "InvalidData"`.

The first packed message starts its metadata with protocol instruction `2` followed
by the selected version; both are unsigned variable-length integers.
Later messages omit this declaration and retain the same protocol for the stream.
The consumer rejects first messages without a leading protocol declaration,
unsupported versions, and repeated declarations. Legacy messages without version
metadata are not accepted. Existing protocol 1 peers can continue to receive
updates, with decimals represented as strings.

Failed encoding or consumption preserves protocol initialization state as well as
pool state, allowing retries. Keep and deliver packed messages in order as usual.

### Decimal wire encoding

Protocol 2 represents `sign × coefficient × 10^-scale`. The first byte's high
nibble is `0x9` for nonnegative decimals or `0xA` for negative decimals; `0x6`
remains reserved. Its low nibble directly contains scales 0–14. A low nibble of
`0xF` is followed by one byte containing the complete scale, 15–28.

The absolute coefficient follows as minimal unsigned LEB128: seven data bits
per byte, least significant group first, with the high bit indicating continuation.
Zero uses one `00` byte. There is no ZigZag transform or negative offset.
Coefficients occupy 1–14 bytes and must fit in 96 bits. The fourteenth byte
must have its continuation bit clear and contain at most `0x1F`.

| Value | Hex bytes (excluding the message header) |
| --- | --- |
| `0.01` | `92 01` |
| `1.20` | `92 78` |
| `12.34` | `92 D2 09` |
| `-12.34` | `A2 D2 09` |
| `0.00` | `92 00` |
| `0.000000000000001` | `9F 0F 01` |

The maximum value encoding length is 16 bytes. Decoders reject negative zero,
non-minimal coefficients, invalid scales, overflow, truncation, and Decimal
tags in protocol 1. Existing Int encoding is unchanged.

### Path key encoding

Protocol v1 path definitions use the low two bits of each segment's unsigned
variable-length integer as a tag: `0b00` carries a UTF-8 byte length followed by
the key bytes, `0b01` carries an array index, and `0b10` carries a string pool
index. The payload is shifted left by two bits before adding the tag; `0b11`
is reserved and rejected.

A cached key uses `0b10` only when its reference is strictly shorter than its
literal encoding. Uncached keys remain inline without adding string pool entries.
The initial snapshot populates the string pool, so subsequent path definitions
can reuse those keys. Once a whole path is cached, actions reuse its path pool ID.
String pool updates precede path definitions; referenced strings stay resident
through message encoding and are resolved when the consumer reads the definition.

This extends v1 directly. Both peers must be upgraded together: older decoders
reject the new `0b10` path segment tag.

### Compact floating-point values

Protocol 2 encodes numbers as binary16, binary32, or
binary64, choosing the shortest representation that preserves the original
binary64 bits. Decoded values remain JavaScript numbers. Signed zero is
preserved; NaNs retain binary64 encoding. Protocol 1 continues to use
binary64 for every number.
