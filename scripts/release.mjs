#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync, mkdtempSync, mkdirSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const npmRegistry = 'https://registry.npmjs.org/';
const versionFiles = ['doxsync-rs/Cargo.toml', 'doxsync-rs/Cargo.lock', 'doxsync-js/package.json'];
const args = process.argv.slice(2);
const dryRun = args.includes('--dry-run');
const versions = args.filter(arg => arg !== '--dry-run');
const version = versions[0];
const stable = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
const quote = value => "'" + value.replaceAll("'", "'\\''") + "'";
let stage = 'preflight';
let temporary;
let saved;
let changed = false;
let committed = false;
let branch;
let tarball;

function run(command, args, cwd = root, capture = false) {
  console.log(`> ${command} ${args.map(quote).join(' ')}`);
  const result = spawnSync(command, args, {
    cwd, stdio: capture ? ['inherit', 'pipe', 'inherit'] : 'inherit', encoding: 'utf8',
    maxBuffer: 32 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} failed (${result.signal ?? result.status})`);
  return result.stdout?.trim();
}
function check(condition, message) { if (!condition) throw new Error(message); }
function compare(a, b) {
  const left = a.split('.').map(BigInt), right = b.split('.').map(BigInt);
  for (let i = 0; i < 3; i++) if (left[i] !== right[i]) return left[i] > right[i] ? 1 : -1;
  return 0;
}
async function absent(url, label) {
  const response = await fetch(url, { signal: AbortSignal.timeout(30000), headers: { 'User-Agent': 'doxsync-release (https://github.com/PeterlitsZo/doxsync)' } });
  if (response.status === 404) return;
  check(!response.ok, `${label} already exists`);
  throw new Error(`Cannot check ${label}: HTTP ${response.status}`);
}
function replaceVersion(text, pattern, label) {
  let count = 0;
  const output = text.replace(pattern, (_, prefix) => { count++; return `${prefix}"${version}"`; });
  check(count === 1, `Expected exactly one ${label} version`);
  return output;
}
function prepare(directory) {
  const cargoPath = join(directory, versionFiles[0]);
  const lockPath = join(directory, versionFiles[1]);
  const jsonPath = join(directory, versionFiles[2]);
  const cargo = readFileSync(cargoPath, 'utf8');
  const manifest = JSON.parse(readFileSync(jsonPath, 'utf8'));
  const packageSection = cargo.match(/^\[package\]\s*\n([\s\S]*?)(?=^\[|$(?![\s\S]))/m)?.[1];
  const current = packageSection?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  check(current && stable.test(current) && manifest.version === current, 'Rust and npm versions must match and be stable X.Y.Z versions');
  check(compare(version, current) >= 0, 'Release version cannot go backwards');
  check(/^name\s*=\s*"doxsync"/m.test(packageSection) && manifest.name === 'doxsync', 'Both packages must be named doxsync');
  check(manifest.license === 'MIT OR Apache-2.0' && /^license\s*=\s*"MIT OR Apache-2.0"/m.test(packageSection), 'Both packages must declare MIT OR Apache-2.0');
  for (const pkg of ['doxsync-rs', 'doxsync-js']) for (const license of ['LICENSE-MIT', 'LICENSE-APACHE']) {
    check(existsSync(join(directory, pkg, license)), `Missing ${pkg}/${license}`);
  }
  writeFileSync(cargoPath, cargo.replace(packageSection, replaceVersion(packageSection, /^(version\s*=\s*)"[^"]+"/gm, 'Cargo package')));
  writeFileSync(lockPath, replaceVersion(readFileSync(lockPath, 'utf8'), /(^\[\[package\]\]\s*\nname = "doxsync"\s*\nversion = )"[^"]+"/gm, 'Cargo lock package'));
  manifest.version = version;
  writeFileSync(jsonPath, JSON.stringify(manifest, null, 2) + '\n');
}
function validate(directory) {
  const rust = join(directory, 'doxsync-rs');
  const js = join(directory, 'doxsync-js');
  run('cargo', ['test', '--locked'], rust);
  run('cargo', ['publish', '--dry-run', '--locked', '--allow-dirty', '--registry', 'crates-io'], rust);
  const crateFiles = new Set(run('cargo', ['package', '--list', '--locked', '--allow-dirty'], rust, true).split('\n'));
  for (const license of ['LICENSE-MIT', 'LICENSE-APACHE']) {
    check(crateFiles.has(license), `Rust package is missing ${license}`);
  }
  run('npm', ['run', 'build'], js);
  run('node', ['examples/node.mjs'], js);
  const info = JSON.parse(run('npm', ['pack', '--ignore-scripts', '--json'], js, true));
  check(info.length === 1 && info[0].name === 'doxsync' && info[0].version === version, 'Unexpected npm package identity');
  const files = new Set(info[0].files.map(file => file.path));
  for (const file of ['dist/index.js', 'dist/node.js', 'dist/runtime.js', 'dist/index.d.ts', 'dist/wasm/doxsync.js', 'dist/wasm/doxsync_bg.wasm', 'LICENSE-MIT', 'LICENSE-APACHE']) {
    check(files.has(file), `npm tarball is missing ${file}`);
  }
  tarball = join(js, info[0].filename);
  const smoke = mkdtempSync(join(tmpdir(), 'doxsync-install-'));
  try {
    writeFileSync(join(smoke, 'package.json'), '{"private":true,"type":"module"}\n');
    run('npm', ['install', '--ignore-scripts', '--no-audit', '--no-fund', '--package-lock=false', tarball], smoke);
    run('node', ['--input-type=module', '-e', `
      import assert from 'node:assert/strict';
      import { init, Producer, Consumer } from 'doxsync';
      await init();
      const producer = new Producer({ count: 1n }, [1]);
      const consumer = new Consumer();
      try {
        consumer.consumeDiff(producer.produceDiff());
        assert.deepEqual(consumer.document(), { count: 1n });
        producer.replace({ count: 2n });
        consumer.consumeDiff(producer.produceDiff());
        assert.deepEqual(consumer.document(), { count: 2n });
        assert.equal(producer.produceDiff(), undefined);
      } finally { producer.free(); consumer.free(); }
    `], smoke);
  } finally { rmSync(smoke, { recursive: true, force: true }); }
}

try {
  check(versions.length === 1 && args.length === (dryRun ? 2 : 1) && stable.test(version), 'Usage: node scripts/release.mjs X.Y.Z [--dry-run]');
  check(Number(process.versions.node.split('.')[0]) >= 22, 'Node.js 22+ is required');
  for (const tool of ['git', 'cargo', 'npm', 'wasm-pack']) run(tool, ['--version']);
  check(run('git', ['status', '--porcelain', '--untracked-files=all'], root, true) === '', 'Commit or stash all changes before releasing');
  branch = run('git', ['symbolic-ref', '--short', 'HEAD'], root, true);
  run('git', ['remote', 'get-url', 'origin'], root, true);
  check(run('git', ['tag', '--list', `v${version}`], root, true) === '', `Local tag v${version} already exists`);
  check(run('git', ['ls-remote', '--tags', 'origin', `refs/tags/v${version}`], root, true) === '', `Remote tag v${version} already exists`);
  await absent(`https://crates.io/api/v1/crates/doxsync/${version}`, `crates.io doxsync ${version}`);
  await absent(`${npmRegistry}doxsync/${version}`, `npm doxsync ${version}`);
  let directory = root;
  if (dryRun) {
    temporary = mkdtempSync(join(tmpdir(), 'doxsync-release-'));
    const archive = join(temporary, 'source.tar');
    run('git', ['archive', '--format=tar', `--output=${archive}`, 'HEAD']);
    directory = join(temporary, 'source');
    mkdirSync(directory);
    run('tar', ['-xf', archive, '-C', directory]);
  } else {
    saved = versionFiles.map(file => readFileSync(join(root, file)));
    changed = true;
  }
  stage = 'version update and validation';
  prepare(directory);
  validate(directory);
  if (dryRun) {
    console.log(`Dry run passed for v${version}. No commit, tag, push or upload was performed.`);
  } else {
    const expected = new Set(versionFiles);
    const changedFiles = run('git', ['diff', '--name-only'], root, true).split('\n').filter(Boolean);
    check(changedFiles.every(file => expected.has(file)), 'Validation unexpectedly changed other tracked files');
    stage = 'release commit';
    run('git', ['add', '--', ...versionFiles]);
    run('git', ['commit', '--allow-empty', '-m', `chore: Release v${version}.`]);
    committed = true;
    stage = 'release tag';
    run('git', ['tag', '-a', `v${version}`, '-m', `Release v${version}.`]);
    stage = 'Git push';
    run('git', ['push', '--atomic', 'origin', `HEAD:refs/heads/${branch}`, `refs/tags/v${version}`]);
    stage = 'crates.io publish';
    run('cargo', ['publish', '--locked', '--registry', 'crates-io'], join(root, 'doxsync-rs'));
    stage = 'npm publish';
    run('npm', ['publish', tarball, '--ignore-scripts', '--access', 'public', '--tag', 'latest', '--registry', npmRegistry]);
    console.log(`Released doxsync v${version} to crates.io and npm.`);
  }
} catch (error) {
  console.error(`Release stopped during ${stage}: ${error.message}`);
  if (changed && !committed) {
    versionFiles.forEach((file, index) => writeFileSync(join(root, file), saved[index]));
    run('git', ['reset', '--quiet', 'HEAD', '--', ...versionFiles]);
    console.error('Restored the original version files. Fix the error and run the release command again.');
  }
  if (committed) {
    console.error('The release commit and any created tag are preserved. Do not rerun the release script.');
    console.error(`Run remaining steps from ${quote(root)} after fixing the error:`);
    if (stage === 'release tag') console.error(`git tag -a ${quote(`v${version}`)} -m ${quote(`Release v${version}.`)}`);
    if (['release tag', 'Git push'].includes(stage)) console.error(`git push --atomic origin ${quote(`HEAD:refs/heads/${branch}`)} ${quote(`refs/tags/v${version}`)}`);
    if (stage !== 'npm publish') console.error('cargo publish --manifest-path doxsync-rs/Cargo.toml --locked --registry crates-io');
    if (tarball) console.error(`npm publish ${quote(tarball)} --ignore-scripts --access public --tag latest --registry ${npmRegistry}`);
    console.error('An upload timeout may still mean success. Check the registry version before retrying that upload.');
  }
  process.exitCode = 1;
} finally {
  if (temporary) rmSync(temporary, { recursive: true, force: true });
}
