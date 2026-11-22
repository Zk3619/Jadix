import { test } from 'node:test';
import assert from 'node:assert/strict';
import { promises as fs } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { Bridge } from './server.mjs';

async function fixture(t) {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'cursor-mcp-'));
  t.after(() => fs.rm(dir, { recursive: true, force: true }));
  return dir;
}

test('two editor rounds block, report, require review, and isolate old results', async t => {
  const dir = await fixture(t);
  const codex = new Bridge(dir);
  const cursor = new Bridge(dir, 'cursor');
  let released = false;
  const dispatch = codex.call('cursor_dispatch', { prompt: 'First task 中文', mode: 'editor', timeout_seconds: 3 }).then(r => { released = true; return r; });
  const first = await cursor.call('cursor_next_task', { timeout_seconds: 3 });
  assert.equal(released, false);
  await assert.rejects(codex.call('cursor_dispatch', { prompt: 'duplicate', mode: 'editor' }), /still active/);
  await assert.rejects(codex.call('cursor_review', { task_id: first.task_id, verdict: 'accept', feedback: 'too early' }), /before completion/);
  await cursor.call('cursor_complete', { task_id: first.task_id, status: 'completed', summary: 'Done', checks: ['test passed'] });
  assert.equal((await dispatch).state, 'awaiting_review');
  await assert.rejects(cursor.call('cursor_complete', { task_id: first.task_id, status: 'completed', summary: 'duplicate' }), /EEXIST/);
  await codex.call('cursor_review', { task_id: first.task_id, verdict: 'revise', feedback: 'Fix the missing case' });
  const secondDispatch = codex.call('cursor_dispatch', { prompt: 'Fix the missing case', mode: 'editor', timeout_seconds: 1 });
  const second = await cursor.call('cursor_next_task', { timeout_seconds: 3 });
  assert.notEqual(first.task_id, second.task_id);
  assert.equal((await secondDispatch).state, 'pending');
  const reconnected = new Bridge(dir);
  const waiting = reconnected.call('cursor_wait', { task_id: second.task_id, timeout_seconds: 3 });
  await cursor.call('cursor_complete', { task_id: second.task_id, status: 'blocked', summary: 'Needs credentials' });
  assert.equal((await waiting).result.status, 'blocked');
  await codex.call('cursor_review', { task_id: second.task_id, verdict: 'accept', feedback: 'Acknowledged blocked result' });
  assert.equal((await codex.call('cursor_status')).state, 'idle');
});

test('CLI really launches and writes a completion file; missing CLI reports failure', async t => {
  const dir = await fixture(t);
  const script = path.join(dir, 'fake-cli.mjs');
  await fs.writeFile(script, `import fs from 'node:fs'; import path from 'node:path';
const root=path.join(process.cwd(), '.cursor-handoff');
const active=JSON.parse(fs.readFileSync(path.join(root,'active.json')));
const task=JSON.parse(fs.readFileSync(path.join(root,active.task_id,'task.json')));
fs.writeFileSync(task.result_file+'.tmp',JSON.stringify({task_id:task.task_id,status:'completed',summary:'CLI report'}));
fs.renameSync(task.result_file+'.tmp',task.result_file);`);
  const codex = new Bridge(dir, 'codex', [process.execPath, script]);
  const result = await codex.call('cursor_dispatch', { prompt: 'test', mode: 'cli', timeout_seconds: 5 });
  assert.equal(result.result.summary, 'CLI report');
  // Wait for the process exit callback before releasing this task.
  await codex.until(async () => await codex.read(path.join(codex.taskDir(result.task.task_id), 'process.json')), 3);
  await codex.call('cursor_review', { task_id: result.task.task_id, verdict: 'accept', feedback: 'Checked' });
  codex.cli = [path.join(dir, 'does-not-exist')];
  const failed = await codex.call('cursor_dispatch', { prompt: 'test', mode: 'cli', timeout_seconds: 5 });
  assert.equal(failed.result.status, 'failed');
  assert.match(failed.result.summary, /completion file/);
});

test('two independent dispatchers cannot overwrite the active task', async t => {
  const dir = await fixture(t);
  const attempts = await Promise.allSettled([new Bridge(dir), new Bridge(dir)].map(b => b.call('cursor_dispatch', { mode: 'editor', prompt: 'one winner', timeout_seconds: 1 })));
  assert.equal(attempts.filter(r => r.status === 'fulfilled').length, 1);
  assert.equal((await new Bridge(dir).status()).state, 'pending');
});

test('cancellation releases watchers; invalid receipts never pass review', async t => {
  const dir = await fixture(t);
  const codex = new Bridge(dir);
  const result = await codex.call('cursor_dispatch', { mode: 'editor', prompt: 'test', timeout_seconds: 1 });
  const taskId = result.task.task_id;
  const controller = new AbortController();
  const waiting = codex.call('cursor_wait', { task_id: taskId }, controller.signal);
  controller.abort();
  await assert.rejects(waiting, /abort/i);
  await fs.writeFile(result.task.result_file, JSON.stringify({ task_id: 'old-task', status: 'completed', summary: 'stale' }));
  await assert.rejects(codex.call('cursor_review', { task_id: taskId, verdict: 'accept', feedback: 'bad' }), /Invalid completion/);
  await assert.rejects(codex.call('cursor_wait', { task_id: '../../escape' }), /Invalid task_id/);
  await assert.rejects(codex.call('cursor_wait', { task_id: taskId, timeout_seconds: -1 }), /Invalid timeout/);
});

test('real stdio MCP session initializes, lists tools, cancels pending call and responds to ping', async t => {
  const dir = await fixture(t);
  const child = spawn(process.execPath, [fileURLToPath(new URL('./server.mjs', import.meta.url)), '--workspace', dir, '--role', 'cursor'], { windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
  t.after(() => child.kill());
  const pending = new Map();
  const lines = createInterface({ input: child.stdout });
  lines.on('line', line => { const result = JSON.parse(line); pending.get(result.id)?.(result); pending.delete(result.id); });
  const send = value => child.stdin.write(JSON.stringify({ jsonrpc: '2.0', ...value }) + '\n');
  const request = (id, method, params) => new Promise(resolve => { pending.set(id, resolve); send({ id, method, params }); });
  assert.equal((await request(1, 'initialize', { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'test', version: '1' } })).result.serverInfo.name, 'cursor-handoff');
  assert.equal((await request(2, 'tools/list')).result.tools.length, 2);
  const waiting = request(3, 'tools/call', { name: 'cursor_next_task', arguments: { timeout_seconds: 10 } });
  assert.deepEqual((await request(4, 'ping')).result, {});
  send({ method: 'notifications/cancelled', params: { requestId: 3 } });
  assert.equal((await waiting).result.isError, true);
  child.stdin.end();
  await new Promise(resolve => child.once('exit', resolve));
});
