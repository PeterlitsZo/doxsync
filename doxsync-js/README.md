# doxsync JavaScript workspace

This pnpm workspace contains the `@doxsync/core` SDK in `packages/core/` and
the private React `@doxsync/playground` application in `packages/playground/`,
plus the private `@doxsync/inspector` diagnostic WASM package.
Packages share `pnpm-lock.yaml` at the workspace root.

Use Node.js 22.12+ (or 24+), pnpm 9.15.0, Rust, and wasm-pack. From this directory:

```sh
pnpm install --frozen-lockfile
pnpm build
pnpm example
pnpm playground
pnpm --dir packages/core pack
```

See [the core package documentation](packages/core/README.md) for the API,
WASM setup, and minimal framework-free browser example. See the
[playground documentation](packages/playground/README.md) for the interactive
browser demo, architecture, and static hosting instructions. `pnpm playground`
builds core and inspector before starting the development server. After `pnpm build`, use
`pnpm --filter @doxsync/playground preview` to preview the production application.
Build outputs live in each package's `dist/` directory. The recursive build
checks playground types and builds core, inspector, then playground.
The release script uses pnpm to install and build, and npm to inspect tarballs
and publish only `@doxsync/core` to the npm registry. The private playground and inspector are built during release validation but are
not published or versioned with the SDK.
