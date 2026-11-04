import { build } from 'esbuild';
import { mkdir } from 'node:fs/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { readFile } from 'node:fs/promises';
import assert from 'node:assert/strict';
const working = fileURLToPath(new URL('../', import.meta.url));
const destination = fileURLToPath(new URL('../../../../.theme-evidence/native-shell/acceptance/sdk/', import.meta.url));
await mkdir(destination, { recursive: true });
for (const platform of ['neutral', 'browser']) {
  await build({ absWorkingDir: working, entryPoints: ['tests/panel.tsx'], bundle: true, format: 'esm', platform, target: 'es2020', sourcemap: true, jsx: 'automatic', jsxImportSource: '@jadix/shell', outfile: `${destination}/panel-${platform}.mjs` });
}
const { default: tree } = await import(pathToFileURL(`${destination}/panel-neutral.mjs`).href);
const envelope = { protocol: 1, session: '1', command: { type: 'mount', root: '2', tree } };
const fixture = JSON.parse(await readFile(new URL('../../tests/fixtures/tsx-panel.json', import.meta.url), 'utf8'));
assert.deepEqual(envelope, fixture);
