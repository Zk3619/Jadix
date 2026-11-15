import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Box, Text, Button, Input, Textarea, Checkbox, Switch, Select, DialogContent, PopoverContent, createClient } from '../src/index.mjs';
import { jsx, jsxs, Fragment } from '../src/jsx-runtime.mjs';

test('JSX and direct SDK construct the same shared component tree', () => {
  assert.deepEqual(jsxs(Box, { children: [jsx(Text, { text: 'hello' }), jsx(Input, { value: '中文😀' })] }), Box({ children: [Text({ text: 'hello' }), Input({ value: '中文😀' })] }));
  assert.equal(jsx(Fragment, { children: Text({ text: 'fragment' }) }).children.length, 1);
});

test('schema rejects unknown properties, invalid event handles and unsupported children', () => {
  assert.throws(() => Button({ label: 'ok', mystery: true }), /unknown property/);
  assert.throws(() => Input({ value: 42 }), /string/);
  assert.throws(() => Button({ label: 'ok', events: { activate: () => {} } }), /invalid event/);
  assert.throws(() => jsx(Text, { text: 'hello', children: Text({ text: 'unexpected' }) }), /unknown property/);
  assert.equal(jsx(Box, {}, 'key').key, 'key');
  assert.throws(() => jsx(Box, {}, ''), /key/);
  assert.throws(() => jsx(Box, {}, 1), /key/);
});

test('Popover JSX and direct SDK share keyed anchor and versioned dismissal contracts', () => {
  const dismiss = { session: '1', sequence: '2' };
  const props = { anchor: 'trigger', open: true, placement: 'bottom-start', gap: 8, events: { dismiss } };
  assert.deepEqual(jsx(PopoverContent, { ...props, children: Text({ text: '中文😀' }) }), PopoverContent({ ...props, children: [Text({ text: '中文😀' })] }));
  assert.throws(() => PopoverContent({ open: true }), /missing property anchor/);
  assert.throws(() => PopoverContent({ anchor: 'a'.repeat(257), open: true }), /string limit/);
  assert.throws(() => PopoverContent({ anchor: 'trigger', open: true, events: { activate: dismiss } }), /invalid event/);
});

test('host transport has decimal identities, explicit cancellation and cross-session checks', async () => {
  const sent = [];
  const client = createClient(envelope => { sent.push(envelope); return 'saved'; }, '9007199254740993');
  const call = client.call('counter.save', { count: 1 });
  assert.equal(await call.result, 'saved');
  await call.cancel();
  assert.deepEqual(sent.map(envelope => envelope.command.type), ['call', 'cancel']);
  assert.equal(sent[0].session, '9007199254740993');
  assert.equal(call.request, '1');
  assert.throws(() => client.event({ session: '1', sequence: '1' }, null), /invalid event/);
  assert.throws(() => createClient(() => {}, '01'), /invalid host/);
});

test('extended SDK controls share generated schemas and keys need no browser APIs', () => {
  assert.equal(Textarea({ value: '多行\n😀' }).component, 'Textarea');
  assert.equal(Checkbox({ label: 'check', checked: true }).props.checked, true);
  assert.equal(Switch({ label: 'switch', checked: false }).component, 'Switch');
  assert.equal(Select({ value: 'one', options: [{ value: 'one', label: 'One', enabled: true }] }).component, 'Select');
  assert.equal(DialogContent({ open: false, children: [Text({ text: 'modal' })] }).children.length, 1);
  assert.throws(() => Checkbox({ label: 'missing checked' }), /missing/);
  assert.throws(() => jsx(Box, {}, '😀'.repeat(65)), /key/);
  assert.throws(() => jsx(Box, {}, '\ud800'), /key/);
});


test('Input and Textarea expose a bounded UTF-16 maxLength without truncating controlled values', () => {
  for (const component of [Input, Textarea]) {
    assert.equal(component({ value: 'external over-limit', maxLength: 0 }).props.maxLength, 0);
    assert.equal(component({ value: '🦀', maxLength: 1048576 }).props.value, '🦀');
    for (const limit of [-1, 0.5, 1048577, NaN, Infinity]) {
      assert.throws(() => component({ value: '', maxLength: limit }));
    }
  }
});

test('UTF-8 budgets accept exact scalar boundaries and reject unpaired surrogates', () => {
  for (const [scalar, width] of [['a', 1], ['é', 2], ['汉', 3], ['🦀', 4]]) {
    const text = scalar.repeat(Math.floor(65536 / width));
    assert.equal(Text({ text }).props.text, text);
    assert.throws(() => Text({ text: text + scalar }), /string limit/);
    const key = scalar.repeat(Math.floor(256 / width));
    assert.equal(jsx(Box, {}, key).key, key);
    assert.throws(() => jsx(Box, {}, key + scalar), /key/);
  }
  for (const text of ['\ud800', '\udfff', 'a\ud800z', '\ud800\ud800']) {
    assert.throws(() => Text({ text }), /surrogate/);
    assert.throws(() => jsx(Box, {}, text), /key/);
  }
});
