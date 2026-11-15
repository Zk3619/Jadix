'use strict';

const MAX_U64 = '18446744073709551615';
const KINDS = new Set(['box', 'text', 'button', 'input', 'textarea', 'checkbox', 'switch', 'select', 'scroll', 'image', 'dialog', 'popover']);

function decimal(value) {
  return typeof value === 'string' && /^(0|[1-9][0-9]*)$/.test(value) &&
    (value.length < MAX_U64.length || (value.length === MAX_U64.length && value <= MAX_U64));
}

function increment(value) {
  if (!decimal(value) || value === MAX_U64) throw new Error('revision exhausted');
  const digits = value.split('');
  for (let index = digits.length - 1; index >= 0; index--) {
    if (digits[index] !== '9') { digits[index] = String(Number(digits[index]) + 1); return digits.join(''); }
    digits[index] = '0';
  }
  return '1' + digits.join('');
}

function greater(left, right) {
  return left.length > right.length || (left.length === right.length && left > right);
}

function focusChain(nodes, id) {
  const chain = new Set();
  while (id !== null && nodes.has(id) && !chain.has(id)) {
    chain.add(id);
    id = nodes.get(id).parent;
  }
  return chain;
}

function encode(text) {
  const escaped = encodeURIComponent(text);
  const bytes = [];
  for (let index = 0; index < escaped.length; index++) {
    if (escaped[index] === '%') { bytes.push(parseInt(escaped.slice(index + 1, index + 3), 16)); index += 2; }
    else bytes.push(escaped.charCodeAt(index));
  }
  return Uint8Array.from(bytes);
}

function decode(bytes) {
  return decodeURIComponent(Array.from(bytes, byte => '%' + byte.toString(16).padStart(2, '0')).join(''));
}

function rgba(color) {
  if (!Array.isArray(color) || color.length !== 4 || color.some(value => !Number.isInteger(value) || value < 0 || value > 255)) throw new Error('invalid color');
  return 'rgba(' + color.slice(0, 3).join(',') + ',' + color[3] / 255 + ')';
}

function font(style) {
  const family = /^[a-zA-Z0-9 ,_-]+$/.test(style.font_family) ? style.font_family : 'sans-serif';
  return style.font_weight + ' ' + style.font_size + 'px ' + family;
}

function descriptor(value) {
  if (!value || !KINDS.has(value.kind) || !Array.isArray(value.frame) || value.frame.length !== 4 ||
      value.frame.some(number => !Number.isFinite(number)) || value.frame[2] < 0 || value.frame[3] < 0 ||
      !value.style || !Number.isFinite(value.style.font_size) || value.style.font_size < 0 ||
      !Number.isFinite(value.style.font_weight)) throw new Error('invalid node descriptor');
  rgba(value.style.foreground);
  rgba(value.style.background);
  if (value.editRevision !== undefined && !decimal(value.editRevision)) throw new Error('invalid edit revision');
  for (const field of ['name', 'description']) {
    if (value[field] != null && typeof value[field] !== 'string') throw new Error('invalid accessibility text');
  }
  if (value.accessibilityRelations !== undefined) {
    const relations = value.accessibilityRelations;
    const validTarget = target => target && decimal(target.root) && target.root !== '0' && decimal(target.id) && target.id !== '0';
    if (!relations || (relations.activeDescendant != null && !validTarget(relations.activeDescendant)) ||
        !Array.isArray(relations.controls) || relations.controls.length > 64 || relations.controls.some(target => !validTarget(target)))
      throw new Error('invalid accessibility relations');
  }
  return value;
}

function spans(node) {
  const text = node.text || '';
  const bytes = encode(text);
  const result = [];
  let cursor = 0;
  for (const run of node.style.inline_styles || []) {
    const start = run.range[0], end = run.range[1];
    if (start < cursor || end <= start || end > bytes.length) throw new Error('invalid inline range');
    if (start > cursor) result.push({text: decode(bytes.subarray(cursor, start)), css: ''});
    let css = '';
    if (run.foreground) css += 'color:' + rgba(run.foreground) + ';';
    if (run.background) css += 'background:' + rgba(run.background) + ';';
    if (run.bold !== null && run.bold !== undefined) css += 'font-weight:' + (run.bold ? '700' : node.style.font_weight) + ';';
    if (run.italic !== null && run.italic !== undefined) css += 'font-style:' + (run.italic ? 'italic' : 'normal') + ';';
    const decoration = [run.underline && 'underline', run.strikethrough && 'line-through'].filter(Boolean);
    if (decoration.length) css += 'text-decoration:' + decoration.join(' ') + ';';
    result.push({text: decode(bytes.subarray(start, end)), css});
    cursor = end;
  }
  if (cursor < bytes.length || !result.length) result.push({text: decode(bytes.subarray(cursor)), css: ''});
  return result;
}

function measureText(context, request) {
  const node = descriptor(request.node), style = node.style;
  if (!context || typeof context.measureText !== 'function') throw new Error('Canvas measureText unavailable');
  context.font = font(style);
  let text = node.text || node.placeholder || '';
  if (node.kind === 'select') text = (node.options.find(option => option.value === node.value) || {}).label || node.placeholder;
  if (node.password) text = '•'.repeat(Array.from(text).length);
  const paragraphs = String(text).split('\n');
  const wrap = node.kind === 'textarea' || (node.kind === 'text' && (!style.text_overflow || style.text_overflow === 'wrap'));
  const measuredWidth = value => {
    const width = context.measureText(value).width;
    if (!Number.isFinite(width) || width < 0) throw new Error('invalid Canvas measurement');
    return width;
  };
  let width = 0, lines = 0;
  const segmenter = typeof Intl !== 'undefined' && Intl.Segmenter ? new Intl.Segmenter(undefined, {granularity: 'grapheme'}) : null;
  for (const [index, paragraph] of paragraphs.entries()) {
    const natural = measuredWidth(paragraph);
    if (!wrap || request.widthMode === 'max' || request.maxWidth <= 0) { width = Math.max(width, natural); lines++; continue; }
    const clusters = request.wrapParagraphs ? request.wrapParagraphs[index]
      : segmenter ? Array.from(segmenter.segment(paragraph), item => item.segment) : Array.from(paragraph);
    let current = '';
    lines++;
    for (const cluster of clusters) {
      const next = measuredWidth(current + cluster);
      if (current && (request.widthMode === 'min' || next > request.maxWidth)) {
        width = Math.max(width, measuredWidth(current)); lines++; current = cluster;
      } else current += cluster;
    }
    width = Math.max(width, measuredWidth(current));
  }
  if (style.max_lines) lines = Math.min(lines, style.max_lines);
  return [Math.min(request.maxWidth, width), Math.min(request.maxHeight, lines * style.font_size * 1.25)];
}

class MiniProgramRuntime {
  constructor(page, context) {
    this.page = page;
    this.context = context;
    this.nodes = new Map();
    this.roots = [];
    this.paths = new Map();
    this.eventRevisions = new Map();
    this.scrollOffsets = new Map();
    this.selections = new Map();
    this.root = null;
    this.revision = '0';
    this.focus = null;
    this.focusVisible = null;
    this.focusWithin = new Set();
    this.closed = false;
    this.busy = false;
    this.pending = [];
  }

  async load(webAssembly, source) {
    const imports = {jadix: {
      measure: (pointer, length, output) => {
        try {
          const size = measureText(this.context, JSON.parse(this.read(pointer, length)));
          new Float64Array(this.instance.exports.memory.buffer, output, 2).set(size);
          return 0;
        } catch (error) { this.lastError = error; return 1; }
      },
      publish: (pointer, length) => {
        try { this.apply(JSON.parse(this.read(pointer, length))); return 0; }
        catch (error) { this.lastError = error; return 1; }
      },
      dispose: () => this.closeView(),
    }};
    const loaded = await webAssembly.instantiate(source, imports);
    this.instance = loaded.instance || loaded;
    for (const name of ['memory', 'jadix_alloc', 'jadix_dealloc', 'jadix_mount', 'jadix_event', 'jadix_environment', 'jadix_resize', 'jadix_dispose', 'jadix_error_pointer', 'jadix_error_length']) {
      if (!this.instance.exports[name]) throw new Error('missing mini-program export: ' + name);
    }
    if (this.closed) this.instance.exports.jadix_dispose();
    return this;
  }

  read(pointer, length) {
    return decode(new Uint8Array(this.instance.exports.memory.buffer, pointer, length));
  }

  call(name, args, accepted) {
    if (this.closed || !this.instance) throw new Error('mini-program surface unavailable');
    if (this.busy) throw new Error('reentrant mini-program call');
    this.lastError = null;
    this.busy = true;
    try {
      if (this.instance.exports[name](...args) !== 0) {
        const exports = this.instance.exports;
        throw this.lastError || new Error(this.read(exports.jadix_error_pointer(), exports.jadix_error_length()));
      }
      if (accepted) accepted();
    } finally {
      this.busy = false;
      const pending = this.pending.splice(0);
      for (const event of pending) this.send(event);
    }
  }

  message(name, value, accepted) {
    if (this.closed || !this.instance) throw new Error('mini-program surface unavailable');
    const bytes = encode(JSON.stringify(value));
    const pointer = this.instance.exports.jadix_alloc(bytes.length);
    if (!pointer) throw new Error('mini-program message exceeds buffer limit');
    try {
      new Uint8Array(this.instance.exports.memory.buffer, pointer, bytes.length).set(bytes);
      this.call(name, [pointer, bytes.length], accepted);
    } finally { this.instance.exports.jadix_dealloc(pointer, bytes.length); }
  }

  mount(width, height) { this.call('jadix_mount', [width, height]); }
  environment(value) { this.message('jadix_environment', value); }
  resize(width, height) { this.call('jadix_resize', [width, height]); }

  apply(batch) {
    if (batch.command === 'scroll') {
      if (this.closed || batch.root !== this.root || batch.revision !== this.revision ||
          !this.nodes.has(batch.node) || this.nodes.get(batch.node).descriptor.kind !== 'scroll' ||
          !Array.isArray(batch.offset) || batch.offset.length !== 2 || batch.offset.some(value => !Number.isFinite(value) || value < 0)) throw new Error('invalid scroll command');
      const target = this.paths.get(batch.node);
      this.page.setData({[target + '.scrollX']: batch.offset[0], [target + '.scrollY']: batch.offset[1]});
      this.scrollOffsets.set(batch.node, batch.offset.slice());
      return;
    }
    if (this.closed || batch.protocol !== 2 || !decimal(batch.root) || batch.root === '0' ||
        (this.root !== null && this.root !== batch.root) || batch.previous !== this.revision ||
        batch.revision !== increment(this.revision) || !Array.isArray(batch.operations)) throw new Error('stale or foreign commit');
    const nodes = new Map(Array.from(this.nodes, ([id, node]) => [id, {...node, children: node.children.slice()}]));
    const roots = this.roots.slice();
    let focus = this.focus, focusVisible = this.focusVisible, structural = false;
    const changed = new Set();
    const siblings = parent => {
      if (parent === null) return roots;
      const node = nodes.get(parent);
      if (!node || !['box', 'scroll', 'dialog', 'popover'].includes(node.descriptor.kind)) throw new Error('invalid parent');
      return node.children;
    };
    for (const operation of batch.operations) {
      if (operation.op === 'focus') {
        if (operation.id !== null && !nodes.has(operation.id)) throw new Error('invalid focus');
        if (focus) changed.add(focus);
        focus = operation.id;
        focusVisible = null;
        if (focus) changed.add(focus);
        continue;
      }
      if (!decimal(operation.id) || operation.id === '0') throw new Error('invalid node identity');
      const existing = nodes.get(operation.id);
      if (operation.op === 'create' || operation.op === 'move') {
        if (!Number.isInteger(operation.index) || operation.index < 0 ||
            (operation.parent !== null && !decimal(operation.parent))) throw new Error('invalid placement');
        if (operation.op === 'create') {
          if (existing) throw new Error('duplicate node');
          const value = descriptor(operation.descriptor);
          nodes.set(operation.id, {descriptor: value, parent: operation.parent, children: []});
        } else {
          if (!existing) throw new Error('unknown move');
          const previous = siblings(existing.parent);
          const position = previous.indexOf(operation.id);
          if (position < 0) throw new Error('detached move');
          previous.splice(position, 1);
          existing.parent = operation.parent;
        }
        const children = siblings(operation.parent);
        if (operation.index > children.length) throw new Error('invalid child index');
        children.splice(operation.index, 0, operation.id);
        structural = true;
      } else if (operation.op === 'update') {
        if (!existing) throw new Error('unknown update');
        existing.descriptor = descriptor(operation.descriptor);
        changed.add(operation.id);
      } else if (operation.op === 'remove') {
        if (!existing) throw new Error('unknown removal');
        const children = siblings(existing.parent);
        children.splice(children.indexOf(operation.id), 1);
        const removed = [operation.id], seen = new Set();
        while (removed.length) {
          const id = removed.pop();
          if (seen.has(id) || !nodes.has(id)) throw new Error('invalid removal tree');
          seen.add(id);
          removed.push(...nodes.get(id).children);
          nodes.delete(id);
          if (focus === id) {
            focus = null;
            focusVisible = null;
          }
        }
        structural = true;
      } else throw new Error('unknown operation');
    }
    const visited = new Set(), pending = roots.slice();
    while (pending.length) {
      const id = pending.pop(), node = nodes.get(id);
      if (!node || visited.has(id)) throw new Error('cyclic or duplicate tree');
      visited.add(id);
      for (const child of node.children) {
        if (!nodes.has(child) || nodes.get(child).parent !== id) throw new Error('invalid child ownership');
        pending.push(child);
      }
    }
    if (visited.size !== nodes.size) throw new Error('orphaned tree');
    for (const id of focusChain(this.nodes, this.focus)) changed.add(id);
    for (const id of focusChain(nodes, focus)) changed.add(id);
    const focusWithin = focusChain(nodes, focus);
    const paths = new Map(), projected = new Map();
    let overlay = 0;
    const build = (id, path, parentFrame) => {
      const node = nodes.get(id), value = node.descriptor, frame = value.frame, style = value.style;
      const panel = value.kind === 'dialog' || value.kind === 'popover';
      const overlayIndex = panel ? ++overlay : 0;
      if (panel) parentFrame = [0, 0, 0, 0];
      let css = 'position:absolute;box-sizing:border-box;margin:0;padding:0;border:0;left:' + (frame[0] - parentFrame[0]) +
        'px;top:' + (frame[1] - parentFrame[1]) + 'px;width:' + frame[2] + 'px;height:' + frame[3] +
        'px;color:' + rgba(style.foreground) + ';background:' + rgba(style.background) + ';font:' + font(style) +
        ';line-height:' + style.font_size * 1.25 + 'px;direction:' +
        (style.direction === 'right-to-left' ? 'rtl' : 'ltr') + ';';
      if (value.kind === 'text' && style.max_lines) css += 'display:-webkit-box;-webkit-box-orient:vertical;-webkit-line-clamp:' + style.max_lines + ';';
      const children = node.children.map((child, index) => build(child, path + '.children[' + index + ']', frame));
      const result = {...value, id, root: batch.root, css, overlay: overlayIndex, focused: focus === id,
        focusVisible: focusVisible === id, focusWithin: focusWithin.has(id), children,
        spans: spans(value), scrollX: (this.scrollOffsets.get(id) || [0, 0])[0], scrollY: (this.scrollOffsets.get(id) || [0, 0])[1],
        contentWidth: Math.max(frame[2], ...node.children.map(child => nodes.get(child).descriptor.frame[0] + nodes.get(child).descriptor.frame[2] - frame[0])),
        contentHeight: Math.max(frame[3], ...node.children.map(child => nodes.get(child).descriptor.frame[1] + nodes.get(child).descriptor.frame[3] - frame[1])),
      };
      const selection = this.selections.get(id);
      const range = selection && selection.text === value.text ? selection.range : [-1, -1];
      result.selectionStart = range[0];
      result.selectionEnd = range[1];
      const selected = (value.options || []).find(option => option.value === value.value);
      const enabledOptions = (value.options || []).filter(option => option.enabled);
      const selectedIndex = enabledOptions.findIndex(option => option.value === value.value);
      result.optionLabels = enabledOptions.map(option => option.label);
      result.optionValues = enabledOptions.map(option => option.value);
      result.selectedIndex = selectedIndex < 0 ? 0 : selectedIndex;
      result.selectEnabled = value.enabled && enabledOptions.length > 0;
      result.selectedLabel = selected ? selected.label : value.placeholder;
      const fallback = ['input', 'textarea', 'select'].includes(value.kind) ? value.placeholder
        : value.kind === 'image' ? value.alt
        : ['button', 'checkbox', 'switch'].includes(value.kind) || value.description ? value.text : '';
      const label = value.name || fallback || '';
      result.accessibilityLabel = [label, value.description].filter(Boolean).join(', ');
      const relations = value.accessibilityRelations || {};
      const activeDescendant = relations.activeDescendant;
      result.accessibilityId = 'jadix-' + batch.root + '-' + id;
      const activeId = activeDescendant && activeDescendant.root === batch.root && nodes.has(activeDescendant.id)
        ? 'jadix-' + batch.root + '-' + activeDescendant.id : null;
      result.accessibilityActiveDescendant = activeId;
      result.accessibilityControls = (relations.controls || [])
        .filter(target => target.root === batch.root && nodes.has(target.id))
        .map(target => 'jadix-' + batch.root + '-' + target.id);
      result.accessibilityControlsAttribute = result.accessibilityControls.join(' ');
      paths.set(id, path);
      projected.set(id, result);
      return result;
    };
    const view = roots.map((id, index) => build(id, 'roots[' + index + ']', [0, 0, 0, 0]));
    const patch = {};
    if (structural || this.root === null) patch.roots = view;
    else {
      for (const id of changed) {
        if (!nodes.has(id)) continue;
        let parent = nodes.get(id).parent, covered = false;
        while (parent !== null) { if (changed.has(parent)) { covered = true; break; } parent = nodes.get(parent).parent; }
        if (!covered) patch[paths.get(id)] = projected.get(id);
      }
    }
    if (Object.keys(patch).length) this.page.setData(patch);
    this.nodes = nodes; this.roots = roots; this.paths = paths;
    this.focus = focus; this.focusVisible = focusVisible; this.focusWithin = focusWithin;
    this.root = batch.root; this.revision = batch.revision;
    for (const id of this.eventRevisions.keys()) if (!nodes.has(id)) this.eventRevisions.delete(id);
    for (const id of this.scrollOffsets.keys()) if (!nodes.has(id)) this.scrollOffsets.delete(id);
    for (const [id, selection] of this.selections) {
      if (!nodes.has(id) || nodes.get(id).descriptor.text !== selection.text) this.selections.delete(id);
    }
  }

  send(event) {
    if (this.closed || !event || event.root !== this.root || !this.nodes.has(event.node)) return false;
    if (this.busy) { this.pending.push(event); return true; }
    const node = this.nodes.get(event.node).descriptor;
    if (node.enabled === false && event.payload.kind !== 'blur') return false;
    const payload = {...event.payload};
    let revision;
    if (['edit', 'toggle', 'select', 'dismiss', 'submit'].includes(payload.kind)) {
      const controlled = node.editRevision || '0', emitted = this.eventRevisions.get(event.node) || '0';
      revision = increment(greater(controlled, emitted) ? controlled : emitted);
      payload.revision = revision;
    }
    const previousSelection = this.selections.get(event.node);
    if (payload.kind === 'edit') this.selections.set(event.node, {text: payload.value, range: payload.selection.slice()});
    let accepted = false;
    try {
      this.message('jadix_event', {root: this.root, node: event.node, payload}, () => {
        accepted = true;
        if (revision) this.eventRevisions.set(event.node, revision);
        if (payload.kind === 'focus' || payload.kind === 'focusVisible' || (payload.kind === 'blur' && this.focus === event.node)) {
          const next = payload.kind === 'blur' ? null : event.node;
          const nextVisible = payload.kind === 'focusVisible' ? event.node : null, patch = {};
          const nextWithin = focusChain(this.nodes, next);
          for (const id of new Set([...this.focusWithin, ...nextWithin])) {
            if (this.paths.has(id)) patch[this.paths.get(id) + '.focusWithin'] = nextWithin.has(id);
          }
          if (this.focus && this.paths.has(this.focus)) patch[this.paths.get(this.focus) + '.focused'] = false;
          if (this.focusVisible && this.paths.has(this.focusVisible)) patch[this.paths.get(this.focusVisible) + '.focusVisible'] = false;
          if (next) patch[this.paths.get(next) + '.focused'] = true;
          if (next && nextVisible) patch[this.paths.get(next) + '.focusVisible'] = true;
          this.focus = next;
          this.focusVisible = nextVisible;
          this.focusWithin = nextWithin;
          if (Object.keys(patch).length) this.page.setData(patch);
        }
        if (payload.kind === 'scroll') this.scrollOffsets.set(event.node, [payload.x, payload.y]);
      });
    } catch (error) {
      if (!accepted) {
        if (previousSelection) this.selections.set(event.node, previousSelection);
        else this.selections.delete(event.node);
      }
      throw error;
    }
    return true;
  }

  closeView() {
    if (this.closed) return;
    this.closed = true;
    this.pending.length = 0;
    this.nodes.clear(); this.paths.clear(); this.eventRevisions.clear(); this.scrollOffsets.clear(); this.selections.clear(); this.roots = [];
    this.focus = null;
    this.focusVisible = null;
    this.focusWithin.clear();
    this.page.setData({roots: []});
  }

  dispose() {
    if (this.closed) return;
    if (this.instance) this.instance.exports.jadix_dispose();
    this.closeView();
  }
}

function environmentSnapshot(info, theme) {
  const os = {android: 'android', ios: 'ios', windows: 'windows', mac: 'macos', ohos: 'ohos', harmonyos: 'ohos'}[String(info.platform).toLowerCase()] || null;
  const language = String(info.language || 'en-US').replace(/_/g, '-');
  const locale = /^[a-zA-Z]{2,8}(?:-[a-zA-Z0-9]{1,8})*$/.test(language) ? language : 'en-US';
  const scale = Number(info.fontSizeSetting) / 16;
  return {platform: {name: 'miniprogram', os}, locale: {locale, languages: [locale], time_zone: null,
    hour_cycle: null, first_day_of_week: null, measurement_system: null, temperature_unit: null},
    color_scheme: ['dark', 'light'].includes(theme) ? theme : null, reduced_motion: null,
    font_scale: Number.isFinite(scale) && scale >= 0.5 && scale <= 4 ? scale : 1, lifecycle: 'active'};
}

module.exports = {MiniProgramRuntime, environmentSnapshot, encode, decode, increment, measureText};
