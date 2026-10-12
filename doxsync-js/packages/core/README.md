# @doxsync/core

Synchronize documents with compact updates in modern browsers and Node.js 22+.
ESM and TypeScript are supported; your application supplies the transport.

## Install

```sh
npm install @doxsync/core
```

## Quick start

```js
import { init, Producer, Consumer, supportedProtocols } from "@doxsync/core";

await init();
const producer = new Producer({ count: 1n }, supportedProtocols());
const consumer = new Consumer();

try {
  consumer.consumeDiff(producer.produceDiff());
  producer.replace({ count: 2n });

  const message = producer.produceDiff();
  if (message !== undefined) consumer.consumeDiff(message);

  console.log(consumer.document()); // { count: 2n }
} finally {
  producer.free();
  consumer.free();
}
```

## API and delivery

- `await init(source?)`: initialize once before using the API. Other calls are synchronous.
- `supportedProtocols()`: get supported versions. For remote peers, pass the
  **consumer's** advertised list to `new Producer(value, protocols)`.
- `producer.replace(value)`: validate and copy a new document without sending it.
- `producer.produceDiff()`: return a `Uint8Array`, or `undefined` if unchanged.
  The first message contains the complete document.
- `consumer.consumeDiff(message)`: apply one complete message. Invalid messages
  throw and leave the consumer unchanged.
- `consumer.document()`: return an independent copy, or `undefined` before the first message.
- `free()`: release each instance when finished.

Deliver messages exactly once and in order to the matching consumer. Keep each
message until delivered: calling `produceDiff()` again does not retransmit it.
If either side loses its session state, create a new producer and consumer.

## Supported values

Use `null`, booleans, strings, numbers, `bigint`, `Uint8Array`, dense arrays,
and plain objects with string keys. Nested values are copied by value.
`number` and `bigint` are distinct; integers must be in `[-2^64, 2^64 - 1]`.
`undefined`, cycles, sparse arrays, getters, functions, and objects such as
`Date`, `Map`, or custom class instances are not supported.

For exact decimals, import `Decimal` from this package and use strings:

```js
import { Decimal } from "@doxsync/core";
const price = new Decimal("12.34");
```

Decimals must be finite, fit a 96-bit coefficient, and have scale 0–28;
unsupported values are rejected without rounding. Trailing zeros are not preserved.
Protocol 1 delivers decimals as strings; protocols 2 and 3 preserve Decimal values.
The producer automatically selects the highest version supported by both peers.

Errors may expose `kind`: `InvalidData`, `UnexpectedType`, or `Internal`.
Initialization and lifecycle errors may be ordinary JavaScript errors.

## Browser setup

Deploy the packaged WASM asset alongside the JavaScript. If your bundler relocates
it, pass its deployed URL or bytes to `init(source)`. The asset is exported as
`@doxsync/core/doxsync.wasm`; serve `.wasm` files as `application/wasm`.
Node.js loads the packaged asset automatically. Without a bundler, also provide
an import map for `decimal.js`; see the [browser example](examples/browser.html).

## Build from source

Requires Rust, wasm-pack, Node.js 22.12+ (or 24+), and pnpm 9.15.0.

```sh
rustup target add wasm32-unknown-unknown
cd doxsync-js
pnpm install --frozen-lockfile
pnpm build
pnpm example
```

To run the browser example, serve `doxsync-js/` with `python3 -m http.server 8000`
and open `/packages/core/examples/browser.html`.
