# Releasing doxsync

The Rust crate and npm package are both named `doxsync` and share one release
version. Both are available under `MIT OR Apache-2.0`.

## Prerequisites

Install Git, Rust/Cargo, Node.js 22+, npm, wasm-pack, and tar. Then install the
WASM target and authenticate with both registries:

```sh
rustup target add wasm32-unknown-unknown
cargo login
npm login --registry https://registry.npmjs.org/
```

Use accounts authorized to publish `doxsync` on each registry, and ensure you can
push the current branch and tags to `origin`. Existing CLI credential providers
and environment configuration are used; the script does not store credentials.
Any npm authentication or two-factor prompts are handled by npm itself.

Commit the release tooling, package metadata, and all intended changes before
running the script. It requires a clean working tree, including untracked files,
and an attached branch. It does not edit or require `CHANGELOG.md`.

## Commands

From the repository root:

```sh
node scripts/release.mjs 0.2.0 --dry-run
node scripts/release.mjs 0.2.0
```

Pass an explicit stable `X.Y.Z` version without a leading `v`. Prereleases and
build metadata are not supported. Both package versions must already agree.
The target cannot be older than the current version. The first release may use
the existing `0.1.0` version; an empty release commit is allowed in that case.
Existing local/remote release tags or registry versions cause an error.

The dry run checks remote tags and registry versions, but requires no registry
publishing credentials. It uses a temporary copy of committed sources, updates
versions there, runs existing Rust tests and Cargo packaging verification,
builds WASM, packs npm, and installs the resulting tarball in another temporary
directory to verify snapshot and patch synchronization. It never creates a
commit or tag, pushes, or uploads. Network access and build caches may be used.

A real release follows these steps:

1. Update both manifests and the root package entry in Cargo.lock, preserving
   dependency versions.
2. Run the same validation and tarball installation checks as the dry run.
3. Commit only the version files with `chore: Release vX.Y.Z.` and create the
   annotated tag `vX.Y.Z` on that commit.
4. Atomically push the current branch and that tag to `origin`. No force push
   is used. This also pushes any preceding local commits on the branch.
5. Publish the Rust package to crates.io with locked dependencies.
6. Publish the already validated npm tarball publicly to registry.npmjs.org
   with the `latest` tag, without rebuilding it.

The tarball remains at `doxsync-js/doxsync-X.Y.Z.tgz`, which Git ignores.

## Failures and recovery

Before the release commit, failure restores the original version files. Fix the
problem and retry. Other build artifacts may remain in ignored directories.

After the release commit, failure preserves the commit, any created tag, and the
npm tarball. The script reports the failed stage and commands for the remaining
steps. Keep the release checkout unchanged while recovering. Do not rerun the
release script: its duplicate-tag/version checks intentionally reject that.

If crates.io succeeded and npm failed, run the printed `npm publish` command on
the retained tarball after resolving the error. Do not create another commit or
change the version to finish the same release.

An upload timeout can occur after a registry accepted the package. Check the
version on the relevant registry before retrying an uncertain upload. Published
versions are not automatically removed, and commits/tags are not reset or deleted.
