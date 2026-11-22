import { createRequire } from 'node:module';
import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { createHash, createPrivateKey, createPublicKey, sign } from 'node:crypto';
const sdk = new URL('../../../crates/jadix_shell/sdk/', import.meta.url);
const require = createRequire(new URL('package.json', sdk));
const esbuild = require('esbuild');
const mobile = process.argv.includes('--mobile');
const entry = mobile ? 'mobile' : 'panel';
async function artifact(name, bytes) {
  const path = new URL(name, import.meta.url);
  if (process.argv.includes('--check')) {
    if (!Buffer.from(await readFile(path)).equals(Buffer.from(bytes))) throw new Error(`stale TSX artifact: ${name}`);
  } else await writeFile(path, bytes);
}
const result = await esbuild.build({
  absWorkingDir: fileURLToPath(new URL('.', import.meta.url)),
  entryPoints: [`${entry}.tsx`], bundle: true, write: false,
  format: 'iife', globalName: 'NativePanel', platform: 'neutral', target: 'es2020',
  jsx: 'automatic', jsxImportSource: '@jadix/shell',
  plugins: [{ name: 'jadix-sdk', setup(build) {
    build.onResolve({ filter: /^@jadix\/shell(?:\/jsx-runtime)?$/ }, args => ({
      path: fileURLToPath(new URL(args.path.endsWith('/jsx-runtime') ? 'src/jsx-runtime.mjs' : 'src/index.mjs', sdk)),
    }));
  } }],
});
const source = result.outputFiles[0].contents;
await artifact(`${entry}.bundle.js`, source);
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const report = { schema: 1, compiler: `esbuild@${esbuild.version}`, bundle_sha256: digest(source), sources: {} };
for (const path of [`${entry}.tsx`, '../../../crates/jadix_shell/sdk/src/index.mjs', '../../../crates/jadix_shell/sdk/src/jsx-runtime.mjs', '../../../crates/jadix_shell/sdk/src/generated.mjs']) {
  report.sources[path] = digest(await readFile(new URL(path, import.meta.url)));
}
await artifact(`${entry}.bundle.json`, JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify(report));
if (mobile) process.exit(0);
const privateKey = createPrivateKey({ key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), Buffer.alloc(32, 7)]), format: 'der', type: 'pkcs8' });
const publicKey = createPublicKey(privateKey).export({ format: 'der', type: 'spki' }).subarray(-32).toString('hex');
const brokenSource = "throw new Error('rejected development update');";
const schema = digest(Buffer.concat([
  await readFile(new URL('../../../crates/jadix_components/schema/portable-v1.json', import.meta.url)),
  Buffer.from([0]),
  await readFile(new URL('../../../crates/jadix_components/schema/extended-v1.json', import.meta.url)),
]));
const releases = {};
for (const release of [1, 2, 3]) {
  const entry = release === 3 ? Buffer.from(brokenSource) : source;
  const manifest = {
    format: 1, package: 'examples.native.tsx', version: `1.0.${release}`, publisher: 'jadix-development', key_id: 'development-key', target: 'native', release: String(release),
    protocol: 1, sdk: '0.1.0', minimum_host: '0.1.0', component_schema: schema, capabilities: [], entry: 'panel.bundle.js',
    resources: { 'panel.bundle.js': { sha256: digest(entry), bytes: entry.length } }, permissions: { 'panel.read': 'Read the panel host state' },
    storage_namespace: 'examples.native.tsx', storage_version: release === 2 ? 2 : 1, migration_from: release === 2 ? [1] : [],
  };
  const input = Buffer.concat([Buffer.from('JADIX-SHELL-PACKAGE\0\x01'), Buffer.from(JSON.stringify(manifest))]);
  releases[release] = { manifest, signature: sign(null, input, privateKey).toString('hex') };
}
await artifact('development-packages.json', JSON.stringify({ schema: 1, development_only: true, public_key: publicKey, broken_source: brokenSource, releases }, null, 2) + '\n');
