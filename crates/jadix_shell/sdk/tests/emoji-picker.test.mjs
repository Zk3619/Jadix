import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createApplication, createEmojiPicker, EmojiPicker, Box, Image, Text} from '../src/index.mjs';

const flatten = root => [root, ...root.children.flatMap(flatten)];
function fixture(run) {
  const calls = [], callbacks = new Map();
  let sequence = 0;
  const entries = Array.from({length: 260}, (_, index) => ({text: index === 0 ? '👍🏿' : `emoji${index}`, base: index === 0 ? '👍' : `emoji${index}`, name: `name${index}`, group: 1, section: 1}));
  const matches = query => query.query === 'none' ? [] : entries;
  const location = (index, query, headers) => ({index, row: index === null ? null : (query.query.trim() || headers?.length === 0 ? 0 : 1) + Math.floor(index / 8), entry: index === null ? null : entries[index]});
  globalThis.__jadixHost = input => {
    const request = JSON.parse(input); calls.push(request);
    let value = null;
    if (request.type === 'identity') value = {session: '1', root: '2'};
    if (request.type === 'register') { value = {session: '1', sequence: String(++sequence)}; callbacks.set(value.sequence, request.key); }
    if (request.type === 'release') callbacks.delete([...callbacks].find(([, key]) => key === request.key)?.[0]);
    if (request.type === 'emoji_search') { const items = matches(request.query); value = {total: items.length, items: items.slice(request.query.offset, request.query.offset + request.query.limit)}; }
    if (request.type === 'emoji_entry') value = entries.find(entry => entry.text === request.emoji) ?? null;
    if (request.type === 'emoji_layout') { const items = matches(request.query); value = {total: items.length, rows: items.length ? [...(request.query.query.trim() || !request.headers?.includes(1) ? [] : [{kind: 'group', value: 1}]), ...Array.from({length: Math.ceil(items.length / request.columns)}, (_, row) => ({kind: 'items', value: {start: row * request.columns, end: Math.min(items.length, (row + 1) * request.columns)}}))] : [], sections: Array(Math.ceil(items.length / request.columns) + (items.length && !request.query.query.trim() && request.headers?.includes(1) ? 1 : 0)).fill(1)}; }
    if (request.type === 'emoji_locate') { const index = matches(request.query).findIndex(entry => entry.text === request.emoji); value = location(index < 0 ? null : index, request.query, request.headers); }
    if (request.type === 'emoji_navigate') value = location(request.key === 'end' ? 259 : request.current === null ? 0 : request.current + 1, request.query, request.headers);
    if (request.type === 'virtual_range') {
      const total = request.count * request.extent;
      let offset = Math.max(0, Math.min(request.offset, total - request.viewport));
      if (request.focus !== undefined) offset = Math.max(0, Math.min(request.focus * request.extent, total - request.viewport));
      const start = Math.max(0, Math.floor(offset / request.extent) - request.overscan), end = Math.min(request.count, Math.ceil((offset + request.viewport) / request.extent) + request.overscan);
      value = {start, end, visible_start: Math.floor(offset / request.extent), visible_end: Math.min(request.count, Math.ceil((offset + request.viewport) / request.extent)), offset, before: start * request.extent, after: (request.count - end) * request.extent};
    }
    return JSON.stringify({ok: true, value});
  };
  try { run({app: createApplication(), calls, callbacks, emit: (reference, payload = null) => globalThis.__jadixDispatch(callbacks.get(reference.sequence), payload)}); }
  finally { delete globalThis.__jadixHost; delete globalThis.__jadixDispatch; delete globalThis.__jadixLocaleChanged; }
}

test('picker composes common controls, delegates geometry and bounds callback ownership', () => fixture(({app, calls, callbacks, emit}) => {
  const selected = [], picker = createEmojiPicker(app, {id: 'emoji', onSelect: value => selected.push(value), onChange() {}});
  assert.equal(picker.scrollToGroup(1), true);
  assert.equal(calls.findLast(request => request.type === 'virtual_range').align, 'start');
  const initial = EmojiPicker({picker});
  const search = flatten(initial).find(node => node.component === 'Input');
  assert.deepEqual(search.props.accessibleControls, [flatten(initial).find(node => node.component === 'Scroll').key]);
  assert.equal(search.props.accessibleActiveDescendant, undefined);
  assert.equal(flatten(initial).find(node => node.component === 'Scroll').props.accessibleRole, 'listbox');
  assert.equal(flatten(initial).find(node => node.props.label === '👍🏿').props.accessibleRole, 'option');
  assert.ok(flatten(initial).some(node => node.props.accessibleRole === 'heading' && node.props.accessibleName === 'People & body'));
  assert.ok(flatten(initial).every(node => node.component !== 'Hosted'));
  assert.ok(callbacks.size < 160);
  emit(flatten(initial).find(node => node.props.label === '👍🏿').events.activate);
  assert.deepEqual(selected, ['👍🏿']);
  const searchKey = flatten(initial).find(node => node.component === 'Input').key;
  assert.equal(picker.highlight('emoji250'), true);
  const pending = flatten(picker.render()), scroll = pending.find(node => node.component === 'Scroll');
  assert.equal(scroll.style.scroll_behavior, 'smooth');
  assert.ok(!pending.some(node => node.props.label === 'emoji250'));
  emit(scroll.events.scroll, {x: 0, y: scroll.style.scroll_to[1], viewport: [320, 260]});
  assert.ok(flatten(picker.render()).some(node => node.props.label === 'emoji250' && node.props.accessibleSelected));
  assert.equal(picker.highlight('missing'), false);
  for (let index = 0; index < 10; index++) { picker.update({query: index % 2 ? 'none' : 'cat', tone: 5}); picker.render(); assert.ok(callbacks.size < 160); }
  picker.update({query: ''});
  assert.equal(flatten(picker.render()).find(node => node.component === 'Input').key, searchKey);
  assert.ok(calls.some(request => request.type === 'emoji_search' && request.query.tone === 5));
  assert.ok(calls.some(request => request.type === 'emoji_locate'));
  picker.release(); picker.release(); assert.equal(callbacks.size, 0);
  assert.throws(() => picker.render(), /released/);
}));

test('IME guards navigation; Escape clears search; focus requests and hidden search use common controls', () => fixture(({app, calls, emit}) => {
  let cancelled = 0;
  const picker = createEmojiPicker(app, {id: 'ime', autoFocus: true, onChange() {}, onCancel() { cancelled++; }});
  let input = flatten(picker.render()).find(node => node.component === 'Input');
  assert.equal(input.props.focusRequest, 1); picker.focus();
  assert.equal(flatten(picker.render()).find(node => node.component === 'Input').props.focusRequest, 2);
  emit(input.events.edit, {value: 'draft', composing: true}); picker.navigate('down');
  assert.ok(!calls.some(request => request.type === 'emoji_navigate'));
  emit(input.events.edit, {value: 'cat', composing: false});
  const filtered = flatten(picker.render());
  assert.equal(filtered.find(node => node.component === 'Input').props.accessibleDescription, 'name0');
  assert.equal(filtered.find(node => node.component === 'Input').props.accessibleActiveDescendant, filtered.find(node => node.props.accessibleSelected).key);
  assert.ok(filtered.find(node => node.props.label === '👍🏿').props.accessibleSelected);
  picker.navigate('escape');
  assert.equal(picker.query.query, ''); assert.equal(cancelled, 0); picker.navigate('escape'); assert.equal(cancelled, 1);
  picker.release();
  const hidden = createEmojiPicker(app, {id: 'hidden', searchable: false, query: {query: 'cat'}, onChange() {}});
  const button = flatten(hidden.render()).find(node => node.key === 'hidden__keyboard');
  assert.equal(button.props.accessibleActiveDescendant, 'hidden__emoji_0');
  assert.deepEqual(button.props.accessibleControls, ['hidden__scroll']);
  hidden.update({query: 'none'});
  assert.equal(flatten(hidden.render()).find(node => node.key === 'hidden__keyboard').props.accessibleActiveDescendant, undefined);
  emit(button.events.navigate, {key: 'right', shift: false, control: false});
  assert.ok(calls.some(request => request.type === 'emoji_navigate' && request.key === 'right'));
  hidden.release();
  const disabled = createEmojiPicker(app, {id: 'disabled', disabled: true, autoFocus: true, onChange() {}});
  disabled.focus(); assert.equal(disabled.highlight('👍🏿'), false);
  assert.equal(flatten(disabled.render()).find(node => node.component === 'Input').props.focusRequest, 0);
  disabled.release();
}));


test('hover preserves scroll position and custom part hooks retain events', () => fixture(({app, calls, callbacks, emit}) => {
  const highlighted = [], customized = [];
  const parts = Object.fromEntries(['searchBar', 'search', 'list', 'row', 'item', 'group'].map(part =>
    [part, element => { customized.push(part); return element; }]));
  const picker = createEmojiPicker(app, {id: 'hover', parts, onChange() {}, onHighlightChange: entry => highlighted.push(entry?.text)});
  const first = picker.render();
  const button = flatten(first).find(node => node.props.label === '👍🏿');
  const calculations = calls.filter(request => request.type === 'virtual_range').length;
  emit(button.events.pointerMove, {x: 10, y: 10}); emit(button.events.pointerMove, {x: 10, y: 10});
  assert.deepEqual(highlighted, ['👍🏿']);
  assert.equal(calls.filter(request => request.type === 'virtual_range').length, calculations);
  assert.ok(flatten(picker.render()).find(node => node.props.label === '👍🏿').props.accessibleSelected);
  const other = flatten(picker.render()).find(node => node.props.accessibleRole === 'option' && node.props.label !== '👍🏿');
  emit(other.events.pointerMove, {x: 10, y: 10});
  assert.deepEqual(highlighted, ['👍🏿']);
  picker.navigate('right');
  const keyboard = highlighted.at(-1);
  emit(button.events.pointerMove, {x: 10, y: 10});
  emit(button.events.pointerMove, {x: NaN, y: 10});
  assert.equal(highlighted.at(-1), keyboard);
  emit(button.events.pointerMove, {x: 11, y: 10});
  assert.equal(highlighted.at(-1), '👍🏿');

  for (const part of ['searchBar', 'search', 'list', 'row', 'item', 'group']) assert.ok(customized.includes(part));
  picker.update({query: 'cat'});
  assert.ok(!flatten(picker.render()).some(node => node.component === 'Text' && node.props.text === 'People & body'));
  assert.equal(picker.scrollToGroup(1), false);
  picker.release(); assert.equal(callbacks.size, 0);
}));

test('headings are optional, filters opt in and search retains caret keys', () => fixture(({app, calls, emit}) => {
  const picker = createEmojiPicker(app, {id: 'minimal', groupLabels: [], searchable: false, onChange() {}});
  let tree = flatten(picker.render());
  assert.ok(tree.every(node => node.component !== 'Select' && node.props.accessibleRole !== 'heading'));
  assert.ok(tree.find(node => node.key === 'minimal__keyboard').props.navigationKeys.includes('home'));
  assert.equal(picker.scrollToGroup(1), true);
  const scroll = tree.find(node => node.component === 'Scroll');
  assert.equal(scroll.style.grow, 1);
  emit(scroll.events.scroll, {x: 0, y: 0, viewport: [320, 280]});
  picker.navigate('pageDown');
  assert.equal(calls.findLast(request => request.type === 'emoji_navigate').page_rows, 7);
  assert.deepEqual(calls.findLast(request => request.type === 'emoji_navigate').headers, []);
  picker.release();
  const groups = [], labeled = createEmojiPicker(app, {id: 'custom', groupLabels: [], showFilters: true,
    renderGroup: group => { groups.push(group); return {component: 'Text', props: {text: group.label}, children: []}; }, onChange() {}});
  tree = flatten(labeled.render());
  assert.equal(tree.filter(node => node.component === 'Select').length, 2);
  assert.deepEqual(groups[0], {id: 1, label: '', first: true});
  const input = tree.find(node => node.component === 'Input');
  assert.ok(!input.props.navigationKeys.includes('home') && !input.props.navigationKeys.includes('end'));
  const count = calls.filter(request => request.type === 'emoji_navigate').length;
  emit(input.events.navigate, {key: 'home'});
  assert.equal(calls.filter(request => request.type === 'emoji_navigate').length, count);
  labeled.release();
}));

test('custom item content retains the shared button, callbacks and identity', () => fixture(({app, emit, calls}) => {
  let rendered = 0;
  const selected = [], picker = createEmojiPicker(app, {id: 'images', query: {query: 'cat'}, onChange() {},
    onSelect: value => selected.push(value),
    renderItem: entry => { rendered++; return Box({children: [Image({src: `asset:/emoji/${entry.text}.png`, alt: entry.name}), Text({text: entry.name})]}); }});
  const first = flatten(picker.render()), input = first.find(node => node.component === 'Input');
  const button = first.find(node => node.component === 'Button' && node.props.label === '👍🏿');
  assert.equal(button.props.accessibleRole, 'option');
  assert.equal(button.props.accessibleName, 'name0');
  assert.equal(button.children[0].children[0].component, 'Image');
  assert.equal(button.children[0].children[0].props.src, 'asset:/emoji/👍🏿.png');
  const mounted = rendered;
  emit(button.events.pointerMove, {x: 10, y: 10});
  const second = flatten(picker.render());
  assert.equal(rendered, mounted);
  assert.equal(second.find(node => node.props.label === '👍🏿').children[0], button.children[0]);
  assert.equal(second.find(node => node.component === 'Input').key, input.key);
  assert.equal(second.find(node => node.props.label === '👍🏿').key, button.key);
  emit(button.events.activate); assert.deepEqual(selected, ['👍🏿']);
  picker.navigate('end');
  assert.equal(calls.findLast(request => request.type === 'emoji_navigate').key, 'end');
  picker.release();
}));

test('custom geometry reaches common virtualization and follows resized viewport navigation', () => fixture(({app, calls, emit}) => {
  const picker = createEmojiPicker(app, {id: 'large', rowHeight: 64, itemSize: 56, height: 128, onChange() {}});
  const initial = flatten(picker.render()), scroll = initial.find(node => node.component === 'Scroll');
  assert.equal(scroll.style.height, 128);
  const option = initial.find(node => node.props.accessibleRole === 'option');
  assert.equal(option.style.width, 0); assert.equal(option.style.grow, 1);
  assert.equal(option.style.height, 56);
  assert.equal(calls.findLast(request => request.type === 'virtual_range').extent, 64);
  picker.navigate('pageDown');
  assert.equal(calls.findLast(request => request.type === 'emoji_navigate').page_rows, 2);
  emit(scroll.events.scroll, {x: 0, y: 64, viewport: [480, 192]});
  picker.render(); picker.navigate('pageDown');
  assert.equal(calls.findLast(request => request.type === 'emoji_navigate').page_rows, 3);
  picker.release();
  for (const dimensions of [{rowHeight: NaN}, {height: 0}, {itemSize: 65, rowHeight: 64}])
    assert.throws(() => createEmojiPicker(app, {id: 'invalid', onChange() {}, ...dimensions}), /dimensions/);
}));

test('section jumps override one animation request and mount their destination immediately', () => fixture(({app, emit}) => {
  const picker = createEmojiPicker(app, {id: 'jump', onChange() {}});
  picker.render(); picker.highlight('emoji250');
  let scroll = flatten(picker.render()).find(node => node.component === 'Scroll');
  emit(scroll.events.scroll, {x: 0, y: scroll.style.scroll_to[1], viewport: [320, 260]});
  assert.ok(flatten(picker.render()).some(node => node.props.label === 'emoji250'));
  assert.equal(picker.scrollToGroup(1, false), true);
  const jumped = flatten(picker.render());
  scroll = jumped.find(node => node.component === 'Scroll');
  assert.equal(scroll.style.scroll_behavior, 'instant');
  assert.deepEqual(scroll.style.scroll_to, [0, 0]);
  assert.ok(jumped.some(node => node.props.label === '👍🏿'));
  picker.highlight('emoji250');
  scroll = flatten(picker.render()).find(node => node.component === 'Scroll');
  assert.equal(scroll.style.scroll_behavior, 'smooth');
  assert.ok(!flatten(picker.render()).some(node => node.props.label === 'emoji250'));
  picker.release();
}));

test('metadata updates retain highlighted entry and mounted destination', () => fixture(({app, emit}) => {
  const changes = [], picker = createEmojiPicker(app, {id: 'metadata', onChange() {}, onHighlightChange(entry) { changes.push(entry?.text); }});
  picker.highlight('emoji200');
  let scroll = flatten(picker.render()).find(node => node.component === 'Scroll');
  emit(scroll.events.scroll, {x: 0, y: scroll.style.scroll_to[1], viewport: [320, 260]});
  picker.update({recent: ['emoji200']});
  const updated = flatten(picker.render());
  assert.ok(updated.some(node => node.props.label === 'emoji200' && node.props.accessibleSelected));
  assert.equal(updated.find(node => node.component === 'Input').props.accessibleDescription, 'name200');
  assert.deepEqual(changes, ['emoji200']);
  picker.update({query: 'cat'});
  assert.equal(flatten(picker.render()).find(node => node.component === 'Input').props.accessibleDescription, 'name0');
  assert.deepEqual(changes, ['emoji200', '👍🏿']);
  picker.release();
}));


test('controlled query proposals wait for parent update and preserve input identity', () => fixture(({app, emit}) => {
  const proposals = [];
  const picker = createEmojiPicker(app, {id: 'controlled', query: {query: 'cat'}, controlledQuery: true,
    onQueryChange(value) { proposals.push(value); }, onChange() {}});
  const input = flatten(picker.render()).find(node => node.component === 'Input');
  emit(input.events.edit, {value: 'none', composing: false});
  assert.equal(picker.query.query, 'cat');
  assert.equal(picker.matchedCount(), 260);
  assert.deepEqual(proposals, ['none']);
  picker.update({query: 'none'});
  assert.equal(picker.matchedCount(), 0);
  assert.deepEqual(proposals, ['none']);
  const updated = flatten(picker.render()).find(node => node.component === 'Input');
  assert.equal(updated.key, input.key);
  assert.equal(updated.props.value, 'none');
  picker.navigate('escape');
  assert.equal(picker.query.query, 'none');
  assert.deepEqual(proposals, ['none', '']);
  picker.release();
}));

test('controlled query proposals allow immediate parent transformation', () => fixture(({app, emit}) => {
  const picker = createEmojiPicker(app, {id: 'accepted', controlledQuery: true,
    onQueryChange(value) { picker.update({query: value.toLowerCase()}); }, onChange() {}});
  const input = flatten(picker.render()).find(node => node.component === 'Input');
  emit(input.events.edit, {value: 'CAT', composing: false});
  assert.equal(picker.query.query, 'cat');
  assert.equal(flatten(picker.render()).find(node => node.component === 'Input').props.value, 'cat');
  picker.release();
}));
