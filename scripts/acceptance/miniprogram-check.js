'use strict';

const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const {parseArgs} = require('node:util');

const {values} = parseArgs({options: {endpoint: {type: 'string'}, project: {type: 'string'}, output: {type: 'string'}, 'probe-module': {type: 'string'}}});
if (!values.endpoint || !values.project || !values.output) throw new Error('supply --endpoint ws://127.0.0.1:PORT --project DIRECTORY --output NEW_DIRECTORY');
const endpoint = new URL(values.endpoint);
if (endpoint.protocol !== 'ws:' || !['127.0.0.1', 'localhost', '[::1]'].includes(endpoint.hostname)) throw new Error('select a loopback WeChat automation endpoint');
const project = path.resolve(values.project), output = path.resolve(values.output);
fs.mkdirSync(output);
const report = {status: 'failed', observed_at: new Date().toISOString(), project, endpoint: values.endpoint,
  checks: [], ignored: [], limits: ['Real WeChat appservice/WXWebAssembly with synthetic component payloads; device input methods and screen readers need device validation.']};
const callbacks = new Map();
const socket = new WebSocket(values.endpoint);
socket.onmessage = event => {
  const packet = JSON.parse(String(event.data));
  const callback = callbacks.get(packet.id);
  if (!callback) return;
  callbacks.delete(packet.id);
  clearTimeout(callback.timer);
  if (packet.error) callback.reject(new Error(JSON.stringify(packet.error)));
  else callback.resolve(packet.result);
};

function send(method, params = {}, timeout = 5000) {
  const id = crypto.randomUUID();
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { callbacks.delete(id); reject(new Error(method + ' timed out')); }, timeout);
    callbacks.set(id, {resolve, reject, timer});
    socket.send(JSON.stringify({id, method, params}));
  });
}

async function evaluate(callback, ...args) {
  return (await send('App.callFunction', {functionDeclaration: callback.toString(), args})).result;
}

function prepareProbe(filename) {
  const page = getCurrentPages()[0], Runtime = page.runtime.constructor;
  page.runtime.dispose();
  const runtime = new Runtime(page, wx.createCanvasContext('jadix-measure', page));
  page.runtime = runtime;
  page.unloaded = false;
  page.jadixProbe = {status: 'loading'};
  runtime.load(WXWebAssembly, '/' + filename).then(() => {
    if (page.runtime !== runtime) return;
    try { runtime.mount(360, 640); page.setData({error: ''}); page.jadixProbe = {status: 'mounted'}; }
    catch (error) { page.jadixProbe = {status: 'failed', message: error.message}; }
  }, error => { if (page.runtime === runtime) page.jadixProbe = {status: 'failed', message: error.message}; });
}

async function main() {
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('WeChat connection timed out')), 5000);
    socket.onopen = () => { clearTimeout(timer); resolve(); };
    socket.onerror = error => { clearTimeout(timer); reject(error); };
  });
  const current = await send('App.getCurrentPage');
  assert.equal(current.path, 'pages/index/index');
  if (values['probe-module']) {
    const bytes = fs.readFileSync(path.resolve(values['probe-module']));
    const digest = crypto.createHash('sha256').update(bytes).digest('hex');
    const extension = values['probe-module'].endsWith('.wasm.br') ? '.wasm.br' : '.wasm';
    const filename = 'jadix-acceptance-' + digest.slice(0, 12) + extension;
    const target = path.join(project, filename);
    if (fs.existsSync(target)) assert.deepEqual(fs.readFileSync(target), bytes);
    else fs.writeFileSync(target, bytes, {flag: 'wx'});
    report.probe = {source: path.resolve(values['probe-module']), filename, sha256: digest,
      scope: 'Supplemental module load into the selected project; not a normal compressed package startup proof.'};
    for (let attempt = 0; ; attempt++) {
      try { await evaluate(prepareProbe, filename); break; }
      catch (error) {
        if (attempt === 2) throw error;
        report.preparation_retries = (report.preparation_retries || []).concat(error.message);
        await new Promise(resolve => setTimeout(resolve, 200));
      }
    }
    let state;
    for (let attempt = 0; attempt < 20; attempt++) {
      state = await evaluate(() => getCurrentPages()[0].jadixProbe);
      if (state.status !== 'loading') break;
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    assert.equal(state.status, 'mounted', JSON.stringify(state));
  }
  const initial = await evaluate(() => {
    const page = getCurrentPages()[0];
    return {error: page.data.error, root: page.runtime.root, nodes: page.runtime.nodes.size,
      children: page.data.roots[0] ? page.data.roots[0].children.slice(0, 6).map(node => ({id: node.id, kind: node.kind, text: node.text, frame: node.frame})) : []};
  });
  assert.equal(initial.error, '');
  assert.ok(initial.root && initial.nodes >= 57);
  report.initial = initial;
  report.checks.push('WXWebAssembly mount and shared native frames');
  const interactions = await evaluate(() => {
    const page = getCurrentPages()[0], root = page.runtime.root, children = page.data.roots[0].children;
    const emit = (index, payload) => page.onJadixEvent({detail: {root, node: children[index].id, payload}});
    emit(1, {kind: 'activate'});
    const value = '中😀👩‍💻e\u0301';
    emit(2, {kind: 'edit', value, selection: [value.length, value.length], composing: null});
    emit(2, {kind: 'focus'});
    emit(3, {kind: 'toggle', checked: true});
    emit(4, {kind: 'select', value: 'b'});
    emit(5, {kind: 'scroll', x: 0, y: 96});
    const environment = {platform: {name: 'miniprogram', os: 'ios'},
      locale: {locale: 'zh-CN', languages: ['zh-CN'], time_zone: null, hour_cycle: null, first_day_of_week: null, measurement_system: null, temperature_unit: null},
      color_scheme: 'dark', reduced_motion: null, font_scale: 1.25, lifecycle: 'active'};
    page.runtime.environment(environment);
    page.preferences = environment;
    page.onResize({size: {windowWidth: 480, windowHeight: 720}});
    page.onHide(); page.onShow();
    const next = page.data.roots[0].children;
    return {error: page.data.error, button: next[1].text, input: next[2].text, focused: next[2].focused,
      checked: next[3].checked, select: next[4].value, scroll: next[5].scrollY, fontSize: next[0].style.font_size,
      sameIds: children.every((node, index) => next[index].id === node.id), background: page.data.roots[0].style.background};
  });
  assert.equal(interactions.error, '');
  assert.equal(interactions.button, 'Count: 1');
  assert.equal(interactions.input, '中😀👩‍💻e\u0301');
  assert.equal(interactions.focused, true);
  assert.equal(interactions.checked, true);
  assert.equal(interactions.select, 'b');
  assert.equal(interactions.scroll, 96);
  assert.equal(interactions.fontSize, 20);
  assert.equal(interactions.sameIds, true);
  assert.deepEqual(interactions.background, [24, 24, 27, 255]);
  report.interactions = interactions;
  report.checks.push('button, UTF-16 editing, toggle, select, focus, scroll, theme, font scale, resize and lifecycle');
  const overlays = await evaluate(() => {
    const page = getCurrentPages()[0];
    const children = () => page.data.roots[0].children;
    const send = (node, payload) => page.onJadixEvent({detail: {root: node.root, node: node.id, payload}});
    const value = '多行😀\nsecond';
    send(children()[6], {kind: 'edit', value, selection: [value.length, value.length], composing: null});
    send(children()[7], {kind: 'toggle', checked: true});
    send(children().find(node => node.text === 'Open dialog'), {kind: 'activate'});
    const dialog = children().find(node => node.kind === 'dialog');
    const dialogOpen = dialog.open;
    send(dialog, {kind: 'dismiss', backdrop: true});
    const dialogClosed = !children().find(node => node.kind === 'dialog').open;
    send(children().find(node => node.text === 'Open popover'), {kind: 'activate'});
    const popover = children().find(node => node.kind === 'popover');
    const popoverOpen = popover.open;
    const frame = popover.frame;
    send(popover.children.find(node => node.text === 'Close popover'), {kind: 'activate'});
    return {error: page.data.error, textarea: children()[6].text, switched: children()[7].checked,
      dialogOpen, dialogClosed, popoverOpen, popoverClosed: !children().find(node => node.kind === 'popover').open, frame};
  });
  assert.equal(overlays.error, '');
  assert.equal(overlays.textarea, '多行😀\nsecond');
  assert.equal(overlays.switched, true);
  for (const key of ['dialogOpen', 'dialogClosed', 'popoverOpen', 'popoverClosed']) assert.equal(overlays[key], true);
  assert.ok(overlays.frame.every(Number.isFinite));
  report.overlays = overlays;
  report.checks.push('textarea, switch, dialog dismissal and anchored popover through shared Rust events');
  try {
    const screenshot = await send('App.captureScreenshot', {}, 3000);
    if (!screenshot.data) throw new Error('empty simulator screenshot');
    const bytes = Buffer.from(screenshot.data, 'base64');
    fs.writeFileSync(path.join(output, 'simulator.png'), bytes);
    report.screenshot_sha256 = crypto.createHash('sha256').update(bytes).digest('hex');
  } catch (error) { report.ignored.push({validation: 'simulator pixels/native pointer interactions', reason: error.message}); }
  const teardown = await evaluate(() => {
    const page = getCurrentPages()[0], button = page.data.roots[0].children[1];
    page.onUnload();
    return {closed: page.runtime.closed, nodes: page.runtime.nodes.size, roots: page.data.roots.length,
      lateAccepted: page.runtime.send({root: button.root, node: button.id, payload: {kind: 'activate'}})};
  });
  assert.deepEqual(teardown, {closed: true, nodes: 0, roots: 0, lateAccepted: false});
  report.teardown = teardown;
  report.checks.push('unload disposal, empty scene and late-event fencing');
  report.status = 'pass';
}

main().catch(error => { report.error = error.stack; process.exitCode = 1; }).finally(() => {
  socket.close();
  for (const callback of callbacks.values()) clearTimeout(callback.timer);
  fs.writeFileSync(path.join(output, 'acceptance.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify(report));
});
