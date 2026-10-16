# Migration verification

Verified locally on 2026-10-16 with Node.js 24.3.0, pnpm 9.15.0,
wasm-pack 0.15.0, and Chromium. No unit tests were added. Browser checks used
actual WASM, UI interactions, and temporary browser-console diagnostics; the
application contains no verification hooks or injected failures.

## Build and package checks

- `pnpm install --frozen-lockfile` completed without lockfile changes.
- `pnpm build` built core before playground and passed TypeScript checks.
- `pnpm example` demonstrated snapshots, incremental updates, Decimal values,
  binary values, and unchanged-document behavior in Node.
- `pnpm playground` built core and served the development application.
- `pnpm --filter @doxsync/playground preview` served the production application.
- `pnpm --filter @doxsync/core exec npm pack --dry-run --ignore-scripts`
  listed SDK JavaScript, declarations, WASM, licenses, and minimal examples.
  No playground assets or React dependencies were added to core.
- Playground `dist/` and `node_modules/` are covered by existing Git ignore rules.

## Browser checks

| Area | Observed result |
| --- | --- |
| Development and production loading | WASM initialized and the initial snapshot reached the consumer. |
| Subdirectory hosting | A plain HTTP server served `/packages/playground/dist/`; JavaScript, CSS, and the emitted WASM loaded beneath that prefix. |
| Minimal browser example | Displayed snapshot count `1n`, update count `2n`, and no message for the unchanged document. |
| Pause and single-step | Queued messages arrived while paused; manual changes remained available. Enter activated the focused pause button with a visible focus outline. |
| Mutation coverage | Temporarily controlled random choices exercised every mutation, including Decimal, bigint, arrays, field insertion/removal, and unchanged documents. Delivered documents matched send-time states. |
| Delay ordering | Decreasing delay from 250 ms to zero preserved equal queued deadlines and FIFO delivery. |
| History | After more than 60 deliveries, exactly 60 delivered entries remained. Pending entries were retained in addition to those 60. |
| Selection | Manual selection disabled follow-latest and survived new messages. Enabling follow selected the latest entry. Pruning an old selected entry left a valid selected entry. |
| Reset | Cleared pending messages/history/counters, preserved interval and delay, restored follow-latest, and restarted with one initial snapshot. Old messages did not reach the new consumer. |
| Snapshot contract | Repeated reads retained identity between notifications; snapshots and history arrays were frozen. Disposed controller commands had no effect. |
| Delivery failure | A temporarily injected SDK delivery error retained its error kind and failed message. Restoring the SDK method and resetting recovered synchronization. |
| Initialization failure | A temporary preview server rejected the first WASM fetch. The loading error and retry button appeared; retry successfully synchronized the initial snapshot. |
| Strict Mode and unmount | A temporary React Strict Mode mount created one live simulation. Unmount freed exactly one producer and one consumer; no delivery occurred afterward. |
| Hot reload | Hot reload preserved the active simulation without freeing or duplicating its handles. At a 500 ms update interval, 1.65 seconds produced exactly three changes. Temporary source instrumentation was removed. |
| Page lifecycle | Dispatching `pagehide` freed both handles; a persisted `pageshow` recreated a working simulation. This verifies the event handlers, rather than browser-specific back/forward cache eligibility. |
| Responsive layout | Desktop and 390 px mobile screenshots retained controls, document panels, history, and byte inspection. The mobile page had no horizontal document overflow. |

Production preview had no console warnings or errors. The plain subdirectory
server returned a harmless 404 for the browser's automatic `/favicon.ico` request;
all application assets loaded successfully. No hosting deployment or release was
performed. Cross-browser behavior beyond Chromium was not assessed.

## Independent byte-level inspection

The initial migration checks above predate the inspector extraction. The current
inspector is a separate Rust crate and WASM package; core Rust and core JavaScript
source files have no diagnostic changes.

- Workspace build passed in dependency order: core, inspector, playground.
- Existing Rust tests passed (seven unit tests and one documentation test). No
  unit tests were added. The inspector passed Clippy with warnings denied.
- Node/WASM smoke checks covered 400 generated messages across protocols 1–4.
  Every leaf span partitioned the full input with no gaps or overlaps; all hex
  strings matched their original byte slices. Snapshot values matched the core
  consumer, including bigint, Decimal, binary, sparse objects, and nested arrays.
- All four AOO modes were exercised: dense values, sparse bitmap, integer deltas,
  and Decimal coefficient deltas. `c0 06 03` decoded as AOO / six rows / three
  columns, with separate byte spans for each field.
- The compact listing grouping was checked on 80 further messages: concatenating
  its displayed hex reproduced the original message exactly, without omitted or
  duplicated bytes. All eight action types and scalar edge cases were also
  checked against the consumer.
- A malformed message that overwrote a string slot and then referenced an absent
  slot failed atomically; subsequent pool reuse still resolved the original value.
- Chromium displayed the initial snapshot's actual AOO encoding at bytes 307–345,
  including `c0`, row/column counts, and three column definitions. The final UI
  renders these as a plain annotated hex listing rather than expandable cards.
  History retained
  the saved inspection without replaying the stream. No console errors occurred. A 390 px viewport had no horizontal page overflow.

The inspector diagnoses encoding and resolves references; it does not validate
whether an action path exists in the consumer document. No protocol format,
release, or deployment changes were made.

## Application workspace layout

- Production build and TypeScript checks passed after the layout update. No unit
  tests were added; synchronization and inspector code were unchanged.
- Chromium at 1468 × 846 and 390 × 844 showed no horizontal or vertical page
  overflow. History, wire bytes, and document panes scroll within the viewport.
- Messages / Documents switching retained the selected historical message. Wire
  contents, Decoded actions, and Raw bytes views remained accessible.
- Hide / Show history worked; the inspector gained the available space. On mobile,
  hiding history provided a 414 px tall byte area in the checked viewport.
- Pause, Change once, Reset, document comparison, and initial snapshot inspection
  were exercised. No browser console warnings or errors were reported.

## Simultaneous Messages and Documents panels

- Production build and TypeScript checks passed. No unit tests were added.
- Desktop inspection confirmed Messages and Documents are visible side by side.
  Hiding either panel expands the other to fill the workspace.
- Hide / Show retained the selected initial snapshot and both wire and document
  scroll positions (150 px and 100 px respectively).
- Both panels can be hidden and restored using Show both panels.
- At 390 × 844, the panels stack in a scrolling workspace, Documents remains
  reachable, and the page has no horizontal or vertical overflow.
- Chromium reported no console warnings or errors.

## Compact toolbar and spacing

- Combined branding, simulation controls, and panel visibility into one toolbar.
  At 1440 × 900, the desktop toolbar measured 57 px high.
- History uses continuous rows with aligned 16 px desktop padding. Summaries
  occupy their own row, separate from message size and delivery status.
- At 390 × 844, toolbar controls wrap within the same header. Both visibility
  controls and pause were exercised; no page overflow or console errors occurred.
- Production build, TypeScript checks, and git diff whitespace checks passed.

## Inspector module and typed-model refactor

- Native inspection now returns typed actions, paths, document values, exact
  decimals, and byte-positioned errors. The WASM JSON schema remains unchanged.
- A temporary Node/WASM differential check compared 420 messages against the
  pre-refactor WASM with exact JSON equality, including annotations and spans.
  It covered protocols 1–4, all eight actions, all four AOO column modes, large
  integers, decimals, binary values, non-finite floats, and negative zero.
- Recursive span checks confirmed complete, non-overlapping byte coverage and
  matching hex slices. All 2,371 truncated snapshot prefixes were rejected.
- A failed string-pool overwrite rolled back; subsequent references resolved the
  original string. The JS wrapper's snapshots matched the core consumer after
  normalizing Decimal instances by value.
- Existing Rust tests passed (seven unit tests and one documentation test). No
  unit tests were added. Native and wasm32 Clippy passed with warnings denied.
- Inspector WASM/TypeScript and playground production builds passed. Chromium
  loaded the rebuilt inspector WASM, displayed `c0 06 03` with the AOO explanation,
  and showed decoded snapshot values and raw bytes without console errors.
- Core Rust and JavaScript SDK source files remain unchanged.
