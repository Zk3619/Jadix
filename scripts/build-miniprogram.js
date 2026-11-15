'use strict';

const fs = require('node:fs');
const path = require('node:path');
const childProcess = require('node:child_process');
const zlib = require('node:zlib');
const crypto = require('node:crypto');

const root = path.resolve(__dirname, '..');
const toolRequire = require('node:module').createRequire(path.join(root, 'examples', 'miniprogram', 'package.json'));
const args = process.argv.slice(2);
const consistentUi = args.includes('--consistent-ui');
const positional = args.filter(argument => argument !== '--consistent-ui');
if (positional.length > 1 || positional.some(argument => argument.startsWith('--'))) throw new Error('usage: node scripts/build-miniprogram.js [output] [--consistent-ui]');
const output = path.resolve(positional[0] || path.join(root, 'target', 'miniprogram-project'));
if (fs.existsSync(output)) throw new Error('output exists; choose a new directory to preserve existing artifacts');
const rustflags = '-C target-cpu=mvp -C target-feature=-reference-types,-multivalue,-bulk-memory,-nontrapping-fptoint,-sign-ext';
const environment = {...process.env, CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS: rustflags};
const features = consistentUi ? ['consistent-ui'] : [];
const result = childProcess.spawnSync('cargo', ['build', '--locked', '-p', 'jadix_miniprogram', '--target', 'wasm32-unknown-unknown', '--profile', 'miniprogram', ...(consistentUi ? ['--features', 'consistent-ui'] : [])], {cwd: root, stdio: 'inherit', shell: false, env: environment});
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status || 1);
const directory = path.resolve(root, process.env.CARGO_TARGET_DIR || 'target', 'wasm32-unknown-unknown', 'miniprogram');
const wasmOpt = path.join(path.dirname(toolRequire.resolve('binaryen/package.json')), 'bin', 'wasm-opt');
const lowering = ['--enable-bulk-memory', '--enable-bulk-memory-opt', '--enable-sign-ext', '--enable-nontrapping-float-to-int',
  '--enable-reference-types', '--enable-multivalue', '--llvm-memory-copy-fill-lowering', '--llvm-nontrapping-fptoint-lowering', '--signext-lowering', '--strip-target-features'];
for (const args of [
  [path.join(directory, 'jadix_miniprogram.wasm'), '-o', path.join(directory, 'jadix_miniprogram.lowered.wasm'), ...lowering],
  [path.join(directory, 'jadix_miniprogram.lowered.wasm'), '-o', path.join(directory, 'jadix_miniprogram.mvp.wasm'), '--mvp-features', '--strip-target-features'],
]) {
  const result = childProcess.spawnSync(process.execPath, [wasmOpt, ...args], {cwd: root, stdio: 'inherit', shell: false});
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status || 1);
}
const wasm = fs.readFileSync(path.join(directory, 'jadix_miniprogram.mvp.wasm'));
const imports = WebAssembly.Module.imports(new WebAssembly.Module(wasm));
const expected = ['dispose', 'measure', 'publish'];
if (imports.some(item => item.module !== 'jadix' || item.kind !== 'function' || !expected.includes(item.name))) throw new Error('unsupported WebAssembly host imports: ' + JSON.stringify(imports));
fs.cpSync(path.join(root, 'examples', 'miniprogram', 'app'), output, {recursive: true, errorOnExist: true});
fs.cpSync(path.join(root, 'crates', 'jadix_native', 'platforms', 'miniprogram'), path.join(output, 'jadix'), {recursive: true, filter: source => !source.endsWith('.test.js')});
const compressed = zlib.brotliCompressSync(wasm, {params: {[zlib.constants.BROTLI_PARAM_QUALITY]: 11}});
fs.writeFileSync(path.join(output, 'app.wasm.br'), compressed);
function inventory(directory, prefix = '') {
  return fs.readdirSync(directory, {withFileTypes: true}).flatMap(entry => {
    const relative = path.posix.join(prefix, entry.name), filename = path.join(directory, entry.name);
    if (entry.isDirectory()) return inventory(filename, relative);
    const content = fs.readFileSync(filename);
    return [{path: relative, bytes: content.length, sha256: crypto.createHash('sha256').update(content).digest('hex')}];
  });
}
const files = inventory(output), bytes = files.reduce((total, file) => total + file.bytes, 0);
const receipt = {status: bytes <= 2 * 1024 * 1024 ? 'pass' : 'package-size-warning', wasm_bytes: wasm.length,
  compressed_wasm_bytes: compressed.length, project_bytes: bytes, wasm_sha256: crypto.createHash('sha256').update(wasm).digest('hex'),
  rustflags, features, binaryen_version: toolRequire('binaryen/package.json').version, lowering, validated_mvp: true, imports, files,
  limits: ['Compilation and package validation only; WeChat DevTools and device rendering require separate verification.']};
fs.writeFileSync(path.join(output, 'build-receipt.json'), JSON.stringify(receipt, null, 2) + '\n');
console.log(JSON.stringify({...receipt, files: files.length, output}));
