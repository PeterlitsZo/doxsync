# doxsync playground

A private React application showing real Rust/WASM synchronization through a
simulated, ordered message queue. It is a workspace application, not an npm
package or a public React integration API.

## Run locally

Use Node.js 22.12+ (or 24+), pnpm 9.15.0, Rust, and wasm-pack. From `doxsync-js/`:

```sh
pnpm install --frozen-lockfile
pnpm playground
```

The command builds `@doxsync/core` and `@doxsync/inspector` before starting Vite. UI changes hot-reload.
After changing core or Rust, stop the server and run `pnpm playground` again to
rebuild both WASM modules and refresh Vite's dependency cache if necessary (`dev --force`).

For a production build and local preview:

```sh
pnpm build
pnpm --filter @doxsync/playground preview
```

The recursive build orders core, inspector, then playground and checks playground types.
To check types separately after building core and inspector:

```sh
pnpm --filter @doxsync/playground typecheck
```

## Workspace layout

The application fills the viewport with one top toolbar combining the brand,
simulation controls, and panel visibility buttons, plus a bottom status bar.
Messages and Documents appear side by side by default. Each has an independent
Hide / Show control; hiding one gives the other the full width. If both are hidden,
a Show both panels button restores the workspace. Panel visibility does not pause
the simulation or clear the selected message.

Messages contains a narrow history panel and an inspector with Wire contents,
Decoded actions, and Raw bytes views. Hide history to give the inspector more
room. Documents shows the live producer and consumer stacked beside Messages,
or side by side when Documents has the full desktop workspace. Each content pane
scrolls independently. On small screens, Messages and Documents stack vertically
inside the scrolling workspace, with the visibility controls always available.

## Architecture and behavior

- `src/simulation/controller.ts` owns SDK handles, mutation and delivery timers,
  the FIFO queue, history, and consistency checks. Its immutable snapshots contain
  display data, not DOM nodes or WASM handles.
- `src/simulation/document.ts` defines the sample document and random mutations.
  Its `tasks` array contains objects with `id`, `title`, and `done` fields.
  Mutations toggle a task's completion, insert or remove tasks, and change their
  order. The list stays between one and eight tasks (initially six), with unique IDs among the
  current entries. IDs are sample data; synchronization still uses the SDK's
  document and array semantics.
  `format.ts` preserves bigint and Decimal types in the displayed document.
- `src/hooks/useSimulation.ts` initializes both WASM modules, subscribes React to the controller,
  and cleans up resources across unmounts and page navigation. The display clock
  is independent of message scheduling.
- `src/components/` renders controls, documents, message history, and an annotated hex listing, decoded operations, and raw bytes.
  Selection and follow-latest behavior belong to the UI.

Pause stops automatic mutations but lets queued messages arrive. Change once
also works while paused. Delay changes affect new messages only; shortening delay
never lets a later message overtake an earlier one. Reset preserves interval and
delay settings, clears history and counters, and starts a fresh simulation.
The history retains the latest 60 delivered messages and every pending message.
A delivery error stops the simulation and remains visible until reset.

## Static output and package boundaries

Deploy the contents of `dist/` with any static HTTP server. Relative asset URLs
support hosting beneath a directory; no server API or route fallback is required.
Serve `.wasm` as `application/wasm`. The WASM URLs come from `@doxsync/core/doxsync.wasm` and
`@doxsync/inspector/inspector.wasm`; both are emitted as separate build assets.

The playground depends on core's built public entry points and does not import
SDK source files. Core has no React or inspector dependency. The workspace release workflow
also builds this application, but only packages and publishes `@doxsync/core` to
npm; playground's local version is independent of SDK release versions.

For the smallest framework-free integration, see
[the core browser example](../core/examples/browser.html).

See [migration verification](VERIFICATION.md) for the completed browser and
packaging checks and their scope.

Message details show the operations decoded from each sent message: its protocol,
paths, values, and array edit ranges. An independent `MessageInspector` from `@doxsync/inspector` follows the
send stream, so inspection works before delayed delivery and selecting historical
messages does not replay bytes through the decoder. The default Wire contents
view is a plain, indented hex listing with short inline comments:

```text
02 # 2 metadatas
  02 04 # Use protocol 4
  00 1b # Apply string pool patch, 27 entries
    00 06 6f 6e 6c 69 6e 65 # strings[0] = "online"
```

Each byte appears exactly once, in wire order. Counts, pool definitions, action
opcodes, paths, and payloads are grouped into readable lines. AOO headers show
`c0 06 03` as AOO / six rows / three columns; indented column data identifies key
references, modes, bitmap presence, and delta arithmetic. The listing is directly
selectable and copyable. Decoded actions and raw hex remain separate views.

The parser lives in `doxsync-rs/crates/doxsync-inspector`, independently of the
core engine. See [inspector documentation](../inspector/README.md) for its stream
contract, supported encodings, and diagnostic resource limits.
