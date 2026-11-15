import { schemas } from './generated.mjs';
import { bytes } from './utf8.mjs';

export const protocol = 1;
export const sdkVersion = '0.1.0';
const registry = new Map(schemas.map(schema => [schema.name, schema]));


function check(schema, value, depth = 0, budget = { nodes: 0, bytes: 0 }) {
  if (++budget.nodes > 4096 || depth > 32) throw new RangeError('value limit');
  switch (schema.type) {
    case 'null': if (value !== null) throw new TypeError('expected null'); break;
    case 'bool': if (typeof value !== 'boolean') throw new TypeError('expected boolean'); break;
    case 'number': if (typeof value !== 'number' || !Number.isFinite(value)) throw new TypeError('expected finite number'); break;
    case 'string': {
      if (typeof value !== 'string') throw new TypeError('string limit');
      const size = bytes(value);
      if (size > schema.max_bytes) throw new TypeError('string limit');
      budget.bytes += size; break;
    }
    case 'array':
      if (!Array.isArray(value) || value.length > schema.max_items || value.length > 4096) throw new TypeError('array limit');
      value.forEach(item => check(schema.items, item, depth + 1, budget)); break;
    case 'object':
      if (value === null || typeof value !== 'object' || Array.isArray(value)) throw new TypeError('expected object');
      if (Object.keys(value).length > 4096) throw new RangeError('object limit');
      for (const [name, field] of Object.entries(schema.fields)) {
        if (field.required && !Object.hasOwn(value, name)) throw new TypeError(`missing property ${name}`);
      }
      for (const [name, item] of Object.entries(value)) {
        if (!Object.hasOwn(schema.fields, name)) throw new TypeError(`unknown property ${name}`);
        budget.bytes += bytes(name);
        check(schema.fields[name].schema, item, depth + 1, budget);
      }
      break;
    default: throw new TypeError('unknown schema');
  }
  if (budget.bytes > 1024 * 1024) throw new RangeError('message limit');
}

function token(value) {
  return typeof value === 'string' && /^[1-9][0-9]{0,19}$/.test(value) && BigInt(value) <= 18446744073709551615n;
}

function handle(value) {
  return value !== null && typeof value === 'object' && Object.keys(value).length === 2 && token(value.session) && token(value.sequence);
}

export function createElement(component, properties = {}, children = []) {
  const schema = registry.get(component);
  if (!schema) throw new TypeError(`unknown component ${component}`);
  const { events = {}, style = {}, ...props } = properties;
  check(schema.properties, props);
  if ((component === 'Input' || component === 'Textarea') && Object.hasOwn(props, 'maxLength') &&
      (!Number.isSafeInteger(props.maxLength) || props.maxLength < 0 || props.maxLength > 1024 * 1024)) {
    throw new RangeError('maxLength must be an integer from 0 to 1048576');
  }
  for (const [name, reference] of Object.entries(events)) {
    if (!Object.hasOwn(schema.events, name) || !handle(reference)) throw new TypeError(`invalid event ${name}`);
  }
  if (!Array.isArray(children) || (!schema.children && children.length)) throw new TypeError('unsupported children');
  return { component, props, style, events, children };
}

export const Box = ({ children = [], ...props }) => createElement('Box', props, children);
export const Text = props => createElement('Text', props);
export const Button = props => createElement('Button', props);
export const Input = props => createElement('Input', props);
export const Textarea = props => createElement('Textarea', props);
export const Checkbox = props => createElement('Checkbox', props);
export const Switch = props => createElement('Switch', props);
export const Select = props => createElement('Select', props);
export const Image = props => createElement('Image', props);
export const DialogContent = ({ children = [], ...props }) => createElement('DialogContent', props, children);
export const PopoverContent = ({ children = [], ...props }) => createElement('PopoverContent', props, children);
export const Scroll = ({ children = [], ...props }) => createElement('Scroll', props, children);

export function createClient(transport, session) {
  if (typeof transport !== 'function' || !token(session)) throw new TypeError('invalid host transport');
  let sequence = 0n;
  const send = command => Promise.resolve().then(() => transport({ protocol, session, command }));
  return {
    mount(root, tree) { if (!token(root)) throw new TypeError('invalid root'); return send({ type: 'mount', root, tree }); },
    writeState(reference, value) { if (!handle(reference) || reference.session !== session) throw new TypeError('invalid state handle'); return send({ type: 'write_state', handle: reference, value }); },
    event(reference, argumentsValue) { if (!handle(reference) || reference.session !== session) throw new TypeError('invalid event handle'); return send({ type: 'event', handle: reference, arguments: argumentsValue }); },
    call(method, argumentsValue) {
      if (typeof method !== 'string' || !method || method.length > 128) throw new TypeError('invalid method');
      if (++sequence > 18446744073709551615n) throw new RangeError('request identity exhausted');
      const request = String(sequence);
      return { request, result: send({ type: 'call', request, method, arguments: argumentsValue }), cancel: () => send({ type: 'cancel', request }) };
    },
  };
}

/** One application per shell runtime. Event handles are allocated by Session. */
export function createApplication() {
  if (typeof globalThis.__jadixHost !== 'function' || globalThis.__jadixDispatch) throw new Error('application host unavailable or already bound');
  const host = request => {
    const response = JSON.parse(globalThis.__jadixHost(JSON.stringify(request)));
    if (!response.ok) throw new Error(response.error);
    return response.value;
  };
  const identity = host({ type: 'identity' });
  const callbacks = new Map();
  let sequence = 0;
  globalThis.__jadixDispatch = (key, value) => { callbacks.get(key)?.(value); };
  return {
    ...identity,
    event(component, event, callback) {
      if (typeof callback !== 'function') throw new TypeError('event callback required');
      const key = String(++sequence);
      const reference = host({ type: 'register', component, event, key });
      callbacks.set(key, callback);
      return Object.freeze(Object.defineProperty(reference, 'release', { value() { host({ type: 'release', key }); callbacks.delete(key); } }));
    },
    mount(tree) { return host({ type: 'mount', tree }); },
    patch(patches) {
      if (!Array.isArray(patches) || !patches.length || patches.length > 1024 ||
          patches.some(p => !p || !Array.isArray(p.path) || p.path.length > 32 ||
            p.path.some(i => !Number.isSafeInteger(i) || i < 0))) throw new TypeError('invalid leaf patches');
      return host({ type: 'patch', patches });
    },
  };
}
