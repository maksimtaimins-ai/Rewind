// Moves the NSIS release artifacts for the current version into nsis/<version>/
// and clears everything else at the top of nsis/. Existing version folders stay.
// Usage: node scripts/organize-release.js (run after `tauri build`)
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), '..');
const conf = JSON.parse(fs.readFileSync(path.join(root, 'src-tauri/tauri.conf.json'), 'utf8'));
// Tauri v2 keeps the version at the top level; v1 had it under package.
const version = conf.version ?? conf.package?.version;
if (!version) {
  console.error('No version in src-tauri/tauri.conf.json');
  process.exit(1);
}

const nsisRel = 'src-tauri/target/release/bundle/nsis';
const nsis = path.join(root, nsisRel);
if (!fs.existsSync(nsis)) {
  console.error(`${nsisRel} not found; run \`npm run tauri build\` first`);
  process.exit(1);
}

const installer = `Rewind_${version}_x64-setup.exe`;
const keep = [installer, `${installer}.sig`];

// A hand-written latest.json is kept only if it describes this version.
const latestPath = path.join(nsis, 'latest.json');
let haveLatest = false;
if (fs.existsSync(latestPath)) {
  let latestVersion;
  try {
    latestVersion = JSON.parse(fs.readFileSync(latestPath, 'utf8')).version;
  } catch {}
  if (latestVersion === undefined || latestVersion === version) haveLatest = true;
  else console.warn(`latest.json is for ${latestVersion}, not ${version}; deleting it`);
}

if (!fs.existsSync(path.join(nsis, installer))) {
  console.error(`${installer} not found in ${nsisRel}`);
  process.exit(1);
}

// tauri build doesn't write latest.json, so build one from the .sig for the updater.
const sigPath = path.join(nsis, `${installer}.sig`);
if (!haveLatest && fs.existsSync(sigPath)) {
  const latest = {
    version,
    notes: `Rewind v${version}`,
    pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, 'Z'),
    platforms: {
      'windows-x86_64': {
        signature: fs.readFileSync(sigPath, 'utf8').trim(),
        url: `https://github.com/maksimtaimins-ai/Rewind/releases/download/v${version}/${installer}`,
      },
    },
  };
  fs.writeFileSync(latestPath, JSON.stringify(latest, null, 2) + '\n');
  console.log(`wrote:   latest.json`);
  haveLatest = true;
}
if (haveLatest) keep.push('latest.json');

const out = path.join(nsis, version);
fs.mkdirSync(out, { recursive: true });
for (const name of keep) {
  const src = path.join(nsis, name);
  if (!fs.existsSync(src)) {
    console.warn(`missing: ${name}`);
    continue;
  }
  fs.renameSync(src, path.join(out, name));
  console.log(`moved:   ${name}`);
}

// Anything left at the top level that isn't a version folder is build leftovers.
const versionDir = /^\d+\.\d+\.\d+([-+].*)?$/;
for (const entry of fs.readdirSync(nsis, { withFileTypes: true })) {
  if (entry.isDirectory() && versionDir.test(entry.name)) continue;
  fs.rmSync(path.join(nsis, entry.name), { recursive: true, force: true });
  console.log(`deleted: ${entry.name}`);
}

console.log(`Release v${version} ready at nsis/${version}/`);
