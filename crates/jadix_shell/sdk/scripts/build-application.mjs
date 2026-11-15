import { build } from 'esbuild';
import { fileURLToPath } from 'node:url';
for (const name of ['application','structural','capabilities','scaling','controlled-leaves','controlled-limits','counter-leaves']) await build({absWorkingDir: fileURLToPath(new URL('../', import.meta.url)), entryPoints: [`tests/${name}.tsx`], bundle: true, format: 'iife', platform: 'neutral', target: 'es2022', jsx:'automatic', jsxImportSource:'@jadix/shell', outfile:`../tests/fixtures/${name}.js`});
