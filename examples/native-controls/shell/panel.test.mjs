import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { createContext, runInContext } from 'node:vm';

const bundle = await readFile(new URL('panel.bundle.js', import.meta.url), 'utf8');

function panel() {
  const context = createContext({});
  runInContext(bundle, context, { timeout: 1000 });
  return source => JSON.parse(runInContext(`JSON.stringify(${source})`, context, { timeout: 1000 }));
}

test('schema migration preserves Unicode and count without changing live state', () => {
  const evaluate = panel();
  evaluate('NativePanel.initialize({draft:"active",count:3},1)');
  const migrated = evaluate('NativePanel.migrateStorage(1,2,{draft:"中文😀",count:17})');
  assert.deepEqual(migrated, { profile: { name: '中文😀' }, counter: 17 });
  assert.deepEqual(evaluate('NativePanel.snapshot()'), { draft: 'active', count: 3 });
  assert.deepEqual(evaluate(`NativePanel.initialize(${JSON.stringify(migrated)},2)`), { draft: '中文😀', count: 17 });
  assert.deepEqual(evaluate('NativePanel.checkpoint(2)'), migrated);
});

test('invalid migrations and wrong-version checkpoints fail without mutating business state', () => {
  const evaluate = panel();
  evaluate('NativePanel.initialize({draft:"stable",count:1},1)');
  for (const source of [
    'NativePanel.migrateStorage(2,1,{profile:{name:"old"},counter:1})',
    'NativePanel.migrateStorage(1,2,{draft:"old",count:-1})',
    'NativePanel.migrateStorage(1,2,{draft:"old",count:1,extra:true})',
    'NativePanel.initialize({draft:"old",count:1},2)',
    'NativePanel.initialize({profile:{name:"old",extra:true},counter:1},2)',
    'NativePanel.initialize({profile:{name:"old"},counter:1.5},2)',
    'NativePanel.initialize({profile:{name:"x".repeat(16385)},counter:0},2)',
    'NativePanel.initialize(null,3)',
    'NativePanel.checkpoint(3)',
  ]) {
    assert.throws(() => evaluate(source));
    assert.deepEqual(evaluate('NativePanel.snapshot()'), { draft: 'stable', count: 1 });
  }
});
