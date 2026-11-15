'use strict';

const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {MiniProgramRuntime, environmentSnapshot, encode, decode, increment, measureText} = require('./runtime');

const wasm = process.env.JADIX_MINIPROGRAM_WASM || path.resolve(__dirname, '../../../../target/wasm32-unknown-unknown/miniprogram/jadix_miniprogram.mvp.wasm');

function page() {
  return {data: {roots: []}, patches: [], setData(patch) {
    this.patches.push(patch);
    for (const [key, value] of Object.entries(patch)) {
      const parts = key.replace(/\[(\d+)\]/g, '.$1').split('.');
      let target = this.data;
      for (const part of parts.slice(0, -1)) target = target[part];
      target[parts[parts.length - 1]] = JSON.parse(JSON.stringify(value));
    }
  }};
}

async function mounted() {
  const view = page();
  const runtime = new MiniProgramRuntime(view, {measureText: text => ({width: Array.from(text).length * 8})});
  await runtime.load(WebAssembly, fs.readFileSync(wasm));
  runtime.mount(360, 640);
  return {runtime, view};
}

test('native confirmation commits its final Unicode value before submitting through real Rust WASM', async () => {
  let component;
  require('node:vm').runInNewContext(fs.readFileSync(path.join(__dirname, 'components/jadix-node/index.js'), 'utf8'),
    {Component: definition => { component = definition; }, require});
  const {runtime, view} = await mounted();
  for (const index of [2, 6]) {
    const node = view.data.roots[0].children[index];
    const host = {data: {node}, triggerEvent: (_name, detail) => runtime.send(detail)};
    Object.assign(host, component.methods);
    const value = '确认中😀e\u0301 ' + index;
    host.onConfirm({detail: {value, cursor: value.length}});
    assert.equal(view.data.roots[0].children[index].text, value);
    assert.equal(view.data.roots[0].children[index].editRevision, '2');
    assert.equal(view.data.roots[0].children[0].text, 'Submitted: ' + value);
    runtime.send({root: node.root, node: node.id,
      payload: {kind: 'edit', value, selection: [0, 0], composing: [0, 1]}});
    assert.throws(() => runtime.send({root: node.root, node: node.id, payload: {kind: 'submit'}}), /InvalidEvent/);
    runtime.send({root: node.root, node: node.id,
      payload: {kind: 'edit', value, selection: [0, 0], composing: null}});
    runtime.send({root: node.root, node: node.id, payload: {kind: 'submit'}});
    assert.equal(view.data.roots[0].children[0].text, 'Submitted: ' + value);
  }
  runtime.dispose();
});

test('Canvas measurement wraps multiline controls and preserves graphemes and explicit empty lines', () => {
  const context = {measureText: text => ({width: Array.from(text).length * 8})};
  const node = {kind: 'textarea', frame: [0, 0, 0, 0], text: '中😀abcd',
    style: {font_size: 16, font_weight: 400, font_family: 'system-ui', foreground: [0, 0, 0, 255], background: [255, 255, 255, 255]}};
  const request = {node, widthMode: 'definite', maxWidth: 24, maxHeight: 1000};
  assert.deepEqual(measureText(context, request), [24, 40]);
  assert.deepEqual(measureText(context, {...request, widthMode: 'max'}), [24, 20]);
  assert.deepEqual(measureText(context, {...request, widthMode: 'min'}), [8, 120]);
  assert.deepEqual(measureText(context, {...request, maxHeight: 30}), [24, 30]);
  assert.deepEqual(measureText(context, {...request, node: {...node, text: '中\n\n'}}), [8, 60]);
  assert.deepEqual(measureText(context, {...request, node: {...node, text: 'e\u0301e\u0301', style: {...node.style, font_size: 20}}, maxWidth: 16}), [16, 50]);
  assert.deepEqual(measureText(context, {...request, node: {...node, kind: 'input'}}), [24, 20]);
  assert.deepEqual(measureText(context, {...request, node: {...node, kind: 'text'}}), [24, 40]);
  for (const width of [NaN, Infinity, -1]) {
    assert.throws(() => measureText({measureText: text => ({width: text === node.text ? 48 : width})}, request), /invalid Canvas measurement/);
  }
});

test('Rust grapheme boundaries preserve wrapping without Intl.Segmenter through real WASM', async () => {
  const sandbox = {module: {exports: {}}, Intl: {}};
  require('node:vm').runInNewContext(fs.readFileSync(path.join(__dirname, 'runtime.js'), 'utf8'), sandbox);
  const view = page(), measured = [];
  const runtime = new sandbox.module.exports.MiniProgramRuntime(view, {measureText: text => {
    measured.push(text);
    return {width: Array.from(text).length * 8};
  }});
  await runtime.load(WebAssembly, fs.readFileSync(wasm));
  runtime.mount(360, 640);
  const node = view.data.roots[0].children[6];
  const clusters = ['e\u0301', '👩‍💻', '🇨🇳'];
  const value = clusters.join('');
  measured.length = 0;
  runtime.send({root: node.root, node: node.id, payload: {kind: 'edit', value,
    selection: [value.length, value.length], composing: null}});
  runtime.resize(24, 640);
  assert.ok(measured.includes(value));
  for (const fragment of measured.filter(text => text && value.includes(text))) {
    const start = value.indexOf(fragment), end = start + fragment.length;
    const boundaries = [0, clusters[0].length, clusters[0].length + clusters[1].length, value.length];
    assert.ok(boundaries.includes(start) && boundaries.includes(end), 'split grapheme: ' + fragment);
  }
  assert.equal(view.data.roots[0].children[6].text, value);
  runtime.dispose();
});

test('actual Rust WASM keeps node identity, Unicode revisions, theme, focus and scroll across resize', async () => {
  const {runtime, view} = await mounted();
  assert.ok(runtime.nodes.size >= 57);
  assert.equal(new Set(Array.from(runtime.nodes.values(), node => node.descriptor.kind)).size, 12);
  const children = view.data.roots[0].children;
  const button = children[1], input = children[2], checkbox = children[3], select = children[4], scroll = children[5];
  assert.equal(runtime.send({root: button.root, node: button.id, payload: {kind: 'activate'}}), true);
  assert.equal(view.data.roots[0].children[1].text, 'Count: 1');
  assert.deepEqual(Object.keys(view.patches.at(-1)), ['roots[0].children[1]']);
  const value = '中😀👩‍💻e\u0301';
  runtime.send({root: input.root, node: input.id, payload: {kind: 'edit', value, selection: [value.length, value.length], composing: null}});
  assert.equal(view.data.roots[0].children[2].text, value);
  assert.equal(view.data.roots[0].children[2].editRevision, '1');
  runtime.send({root: input.root, node: input.id, payload: {kind: 'focus'}});
  runtime.send({root: checkbox.root, node: checkbox.id, payload: {kind: 'toggle', checked: true}});
  runtime.send({root: select.root, node: select.id, payload: {kind: 'select', value: 'b'}});
  runtime.send({root: scroll.root, node: scroll.id, payload: {kind: 'scroll', x: 0, y: 96}});
  const environment = environmentSnapshot({platform: 'ios', language: 'zh_CN', fontSizeSetting: 20}, 'dark');
  runtime.environment(environment);
  runtime.resize(480, 720);
  const next = view.data.roots[0].children;
  assert.equal(next[1].id, button.id);
  assert.match(next[1].css, /direction:ltr/);
  assert.equal(next[2].text, value);
  assert.equal(next[2].focused, true);
  assert.equal(next[3].checked, true);
  assert.equal(next[4].selectedLabel, '选项 B 😀');
  assert.equal(next[5].scrollY, 96);
  assert.equal(next[0].style.font_size, 20);
  assert.deepEqual(view.data.roots[0].style.background, [24, 24, 27, 255]);
  assert.equal(environment.platform.name, 'miniprogram');
  assert.equal(environment.platform.os, 'ios');
  runtime.apply({protocol: 2, root: runtime.root, previous: runtime.revision, revision: increment(runtime.revision), operations: [
    {op: 'update', id: next[1].id, descriptor: {...next[1], style: {...next[1].style, direction: 'right-to-left'}}},
  ]});
  assert.match(view.data.roots[0].children[1].css, /direction:rtl/);
  runtime.dispose();
  runtime.dispose();
  assert.equal(runtime.send({root: button.root, node: button.id, payload: {kind: 'activate'}}), false);
  assert.deepEqual(view.data.roots, []);
});

test('actual Rust WASM projects only enabled picker options and preserves a disabled current label', async () => {
  const {runtime, view} = await mounted();
  const select = view.data.roots[0].children[4];
  const original = runtime.nodes.get(select.id).descriptor;
  const options = [
    {label: 'Disabled A', value: 'a', enabled: false},
    {label: 'Enabled B', value: 'b', enabled: true},
    {label: 'Disabled C', value: 'c', enabled: false},
    {label: 'Enabled D', value: 'd', enabled: true},
  ];
  runtime.apply({protocol: 2, root: runtime.root, previous: runtime.revision, revision: increment(runtime.revision), operations: [
    {op: 'update', id: select.id, descriptor: {...original, value: 'd', options}},
  ]});
  const projected = view.data.roots[0].children[4];
  assert.deepEqual(projected.optionLabels, ['Enabled B', 'Enabled D']);
  assert.deepEqual(projected.optionValues, ['b', 'd']);
  assert.equal(projected.selectedIndex, 1);
  assert.equal(projected.selectedLabel, 'Enabled D');
  assert.equal(projected.selectEnabled, true);

  runtime.apply({protocol: 2, root: runtime.root, previous: runtime.revision, revision: increment(runtime.revision), operations: [
    {op: 'update', id: select.id, descriptor: {...original, value: 'a', options}},
  ]});
  const disabledSelection = view.data.roots[0].children[4];
  assert.equal(disabledSelection.selectedLabel, 'Disabled A');
  assert.equal(disabledSelection.selectedIndex, 0);
  assert.equal(disabledSelection.selectEnabled, true);

  runtime.apply({protocol: 2, root: runtime.root, previous: runtime.revision, revision: increment(runtime.revision), operations: [
    {op: 'update', id: select.id, descriptor: {...original, options: options.map(option => ({...option, enabled: false}))}},
  ]});
  assert.equal(view.data.roots[0].children[4].selectEnabled, false);
  runtime.dispose();
});

test('actual WASM rejects stale roots, split surrogates, disabled choices and invalid environment without changing accepted UI', async () => {
  const {runtime, view} = await mounted();
  const input = view.data.roots[0].children[2], select = view.data.roots[0].children[4];
  const before = JSON.stringify(view.data), revision = runtime.revision;
  assert.equal(runtime.send({root: '99999', node: input.id, payload: {kind: 'focus'}}), false);
  assert.throws(() => runtime.send({root: input.root, node: input.id, payload: {kind: 'edit', value: '😀', selection: [1, 1], composing: null}}), /InvalidEvent/);
  assert.throws(() => runtime.send({root: select.root, node: select.id, payload: {kind: 'select', value: 'missing'}}), /InvalidEvent/);
  const environment = environmentSnapshot({platform: 'android'}, 'light');
  environment.platform.name = '../ohos';
  assert.throws(() => runtime.environment(environment), /platform name/);
  assert.throws(() => runtime.resize(-1, 640), /viewport/);
  assert.equal(JSON.stringify(view.data), before);
  assert.equal(runtime.revision, revision);
  runtime.dispose();
});

test('protocol admission is atomic for stale revisions, wrong roots, cycles, missing parents and invalid descriptors', async () => {
  const {runtime, view} = await mounted();
  const parent = runtime.roots[0], child = view.data.roots[0].children[1].id;
  const before = JSON.stringify(view.data), revision = runtime.revision;
  const commit = operations => ({protocol: 2, root: runtime.root, previous: revision, revision: increment(revision), operations});
  for (const batch of [
    {...commit([]), root: '999'}, {...commit([]), previous: '0'},
    commit([{op: 'move', id: parent, parent, index: 0}]),
    commit([{op: 'move', id: child, parent: '99999', index: 0}]),
    commit([{op: 'update', id: child, descriptor: {kind: 'unknown'}}]),
    commit([{op: 'remove', id: '99999'}]),
  ]) {
    assert.throws(() => runtime.apply(batch));
    assert.equal(JSON.stringify(view.data), before);
    assert.equal(runtime.revision, revision);
  }
  runtime.dispose();
});

test('descriptions keep name and visual text separate, clear their spoken fallback and reject malformed metadata atomically', async () => {
  const {runtime, view} = await mounted();
  const [id, node] = Array.from(runtime.nodes).find(([, node]) => node.descriptor.kind === 'input');
  const original = node.descriptor;
  const publish = descriptor => runtime.apply({protocol: 2, root: runtime.root,
    previous: runtime.revision, revision: increment(runtime.revision),
    operations: [{op: 'update', id, descriptor}]});
  const projected = () => {
    let result = view.data;
    for (const part of runtime.paths.get(id).replace(/\[(\d+)\]/g, '.$1').split('.')) result = result[part];
    return result;
  };
  for (const description of ['说明 😀', 'Changed', null]) {
    publish({...original, name: 'Name', description, text: 'private value', password: true});
    assert.equal(projected().description, description);
    assert.equal(projected().name, 'Name');
    assert.equal(projected().text, 'private value');
    assert.equal(projected().accessibilityLabel, description ? 'Name, ' + description : 'Name');
  }
  const before = JSON.stringify(view.data), revision = runtime.revision;
  assert.throws(() => publish({...original, description: {invalid: true}}), /accessibility text/);
  assert.equal(JSON.stringify(view.data), before);
  assert.equal(runtime.revision, revision);
  runtime.dispose();
});

test('accessibility ID references resolve against the committed tree and clear when targets disappear', async () => {
  const {runtime, view} = await mounted();
  const owner = runtime.roots[0], ownerNode = runtime.nodes.get(owner);
  const target = '9001', root = runtime.root;
  const ownerDescriptor = {...ownerNode.descriptor, accessibilityRelations: {
    activeDescendant: {root, id: target}, controls: [{root, id: target}, {root, id: '9002'}, {root: '42', id: '9003'}],
  }};
  const commit = operations => runtime.apply({protocol: 2, root, previous: runtime.revision,
    revision: increment(runtime.revision), operations});
  commit([
    {op: 'update', id: owner, descriptor: ownerDescriptor},
    {op: 'create', id: target, parent: owner, index: 0, descriptor: {...ownerNode.descriptor,
      accessibilityRelations: {activeDescendant: null, controls: []}}},
  ]);
  assert.equal(view.data.roots[0].accessibilityId, 'jadix-' + root + '-' + owner);
  assert.equal(view.data.roots[0].accessibilityActiveDescendant, 'jadix-' + root + '-' + target);
  assert.deepEqual(view.data.roots[0].accessibilityControls, ['jadix-' + root + '-' + target]);
  assert.equal(view.data.roots[0].accessibilityControlsAttribute, 'jadix-' + root + '-' + target);

  commit([{op: 'remove', id: target}]);
  assert.equal(view.data.roots[0].accessibilityActiveDescendant, null);
  assert.deepEqual(view.data.roots[0].accessibilityControls, []);
  assert.equal(view.data.roots[0].accessibilityControlsAttribute, '');
  assert.throws(() => commit([{op: 'update', id: owner, descriptor: {...ownerDescriptor,
    accessibilityRelations: {activeDescendant: null, controls: [{root, id: '0'}]}}}]), /invalid accessibility relations/);
  runtime.dispose();
});

test('WXML binds stable element IDs and ARIA relations to each presented node', () => {
  const fs = require('node:fs');
  const path = require('node:path');
  const template = fs.readFileSync(path.join(__dirname, 'components/jadix-node/index.wxml'), 'utf8');
  assert.match(template, /id="\{\{node\.accessibilityId\}\}"/);
  assert.match(template, /aria-activedescendant="\{\{node\.accessibilityActiveDescendant\}\}"/);
  assert.match(template, /aria-controls="\{\{node\.accessibilityControlsAttribute\}\}"/);
});

test('decimal u64 revisions and UTF-8 transport retain values beyond JavaScript safe integers', () => {
  assert.equal(increment('9007199254740992'), '9007199254740993');
  assert.equal(increment('999'), '1000');
  for (const revision of ['01', '-1', '18446744073709551615', '18446744073709551616']) assert.throws(() => increment(revision));
  const value = '中😀👩‍💻\u0000e\u0301';
  assert.equal(decode(encode(value)), value);
  assert.equal(environmentSnapshot({platform: 'devtools'}, 'unknown').platform.os, null);
});

test('actual shared WASM modal blocking, anchored popovers and programmatic scroll commands retain their lifecycle', async () => {
  const {runtime, view} = await mounted();
  const children = view.data.roots[0].children;
  const send = (node, payload) => runtime.send({root: node.root, node: node.id, payload});
  const find = label => Array.from(runtime.nodes, ([id, node]) => ({id, root: runtime.root, ...node.descriptor})).find(node => node.text === label);
  send(find('Open dialog'), {kind: 'activate'});
  const dialog = view.data.roots[0].children.find(node => node.kind === 'dialog');
  assert.equal(dialog.open, true);
  assert.throws(() => send(children[1], {kind: 'activate'}), /InvalidEvent/);
  send(dialog, {kind: 'dismiss', backdrop: true});
  assert.equal(view.data.roots[0].children.find(node => node.kind === 'dialog').open, false);
  send(find('Open popover'), {kind: 'activate'});
  const popover = view.data.roots[0].children.find(node => node.kind === 'popover');
  assert.equal(popover.open, true);
  assert.ok(popover.frame.every(Number.isFinite));
  send(find('Close popover'), {kind: 'activate'});
  assert.equal(view.data.roots[0].children.find(node => node.kind === 'popover').open, false);
  assert.throws(() => send(find('Close popover'), {kind: 'activate'}), /InvalidEvent/);
  const scroll = view.data.roots[0].children[5];
  runtime.apply({command: 'scroll', root: runtime.root, revision: runtime.revision, node: scroll.id, offset: [0, 140]});
  assert.equal(view.data.roots[0].children[5].scrollY, 140);
  assert.throws(() => runtime.apply({command: 'scroll', root: '999', revision: runtime.revision, node: scroll.id, offset: [0, 200]}));
  runtime.dispose();
});


test('native blur clears focus without stealing a newer owner; UTF-16 cursor survives controlled echo and resize', async () => {
  const {runtime, view} = await mounted();
  const input = view.data.roots[0].children[2], textarea = view.data.roots[0].children[6];
  const send = (node, payload) => runtime.send({root: node.root, node: node.id, payload});
  send(input, {kind: 'focus'});
  assert.equal(view.data.roots[0].children[2].focused, true);
  send(textarea, {kind: 'focus'});
  send(input, {kind: 'blur'});
  assert.equal(view.data.roots[0].children[2].focused, false);
  assert.equal(view.data.roots[0].children[6].focused, true);
  const value = '中😀abc';
  send(textarea, {kind: 'edit', value, selection: [3, 3], composing: null});
  assert.equal(view.data.roots[0].children[6].selectionStart, 3);
  assert.throws(() => send(textarea, {kind: 'edit', value, selection: [2, 2], composing: null}), /InvalidEvent/);
  runtime.resize(420, 720);
  assert.equal(view.data.roots[0].children[6].selectionEnd, 3);
  send(textarea, {kind: 'blur'});
  assert.equal(runtime.focus, null);
  assert.equal(view.data.roots[0].children[6].focused, false);
  runtime.resize(430, 740);
  assert.equal(view.data.roots[0].children[6].focused, false);
  runtime.dispose();
  assert.equal(runtime.selections.size, 0);
});

test('Mini Program projects focus-within to ancestors across focus ownership changes', async () => {
  const {runtime, view} = await mounted();
  const input = view.data.roots[0].children[2], textarea = view.data.roots[0].children[6];
  const group = runtime.nodes.get(input.id).parent;
  const send = (node, kind) => runtime.send({root: node.root, node: node.id, payload: {kind}});
  assert.equal(runtime.nodes.get(group).descriptor.kind, 'box');

  send(input, 'focus');
  assert.equal(view.data.roots[0].focusWithin, true);
  assert.equal(view.data.roots[0].children[2].focusWithin, true);

  send(textarea, 'focus');
  send(input, 'blur');
  assert.equal(view.data.roots[0].focusWithin, true);
  assert.equal(view.data.roots[0].children[2].focusWithin, false);
  assert.equal(view.data.roots[0].children[6].focusWithin, true);

  send(textarea, 'blur');
  assert.equal(view.data.roots[0].focusWithin, false);
  assert.equal(view.data.roots[0].children[6].focusWithin, false);
  assert.equal(view.patches.at(-1)['roots[0].focusWithin'], false);

  send(input, 'focus');
  const revision = runtime.revision;
  runtime.apply({protocol: 2, root: runtime.root, previous: revision, revision: increment(revision),
    operations: [{op: 'remove', id: runtime.roots[0]}]});
  assert.equal(runtime.focus, null);
  assert.equal(runtime.focusWithin.size, 0);
  assert.deepEqual(view.data.roots, []);
  runtime.dispose();
});

test('explicit keyboard focus visibility renders a ring and clears on neutral focus or blur', async () => {
  const {runtime, view} = await mounted();
  const input = view.data.roots[0].children[2];
  const textarea = view.data.roots[0].children[6];
  const send = (node, kind) => runtime.send({root: node.root, node: node.id, payload: {kind}});
  send(input, 'focusVisible');
  assert.equal(view.data.roots[0].children[2].focusVisible, true);
  assert.equal(view.data.roots[0].children[2].focused, true);
  send(textarea, 'focus');
  assert.equal(view.data.roots[0].children[2].focusVisible, false);
  assert.equal(view.data.roots[0].children[6].focusVisible, false);
  assert.equal(view.data.roots[0].children[6].focused, true);
  send(textarea, 'focusVisible');
  assert.equal(view.data.roots[0].children[6].focusVisible, true);
  send(textarea, 'blur');
  assert.equal(view.data.roots[0].children[6].focusVisible, false);
  assert.equal(runtime.focusVisible, null);
  runtime.dispose();
});

test('focus emitted synchronously by setData keeps the most recent native owner', async () => {
  const {runtime, view} = await mounted();
  const input = view.data.roots[0].children[2], textarea = view.data.roots[0].children[6];
  const setData = view.setData.bind(view);
  let queued = false;
  view.setData = patch => {
    setData(patch);
    if (!queued) {
      queued = true;
      runtime.send({root: textarea.root, node: textarea.id, payload: {kind: 'focus'}});
    }
  };
  runtime.send({root: input.root, node: input.id, payload: {kind: 'focus'}});
  assert.equal(runtime.focus, textarea.id);
  runtime.resize(440, 740);
  assert.equal(view.data.roots[0].children[2].focused, false);
  assert.equal(view.data.roots[0].children[6].focused, true);
  runtime.dispose();
});
