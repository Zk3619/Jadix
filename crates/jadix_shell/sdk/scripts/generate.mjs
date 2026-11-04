import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const schemas = (await Promise.all(['portable-v1.json', 'extended-v1.json'].map(async name =>
  JSON.parse(await readFile(new URL(`../../../jadix_components/schema/${name}`, import.meta.url), 'utf8'))))).flat();
const type = schema => {
  switch (schema.type) {
    case 'null': return 'null';
    case 'bool': return 'boolean';
    case 'number': return 'number';
    case 'string': return 'string';
    case 'array': return `ReadonlyArray<${type(schema.items)}>`;
    case 'object': return `{ ${Object.entries(schema.fields).map(([name, field]) => `${JSON.stringify(name)}${field.required ? '' : '?'}: ${type(field.schema)}`).join('; ')} }`;
    default: throw new Error(`unsupported schema ${schema.type}`);
  }
};
const generated = new Map([
  ['../src/generated.mjs', `export const schemas = ${JSON.stringify(schemas, null, 2)};\n`],
  ['../src/generated.d.mts', `export interface ComponentProperties {\n${schemas.map(schema => `  ${schema.name}: ${type(schema.properties)};`).join('\n')}\n}\nexport interface ComponentEvents {\n${schemas.map(schema => `  ${schema.name}: { ${Object.entries(schema.events).map(([name, schema]) => `${JSON.stringify(name)}?: EventHandle<${type(schema)}>`).join('; ')} };`).join('\n')}\n}\nimport type { EventHandle } from './index.mjs';\n`],
]);
for (const [path, content] of generated) {
  const destination = new URL(path, import.meta.url);
  if (process.argv.includes('--check')) {
    if (await readFile(destination, 'utf8') !== content) throw new Error(`stale schema output: ${fileURLToPath(destination)}`);
  } else await writeFile(destination, content);
}
