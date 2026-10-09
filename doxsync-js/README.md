# doxsync JavaScript workspace

This pnpm workspace contains `@doxsync/core` in `packages/core/`. Packages share
`pnpm-lock.yaml` at the workspace root. Add future packages under `packages/`.

Use Node.js 22.12+ (or 24+), pnpm 9.15.0, Rust, and wasm-pack. From this directory:

```sh
pnpm install --frozen-lockfile
pnpm build
pnpm example
pnpm --dir packages/core pack
```

See [the core package documentation](packages/core/README.md) for the API,
WASM setup, and browser example. Build output lives in `packages/core/dist/`.
The release script uses pnpm to install and build, and npm to inspect tarballs
and publish to the npm registry.
