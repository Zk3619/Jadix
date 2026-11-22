import { promises as fs, watch } from 'node:fs';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { pathToFileURL } from 'node:url';

const object = (properties, required = []) => ({ type: 'object', properties, required, additionalProperties: false });
const str = { type: 'string', minLength: 1 };
const timeout = { type: 'integer', minimum: 1, maximum: 1800, default: 300 };
const id = { type: 'string', pattern: '^[0-9a-f-]{36}$' };
const definition = (name, description, inputSchema) => ({ name, description, inputSchema });

export class Bridge {
  constructor(workspace, role = 'codex', cli = ['agent']) {
    this.workspace = path.resolve(workspace);
    this.root = path.join(this.workspace, '.cursor-handoff');
    this.role = role;
    this.cli = cli;
    this.children = new Set();
  }

  tools() {
    if (this.role === 'cursor') return [
      definition('cursor_next_task', 'Block until an editor task is available. After reporting a result, call again to wait for Codex review and its next task. Timeout means keep waiting.', object({ timeout_seconds: timeout })),
      definition('cursor_complete', 'Atomically write the completion file. Report actual edits, checks and unresolved issues. Then call cursor_next_task.', object({ task_id: id, status: { enum: ['completed', 'failed', 'blocked'] }, summary: str, checks: { type: 'array', items: str } }, ['task_id', 'status', 'summary'])),
    ];
    return [
      definition('cursor_dispatch', 'Send one task to Cursor editor or CLI and block on its result file. For editor mode start the worker once using the returned bootstrap prompt. A pending result must be awaited with cursor_wait, never dispatched again.', object({ prompt: str, mode: { enum: ['editor', 'cli'] }, timeout_seconds: timeout }, ['prompt', 'mode'])),
      definition('cursor_wait', 'Continue blocking on the SAME task result file after a timeout or reconnect. Completion is a report, not review approval; inspect the diff and run checks before cursor_review.', object({ task_id: id, timeout_seconds: timeout }, ['task_id'])),
      definition('cursor_status', 'Read the current task, completion, and review without blocking.', object({})),
      definition('cursor_review', 'Record Codex review AFTER inspecting actual changes and checks. Both accept and revise release the current task so a new task can be dispatched. Include findings in feedback and in the next prompt.', object({ task_id: id, verdict: { enum: ['accept', 'revise'] }, feedback: str }, ['task_id', 'verdict', 'feedback'])),
    ];
  }

  taskDir(taskId) {
    if (typeof taskId !== 'string' || !/^[0-9a-f-]{36}$/.test(taskId)) throw new Error('Invalid task_id');
    return path.join(this.root, taskId);
  }

  async read(file) {
    try { return JSON.parse((await fs.readFile(file, 'utf8')).replace(/^\uFEFF/, '')); }
    catch (error) { if (error.code === 'ENOENT') return null; throw error; }
  }

  async atomic(file, value) {
    const temp = `${file}.${randomUUID()}.tmp`;
    try {
      await fs.writeFile(temp, JSON.stringify(value, null, 2) + '\n', 'utf8');
      await fs.rename(temp, file);
    } finally { await fs.rm(temp, { force: true }); }
  }

  async current() {
    const active = await this.read(path.join(this.root, 'active.json'));
    if (!active) return null;
    return this.read(path.join(this.taskDir(active.task_id), 'task.json'));
  }

  async status(taskId) {
    const task = taskId ? await this.read(path.join(this.taskDir(taskId), 'task.json')) : await this.current();
    if (!task) return { state: 'idle' };
    const dir = this.taskDir(task.task_id);
    const result = await this.read(path.join(dir, 'result.json'));
    if (result && (result.task_id !== task.task_id || !['completed', 'failed', 'blocked'].includes(result.status) || typeof result.summary !== 'string' || !result.summary.trim())) {
      throw new Error('Invalid completion file; task_id, status and summary are required');
    }
    const review = await this.read(path.join(dir, 'review.json'));
    return { state: review ? 'reviewed' : result ? 'awaiting_review' : 'pending', task, result, review };
  }

  async until(probe, seconds, signal) {
    if (!Number.isInteger(seconds) || seconds < 1 || seconds > 1800) throw new Error('timeout_seconds must be 1..1800');
    await fs.mkdir(this.root, { recursive: true });
    const deadline = Date.now() + seconds * 1000;
    // Watch first, then read: covers writes before and during watcher registration.
    let wake;
    const watcher = watch(this.root, { recursive: true }, () => wake?.());
    watcher.on('error', () => wake?.());
    try {
      while (true) {
        signal?.throwIfAborted();
        const found = await probe();
        if (found) return found;
        const remaining = deadline - Date.now();
        if (remaining <= 0) return null;
        await new Promise(resolve => {
          const done = () => { clearTimeout(timer); signal?.removeEventListener('abort', done); wake = null; resolve(); };
          const timer = setTimeout(done, Math.min(1000, remaining));
          wake = done;
          signal?.addEventListener('abort', done, { once: true });
          if (signal?.aborted) done();
        });
      }
    } finally { watcher.close(); }
  }

  async wait(taskId, seconds, signal) {
    if (!(await this.read(path.join(this.taskDir(taskId), 'task.json')))) throw new Error('Unknown task');
    const finished = await this.until(async () => {
      const state = await this.status(taskId);
      if (state.result) {
        const r = state.result;
        if (r.task_id !== taskId || !['completed', 'failed', 'blocked'].includes(r.status) || typeof r.summary !== 'string' || !r.summary.trim()) {
          throw new Error('Invalid completion file; task_id, status and summary are required');
        }
        return state;
      }
    }, seconds, signal);
    return finished || await this.status(taskId);
  }

  bootstrap() {
    return `在工作区 ${this.workspace} 中作为 Codex 的 Cursor 执行端。调用 cursor_handoff MCP 的 cursor_next_task 等待任务；按返回的 prompt 执行并验证，保留无关改动。通过 cursor_complete 回报真实结果（失败或阻塞也必须回报）。然后立即再次调用 cursor_next_task，等待 Codex 审阅后下发下一步。等待超时继续调用，不能自行进入下一阶段。用户要求停止时结束。`;
  }

  async dispatch(args, signal) {
    await fs.mkdir(this.root, { recursive: true });
    const previous = await this.current();
    if (previous) throw new Error(`Task ${previous.task_id} is still active. Wait and review it before dispatching again.`);
    const taskId = randomUUID();
    const dir = this.taskDir(taskId);
    await fs.mkdir(dir);
    const task = { task_id: taskId, mode: args.mode, prompt: args.prompt, workspace: this.workspace, created_at: new Date().toISOString(), result_file: path.join(dir, 'result.json') };
    await this.atomic(path.join(dir, 'task.json'), task);
    // Exclusive creation also serializes independent Codex MCP processes.
    const activeTemp = path.join(dir, 'active.json');
    await this.atomic(activeTemp, { task_id: taskId });
    await fs.link(activeTemp, path.join(this.root, 'active.json'));
    const bootstrap = this.bootstrap();
    if (args.mode === 'cli') await this.launch(task);
    const state = await this.wait(taskId, args.timeout_seconds ?? 300, signal);
    return { ...state, ...(args.mode === 'editor' ? { bootstrap_prompt: bootstrap, bootstrap_url: `cursor://anysphere.cursor-deeplink/prompt?text=${encodeURIComponent(bootstrap)}` } : {}) };
  }

  async complete(args) {
    const task = await this.current();
    if (!task || task.task_id !== args.task_id) throw new Error('Task is not active');
    const dir = this.taskDir(args.task_id);
    // A permanent claim prevents duplicate terminal reports, including across processes.
    const claim = await fs.open(path.join(dir, 'completion.claim'), 'wx');
    await claim.close();
    try {
      await this.atomic(path.join(dir, 'result.json'), { ...args, completed_at: new Date().toISOString() });
    } catch (error) { await fs.rm(path.join(dir, 'completion.claim'), { force: true }); throw error; }
    return { task_id: args.task_id, state: 'awaiting_review' };
  }

  async launch(task) {
    const dir = this.taskDir(task.task_id);
    const prompt = `Read ${JSON.stringify(path.join(dir, 'task.json'))} and execute its prompt in ${JSON.stringify(this.workspace)}. Preserve unrelated changes. Run relevant checks. Before exiting, atomically write ${JSON.stringify(task.result_file)} as UTF-8 JSON with task_id=${JSON.stringify(task.task_id)}, status (completed/failed/blocked), summary and checks (array of strings). This file is a report for Codex review, not approval. Do not commit or push unless the task explicitly requests it.`;
    let child;
    try {
      const stdout = await fs.open(path.join(dir, 'stdout.log'), 'a');
      const stderr = await fs.open(path.join(dir, 'stderr.log'), 'a');
      let exited;
      try {
        child = spawn(this.cli[0], [...this.cli.slice(1), '--print', '--output-format', 'text', '--workspace', this.workspace, prompt], { cwd: this.workspace, windowsHide: true, shell: false, stdio: ['ignore', stdout.fd, stderr.fd] });
        exited = new Promise(resolve => {
          child.once('error', error => resolve({ error: error.message }));
          child.once('exit', (code, signal) => resolve({ code, signal }));
        });
      } finally { await stdout.close(); await stderr.close(); }
      this.children.add(child);
      void exited.then(async exit => {
        this.children.delete(child);
        await this.atomic(path.join(dir, 'process.json'), { pid: child.pid ?? null, ...exit });
        if (!(await this.read(task.result_file))) {
          await this.complete({ task_id: task.task_id, status: 'failed', summary: `Cursor CLI exited without a completion file: ${JSON.stringify(exit)}. Inspect stdout.log and stderr.log.` });
        }
      }).catch(error => console.error(error.message));
    } catch (error) {
      await this.complete({ task_id: task.task_id, status: 'failed', summary: `Cannot launch Cursor CLI: ${error.message}` });
    }
  }

  async call(name, args = {}, signal) {
    const tool = this.tools().find(tool => tool.name === name);
    if (!tool) throw new Error('Unknown tool for this role');
    if (!args || typeof args !== 'object' || Array.isArray(args)) throw new Error('Expected object arguments');
    for (const key of tool.inputSchema.required) if (args[key] === undefined) throw new Error(`Missing ${key}`);
    for (const [key, value] of Object.entries(args)) {
      const schema = tool.inputSchema.properties[key];
      if (!schema) throw new Error(`Unknown argument ${key}`);
      if (schema.enum && !schema.enum.includes(value)) throw new Error(`Invalid ${key}`);
      if (schema.type === 'string' && (typeof value !== 'string' || !value.trim())) throw new Error(`Invalid ${key}`);
      if (schema.pattern && !new RegExp(schema.pattern).test(value)) throw new Error(`Invalid ${key}`);
      if (schema.type === 'integer' && (!Number.isInteger(value) || value < schema.minimum || value > schema.maximum)) throw new Error(`Invalid ${key}`);
      if (schema.type === 'array' && (!Array.isArray(value) || value.some(v => typeof v !== 'string' || !v.trim()))) throw new Error(`Invalid ${key}`);
    }
    if (name === 'cursor_dispatch') return this.dispatch(args, signal);
    if (name === 'cursor_status') return this.status();
    if (name === 'cursor_wait') return this.wait(args.task_id, args.timeout_seconds ?? 300, signal);
    if (name === 'cursor_complete') return this.complete(args);
    if (name === 'cursor_next_task') {
      return await this.until(async () => {
        const state = await this.status();
        return state.state === 'pending' && state.task.mode === 'editor' ? state.task : null;
      }, args.timeout_seconds ?? 300, signal) || { state: 'pending', instruction: 'Call cursor_next_task again; no new editor task yet.' };
    }
    if (name === 'cursor_review') {
      const current = await this.current();
      if (!current || current.task_id !== args.task_id) throw new Error('Task is not active');
      const state = await this.status(args.task_id);
      if (!state.result) throw new Error('Cannot review before completion');
      const dir = this.taskDir(args.task_id);
      const claim = await fs.open(path.join(dir, 'review.claim'), 'wx');
      await claim.close();
      await this.atomic(path.join(dir, 'review.json'), { ...args, reviewed_at: new Date().toISOString() });
      await fs.unlink(path.join(this.root, 'active.json'));
      return this.status(args.task_id);
    }
  }
}

export function serve(bridge) {
  const running = new Map();
  const send = value => process.stdout.write(JSON.stringify({ jsonrpc: '2.0', ...value }) + '\n');
  const lines = createInterface({ input: process.stdin });
  lines.on('line', async line => {
    let request;
    try { request = JSON.parse(line); } catch { send({ id: null, error: { code: -32700, message: 'Parse error' } }); return; }
    if (request.method === 'notifications/cancelled') { running.get(request.params?.requestId)?.abort(); return; }
    if (request.id === undefined) return;
    const controller = new AbortController();
    running.set(request.id, controller);
    try {
      let result;
      if (request.method === 'initialize') result = { protocolVersion: '2024-11-05', capabilities: { tools: {} }, serverInfo: { name: 'cursor-handoff', version: '1.0.0' } };
      else if (request.method === 'ping') result = {};
      else if (request.method === 'tools/list') result = { tools: bridge.tools() };
      else if (request.method === 'tools/call') {
        try { result = { content: [{ type: 'text', text: JSON.stringify(await bridge.call(request.params.name, request.params.arguments, controller.signal)) }] }; }
        catch (error) { result = { isError: true, content: [{ type: 'text', text: error.message }] }; }
      } else { send({ id: request.id, error: { code: -32601, message: 'Method not found' } }); return; }
      send({ id: request.id, result });
    } catch (error) { send({ id: request.id, error: { code: -32603, message: error.message } }); }
    finally { running.delete(request.id); }
  });
  // Cancels watchers on disconnect. CLI jobs deliberately survive reconnects;
  // their durable task ID prevents accidentally dispatching duplicate work.
  lines.on('close', () => { for (const controller of running.values()) controller.abort(); });
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const options = process.argv.slice(2);
  const option = (name, fallback) => { const index = options.indexOf(name); return index < 0 ? fallback : options[index + 1]; };
  const role = option('--role', 'codex');
  if (!['codex', 'cursor'].includes(role)) throw new Error('--role must be codex or cursor');
  const cli = JSON.parse(process.env.CURSOR_MCP_COMMAND || '["agent"]');
  if (!Array.isArray(cli) || !cli.length || cli.some(v => typeof v !== 'string' || !v)) throw new Error('CURSOR_MCP_COMMAND must be a nonempty JSON argv array');
  serve(new Bridge(option('--workspace', process.cwd()), role, cli));
}
