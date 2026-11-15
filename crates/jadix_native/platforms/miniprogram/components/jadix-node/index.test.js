'use strict';

const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
let component;
vm.runInNewContext(fs.readFileSync(path.join(__dirname, 'index.js'), 'utf8'), {Component(value) { component = value; }});

test('confirmation commits its final Unicode value before submitting and preserves disabled controls', () => {
  const events = [];
  const control = {data: {node: {kind: 'input', enabled: true, text: 'old', root: '1', id: '2'}},
    ...component.methods, triggerEvent(name, detail) { events.push(JSON.parse(JSON.stringify(detail))); }};
  control.onConfirm({detail: {value: '中😀', cursor: 2}});
  assert.deepEqual(events.map(event => event.payload), [
    {kind: 'edit', value: '中😀', selection: [1, 1], composing: null}, {kind: 'submit'},
  ]);
  assert.ok(events.every(event => event.root === '1' && event.node === '2'));
  events.length = 0;
  control.data.node.text = '中😀';
  control.onConfirm({detail: {value: '中😀'}});
  assert.deepEqual(events.map(event => event.payload), [{kind: 'submit'}]);
  for (const node of [{kind: 'input', enabled: false}, {kind: 'button', enabled: true}, null]) {
    events.length = 0; control.data.node = node;
    control.onConfirm({detail: {value: 'ignored'}});
    assert.equal(events.length, 0);
  }
  const template = fs.readFileSync(path.join(__dirname, 'index.wxml'), 'utf8');
  for (const tag of ['input', 'textarea']) assert.match(template, new RegExp('<' + tag + '\\b[^>]*bindconfirm="onConfirm"'));
});

test('picker choices map enabled option indexes back to values and reject unavailable choices', () => {
  const events = [];
  const control = {data: {node: {kind: 'select', enabled: true,
    optionValues: ['first', 'third'], root: '1', id: '3'}},
    ...component.methods, triggerEvent(_name, detail) { events.push(detail.payload); }};
  control.onSelect({detail: {value: '1'}});
  assert.equal(JSON.stringify(events), JSON.stringify([{kind: 'select', value: 'third'}]));
  control.data.node.optionValues = [];
  control.onSelect({detail: {value: '0'}});
  control.data.node.enabled = false;
  control.data.node.optionValues = ['first'];
  control.onSelect({detail: {value: '0'}});
  assert.equal(events.length, 1);
});

test('native radio activation ignores selected, disabled and unrelated controls', () => {
  const events = [];
  const control = {data: {node: {kind: 'button', role: 'radio', enabled: true,
    selected: false, root: '1', id: '4'}}, ...component.methods,
    triggerEvent(_name, detail) { events.push(JSON.parse(JSON.stringify(detail))); }};
  control.onRadio();
  assert.deepEqual(events, [{root: '1', node: '4', payload: {kind: 'activate'}}]);
  for (const node of [
    {kind: 'button', role: 'radio', enabled: true, selected: true},
    {kind: 'button', role: 'radio', enabled: false, selected: false},
    {kind: 'button', enabled: true}, {kind: 'input', role: 'radio', enabled: true}, null,
  ]) {
    control.data.node = node;
    control.onRadio();
  }
  assert.equal(events.length, 1);
  const template = fs.readFileSync(path.join(__dirname, 'index.wxml'), 'utf8');
  assert.match(template, /<radio\b[^>]*checked="\{\{node.selected\}\}"[^>]*disabled="\{\{!node.enabled\}\}"/);
});
