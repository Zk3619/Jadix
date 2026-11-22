import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Box, Text, Image, Button, Input, Textarea, Checkbox, Switch, Select, DialogContent, PopoverContent, MenuButton, TabBar, createElement, createClient, createApplication } from '../src/index.mjs';
import { jsx, jsxs, Fragment } from '../src/jsx-runtime.mjs';

test('updater uses the asynchronous desktop bridge and preserves channel selection', async () => {
  const requests = [];
  globalThis.__jadixHost = input => {
    const request = JSON.parse(input);
    if (request.type === 'identity') return JSON.stringify({ok: true, value: {session: '1', root: '2'}});
    requests.push(request);
    return JSON.stringify({ok: true, value: null});
  };
  try {
    const app = createApplication();
    await assert.rejects(app.updater(null), /invalid updater/);
    const pending = app.updater({action: 'check', channel: 'preview', locale: 'zh-CN'});
    const request = requests.pop();
    assert.equal(request.type, 'updater');
    assert.deepEqual(request.request, {action: 'check', channel: 'preview', locale: 'zh-CN'});
    const snapshot = {phase: 'available', generation: 1, received: 0, error: null, available: {version: '2.0.0', canInstall: true}};
    globalThis.__jadixDispatch(request.key, {value: snapshot});
    assert.deepEqual(await pending, snapshot);
    const failed = app.updater({action: 'install', restart: true});
    const rejection = assert.rejects(failed, /not staged/);
    globalThis.__jadixDispatch(requests.pop().key, {error: 'not staged'});
    await rejection;
  } finally { delete globalThis.__jadixHost; delete globalThis.__jadixDispatch; }
});

test('locale calls preserve structured arguments and propagate host validation', () => {
  const requests = [];
  globalThis.__jadixHost = input => {
    const request = JSON.parse(input);
    if (request.type === 'identity') return JSON.stringify({ok: true, value: {session: '1', root: '2'}});
    requests.push(request);
    if (request.request.action === 'number') return JSON.stringify({ok: false, error: 'invalid locale format arguments'});
    return JSON.stringify({ok: true, value: request.request.action === 'snapshot' ? {availableLocales: ['en', 'zh']} : '译文'});
  };
  try {
    const app = createApplication();
    assert.deepEqual(app.availableLocales(), ['en', 'zh']);
    assert.equal(app.t('greeting', {name: '中文🦀', count: 3}), '译文');
    assert.deepEqual(requests.at(-1), {type: 'i18n', request: {action: 'translate', key: 'greeting', args: {name: '中文🦀', count: '3'}}});
    app.setLocale('ja', 'en', 'Asia/Tokyo');
    assert.deepEqual(requests.at(-1).request, {action: 'configure', locale: 'ja', fallback: 'en', timezone: 'Asia/Tokyo'});
    app.formatTime(13, 15);
    assert.deepEqual(requests.at(-1).request, {action: 'time', hour: 13, minute: 15, hour12: false});
    assert.throws(() => app.formatNumber(NaN), /invalid locale/);
    assert.throws(() => app.t('greeting', []), /invalid translation/);
    const changes = [];
    const stop = app.onLocaleChange(snapshot => changes.push(snapshot.locale));
    globalThis.__jadixLocaleChanged({locale: 'ja'});
    stop();
    globalThis.__jadixLocaleChanged({locale: 'en'});
    assert.deepEqual(changes, ['ja']);
    assert.throws(() => app.onLocaleChange(null), /callback/);
  } finally { delete globalThis.__jadixHost; delete globalThis.__jadixDispatch; }
});

test('JSX and direct SDK construct the same shared component tree', () => {
  assert.deepEqual(jsxs(Box, { children: [jsx(Text, { text: 'hello' }), jsx(Input, { value: '中文😀' })] }), Box({ children: [Text({ text: 'hello' }), Input({ value: '中文😀' })] }));
  assert.equal(jsx(Fragment, { children: Text({ text: 'fragment' }) }).children.length, 1);
  const row = jsx(Text, {text: 'mapped child'}, 'stable');
  assert.deepEqual(jsxs(Box, {children: [[], [row], [[row]]]}).children, [row, row]);
  assert.throws(() => jsx(Box, {children: Array(4097).fill(row)}), /children limit/);
  let nested = row;
  for (let index = 0; index < 33; index++) nested = [nested];
  assert.throws(() => jsx(Box, {children: nested}), /children limit/);
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

test('desktop promises relay errors, release callbacks and enforce the outstanding limit', async () => {
  const requests = [];
  globalThis.__jadixHost = input => {
    const request = JSON.parse(input);
    if (request.type === 'identity') return JSON.stringify({ok: true, value: {session: '1', root: '2'}});
    if (request.url === 'denied:') return JSON.stringify({ok: false, error: 'PermissionDenied'});
    requests.push(request);
    return JSON.stringify({ok: true, value: null});
  };
  try {
    const app = createApplication();
    await assert.rejects(app.openUrl('denied:'), /PermissionDenied/);
    await assert.rejects(app.share({files: [42]}), /invalid share/);
    const pending = Array.from({length: 64}, () => app.openUrl('https://example.com'));
    await assert.rejects(app.openUrl('https://example.com/overflow'), /limit/);
    for (const request of requests.splice(0)) {
      globalThis.__jadixDispatch(request.key, {value: true});
      globalThis.__jadixDispatch(request.key, {error: 'duplicate must be ignored'});
    }
    assert.deepEqual(await Promise.all(pending), Array(64).fill(true));
    const shared = app.share({text: '中文🦀', files: ['1:file:1']});
    globalThis.__jadixDispatch(requests.pop().key, {value: {status: 'dismissed'}});
    assert.deepEqual(await shared, {status: 'dismissed'});
    const failed = app.openUrl('https://example.com');
    const rejection = assert.rejects(failed, /system refused/);
    globalThis.__jadixDispatch(requests.pop().key, {error: 'system refused'});
    await rejection;
  } finally {
    delete globalThis.__jadixHost;
    delete globalThis.__jadixDispatch;
  }
});

test('hosted menu children and tab metadata preserve the shared contract', () => {
  const child = Text({text: 'Open'});
  const menu = MenuButton({label: 'Menu', items: [{id: 'group', label: 'Group', inline: true, children: [{id: 'open', label: 'Open'}]}], children: [child]});
  assert.deepEqual(menu.children, [child]);
  const tab = TabBar({items: [{id: 'home', label: 'Home', badge: '2', symbol: 'house', accessibility_label: 'Home screen'}], selected: 'home', hidden: true});
  const leaf = JSON.parse(tab.props.specification);
  assert.equal(leaf.hidden, true);
  assert.equal(leaf.items[0].badge, '2');
  assert.throws(() => createElement('Hosted', tab.props, [child]), /children/);
});

test('haptics and scroll stop use cancellable application requests', async () => {
  const requests = [];
  globalThis.__jadixHost = input => {
    const request = JSON.parse(input);
    if (request.type === 'identity') return JSON.stringify({ok: true, value: {session: '1', root: '2'}});
    requests.push(request); return JSON.stringify({ok: true, value: null});
  };
  try {
    const app = createApplication();
    await assert.rejects(app.stopScroll([-1]), /invalid scroll/);
    assert.equal(requests.length, 0);
    const feedback = app.haptic({type: 'impact', style: 'soft'});
    const haptic = requests.pop();
    assert.equal(haptic.type, 'haptic');
    assert.deepEqual(haptic.feedback, {type: 'impact', style: 'soft'});
    globalThis.__jadixDispatch(haptic.key, {value: {available: true}});
    assert.deepEqual(await feedback, {available: true});
    const stop = app.stopScroll([0, 2]);
    const request = requests.pop();
    assert.equal(request.type, 'stop_scroll'); assert.deepEqual(request.path, [0, 2]);
    globalThis.__jadixDispatch(request.key, {value: null}); await stop;
  } finally { delete globalThis.__jadixHost; delete globalThis.__jadixDispatch; }
});


test('authentication sessions preserve ephemeral requests and cancellation replies', async () => {
  const requests = [];
  globalThis.__jadixHost = input => {
    const request = JSON.parse(input);
    if (request.type === 'identity') return JSON.stringify({ok: true, value: {session: '1', root: '2'}});
    requests.push(request); return JSON.stringify({ok: true, value: null});
  };
  try {
    const app = createApplication();
    await assert.rejects(app.openAuthSession(null), /invalid authentication/);
    assert.equal(requests.length, 0);
    const options = {url: 'https://example.com/login', callback_scheme: 'demo', ephemeral: true};
    const pending = app.openAuthSession(options);
    const open = requests.pop();
    assert.equal(open.type, 'auth_session'); assert.deepEqual(open.request, {action: 'open', request: options});
    const close = app.closeAuthSession(), cancel = requests.pop();
    assert.deepEqual(cancel.request, {action: 'close'});
    globalThis.__jadixDispatch(cancel.key, {value: null}); await close;
    const reply = {type: 'auth_session', outcome: {status: 'cancelled'}, error: null};
    globalThis.__jadixDispatch(open.key, {value: reply}); assert.deepEqual(await pending, reply);
  } finally { delete globalThis.__jadixHost; delete globalThis.__jadixDispatch; }
});

test('app address updates and clears use the shared deep link bridge', async () => {
  const requests = [];
  globalThis.__jadixHost = input => {
    const request = JSON.parse(input);
    if (request.type === 'identity') return JSON.stringify({ok: true, value: {session: '1', root: '2'}});
    requests.push(request); return JSON.stringify({ok: true, value: null});
  };
  try {
    const app = createApplication();
    for (const address of ['demo://inbox/item', null]) {
      const pending = app.setAppAddress(address), request = requests.pop();
      assert.equal(request.type, 'deep_links'); assert.deepEqual(request.request, {action: 'address', address});
      globalThis.__jadixDispatch(request.key, {value: null}); assert.equal(await pending, null);
    }
  } finally { delete globalThis.__jadixHost; delete globalThis.__jadixDispatch; }
});


test('generic keyboard targets share bounded Box and Button contracts', () => {
  const handle = {session: '1', sequence: '2'};
  const keyboard = {focusable: true, keyDown: true, keyUp: true,
    shortcuts: [{id: 'save', key: 's', modifiers: {meta: true}}]};
  for (const component of [Box, Button]) {
    const props = {...(component === Button ? {label: 'Save'} : {}), keyboard, focusRequest: 1,
      events: {key: handle, shortcut: handle}};
    assert.deepEqual(component(props).props.keyboard, keyboard);
    assert.deepEqual(jsx(component, props), component(props));
    assert.throws(() => component({...props, keyboard: {...keyboard, keyDown: 'yes'}}), /boolean/);
    assert.throws(() => component({...props, keyboard: {...keyboard, shortcuts: Array(65).fill(keyboard.shortcuts[0])}}), /limit/);
    assert.throws(() => component({...props, keyboard: {...keyboard, shortcuts: [{id: 'save', key: 's', modifiers: {unknown: true}}]}}), /unknown/);
  }
});


test('generic interaction policies share bounded component and JSX contracts', () => {
  const handle = {session: '1', sequence: '2'};
  const interaction = {press: true, longPressMillis: 500, pan: 'horizontal',
    panActivation: {activeOffsetX: 12.5, failOffsetY: 8},
    pinch: true, rotation: true, touch: true, hover: true};
  for (const component of [Box, Text, Image, Button]) {
    const content = component === Button ? {label: 'Run'} : component === Text ? {text: 'Text'} : component === Image ? {src: 'asset:/image.png'} : {};
    const props = {...content, interaction, events: {interaction: handle}};
    assert.deepEqual(component(props).props.interaction, interaction);
    assert.deepEqual(jsx(component, props), component(props));
    assert.throws(() => component({...props, interaction: {...interaction, press: 'true'}}), /boolean/);
    assert.throws(() => component({...props, interaction: {...interaction, longPressMillis: '500'}}), /number/);
    assert.throws(() => component({...props, interaction: {...interaction, unknown: true}}), /unknown/);
    assert.throws(() => component({...props, interaction: {...interaction, panActivation: {activeOffsetX: '12'}}}), /number/);
  }
});

test('persistence stores bounded JSON and rejects duplicate registrations', () => {
  globalThis.__jadixHost = () => JSON.stringify({ok: true, value: {session: '1', root: '2'}});
  try {
    const app = createApplication();
    let state = {version: 1, value: '中文😀 العربية'};
    assert.throws(() => app.persistence(null, () => {}), /callbacks required/);
    app.persistence(() => state, value => {state = value;});
    assert.deepEqual(JSON.parse(globalThis.__jadixCheckpoint()), state);
    globalThis.__jadixRestore({version: 1, value: 'restored'});
    assert.equal(JSON.parse(globalThis.__jadixCheckpoint()).value, 'restored');
    assert.throws(() => app.persistence(() => null, () => {}), /already configured/);
    state = '😀'.repeat(16385);
    assert.throws(() => globalThis.__jadixCheckpoint(), /64 KiB/);
  } finally {
    for (const key of ['__jadixHost', '__jadixDispatch', '__jadixCheckpoint', '__jadixRestore']) delete globalThis[key];
  }
});

test('WebSocket bridge preserves handle, binary and terminal results', async () => {
  const requests=[];
  globalThis.__jadixHost=(raw)=>{const request=JSON.parse(raw);if(request.type==='identity')return JSON.stringify({ok:true,value:{session:'1',root:'2'}});requests.push(request);return JSON.stringify({ok:true,value:null});};
  try {
    const app=createApplication();
    await assert.rejects(app.websocket(null),/invalid WebSocket/);
    let result=app.websocket({action:'connect',request:{url:'ws://127.0.0.1/',protocols:['chat']}});
    let request=requests.pop(); assert.equal(request.type,'web_socket');
    globalThis.__jadixDispatch(request.key,{value:7});assert.equal(await result,7);
    result=app.websocket({action:'send',id:7,message:{type:'binary',data:[0,255]}});
    request=requests.pop();assert.deepEqual(request.request.message.data,[0,255]);globalThis.__jadixDispatch(request.key,{value:null});await result;
    result=app.websocket({action:'events',id:7});request=requests.pop();const events=[{type:'closed',code:1000,reason:'done'}];globalThis.__jadixDispatch(request.key,{value:events});assert.deepEqual(await result,events);
  } finally {delete globalThis.__jadixHost;delete globalThis.__jadixDispatch;}
});
