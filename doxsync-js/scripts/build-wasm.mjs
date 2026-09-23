import { spawnSync } from "node:child_process";
import { mkdir, rm } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const root = new URL("../", import.meta.url);
const dist = new URL("dist/", root);
await rm(dist, { recursive: true, force: true });
await mkdir(dist, { recursive: true });

const result = spawnSync("wasm-pack", [
  "build", fileURLToPath(new URL("../doxsync-rs/", root)),
  "--target", "web",
  "--release",
  "--out-dir", fileURLToPath(new URL("wasm/", dist)),
  "--out-name", "doxsync",
  "--no-pack",
  "--", "--locked", "--features", "wasm",
], { stdio: "inherit" });

if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);

// Use the outer npm manifest. wasm-pack's generated manifest has a different
// package name and would also obscure the outer package's ESM configuration.
await rm(new URL("wasm/package.json", dist), { force: true });
// wasm-pack ignores its entire output directory. npm would honor that nested
// ignore file even though the outer package explicitly includes dist/.
await rm(new URL("wasm/.gitignore", dist), { force: true });
