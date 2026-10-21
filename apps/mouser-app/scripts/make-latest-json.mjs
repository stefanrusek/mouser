// Builds latest.json (tauri-plugin-updater static format, see
// specs/001-mouser-v1/contracts/release-manifest.schema.json) from the signed
// updater bundles collected from the three build jobs.
// Usage: node make-latest-json.mjs <assets-dir> <tag> <owner/repo> > latest.json
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const [dir, tag, repo] = process.argv.slice(2);
const files = readdirSync(dir);

function entry(suffix) {
  const matches = files.filter((f) => f.endsWith(suffix));
  if (matches.length !== 1) {
    throw new Error(`expected exactly one *${suffix} in ${dir}, found ${matches.length}`);
  }
  const name = matches[0];
  return {
    url: `https://github.com/${repo}/releases/download/${tag}/${encodeURIComponent(name)}`,
    signature: readFileSync(join(dir, `${name}.sig`), 'utf8').trim(),
  };
}

const mac = entry('_universal.app.tar.gz');
const manifest = {
  version: tag,
  notes: `Mouser ${tag}`,
  pub_date: new Date().toISOString(),
  platforms: {
    // One universal bundle serves both Mac architectures.
    'darwin-aarch64': mac,
    'darwin-x86_64': mac,
    'windows-x86_64': entry('-setup.exe'),
    'linux-x86_64': entry('.AppImage'),
  },
};
process.stdout.write(`${JSON.stringify(manifest, null, 2)}\n`);
