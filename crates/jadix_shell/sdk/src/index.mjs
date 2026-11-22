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
  if (component === 'Hosted' && children.length && JSON.parse(props.specification).type !== 'menu_button') throw new TypeError('unsupported hosted children');
  return { component, props, style, events, children };
}

export const Box = ({ children = [], ...props }) => createElement('Box', props, children);
export const Text = props => createElement('Text', props);
export const Button = ({ children = [], ...props }) => createElement('Button', props, children);
export const Input = props => createElement('Input', props);
export const Textarea = props => createElement('Textarea', props);
export const Checkbox = props => createElement('Checkbox', props);
export const Switch = props => createElement('Switch', props);
export const Select = props => createElement('Select', props);
export const Image = props => createElement('Image', props);
export const DialogContent = ({ children = [], ...props }) => createElement('DialogContent', props, children);
export const PopoverContent = ({ children = [], ...props }) => createElement('PopoverContent', props, children);
export const Scroll = ({ children = [], ...props }) => createElement('Scroll', props, children);
export const Hosted = ({content, children = [], ...props}) => createElement('Hosted', {...props, specification: JSON.stringify(content)}, children);
export const MenuButton = ({label, items, disabled = false, children = [], ...props}) =>
  Hosted({...props, children, content: {type: 'menu_button', label, items, enabled: !disabled}});
export const TabBar = ({items, selected = null, hidden = false, disabled = false, ...props}) =>
  Hosted({...props, content: {type: 'tab_bar', items, selected, hidden, enabled: !disabled}});
export { createVirtualList, VirtualList } from './virtual-list.mjs';
export { createEmojiPicker, EmojiPicker } from './emoji-picker.mjs';
export const Embed = ({kind, properties = {}, ...props}) => Hosted({...props, content: {type: 'custom', kind, properties}});
export const Calendar = ({value = null, minimum = null, maximum = null, locale = null, disabled = false, ...props}) =>
  Hosted({...props, content: {type: 'date', value, minimum, maximum, locale, enabled: !disabled, mode: 'calendar'}});
export const DatePicker = ({value = null, minimum = null, maximum = null, locale = null, disabled = false, ...props}) =>
  Hosted({...props, content: {type: 'date', value, minimum, maximum, locale, enabled: !disabled, mode: 'field'}});
export const TimeField = ({value = {hour: 0, minute: 0, second: 0}, seconds = true, disabled = false, ...props}) =>
  Hosted({...props, content: {type: 'time', value: {...value, second: seconds ? value.second ?? 0 : 0}, seconds, enabled: !disabled}});
export const ColorPicker = ({value = [0, 0, 0, 255], alpha = true, disabled = false, ...props}) =>
  Hosted({...props, content: {type: 'color', rgba: alpha ? value : [value[0], value[1], value[2], 255], alpha, enabled: !disabled}});

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
  const windows = new Map();
  const localeListeners = new Set();
  globalThis.__jadixLocaleChanged = snapshot => {
    for (const callback of [...localeListeners]) if (localeListeners.has(callback)) callback(snapshot);
  };
  let sequence = 0;
  globalThis.__jadixDispatch = (key, value) => { callbacks.get(key)?.(value); };
  let pendingDesktop = 0;
  const desktop = request => {
    if (pendingDesktop >= 64) return Promise.reject(new RangeError('desktop request limit'));
    const key = 'desktop:' + String(++sequence);
    pendingDesktop++;
    return new Promise((resolve, reject) => {
      callbacks.set(key, result => {
        callbacks.delete(key);
        pendingDesktop--;
        if (Object.hasOwn(result, 'error')) reject(new Error(result.error));
        else resolve(result.value);
      });
      try { host({ ...request, key }); }
      catch (error) { callbacks.delete(key); pendingDesktop--; reject(error); }
    });
  };
  return {
    ...identity,
    persistence(save, restore) {
      if (typeof save !== 'function' || typeof restore !== 'function') throw new TypeError('persistence callbacks required');
      if (globalThis.__jadixCheckpoint) throw new Error('persistence already configured');
      globalThis.__jadixCheckpoint = () => {
        const json = JSON.stringify(save());
        if (typeof json !== 'string' || bytes(json) > 65536) throw new RangeError('checkpoint exceeds 64 KiB');
        return json;
      };
      globalThis.__jadixRestore = restore;
    },
    virtualRange(options) { return host({...options, type: 'virtual_range'}); },
    emojiEntry(query, emoji) { return host({type: 'emoji_entry', query, emoji}); },
    searchEmoji(query = {}) { return host({type: 'emoji_search', query}); },
    layoutEmoji(query = {}, columns = 8, headers) { return host({type: 'emoji_layout', query, columns, headers}); },
    locateEmoji(query, columns, emoji, headers) { return host({type: 'emoji_locate', query, columns, emoji, headers}); },
    navigateEmoji(query, columns, current, key, rtl = false, page_rows = 6, headers) { return host({type: 'emoji_navigate', query, columns, current, key, rtl, page_rows, headers}); },
    locale() { return host({type: 'i18n', request: {action: 'snapshot'}}); },
    onLocaleChange(callback) {
      if (typeof callback !== 'function') throw new TypeError('locale callback required');
      if (localeListeners.size >= 256) throw new RangeError('locale listener limit');
      localeListeners.add(callback);
      return () => localeListeners.delete(callback);
    },
    setLocale(locale, fallback, timezone) {
      return host({type: 'i18n', request: {action: 'configure', locale, fallback, timezone}});
    },
    resetLocale() { return host({type: 'i18n', request: {action: 'reset'}}); },
    availableLocales() { return host({type: 'i18n', request: {action: 'snapshot'}}).availableLocales; },
    t(key, args = {}) {
      if (typeof key !== 'string' || !args || typeof args !== 'object' || Array.isArray(args)) throw new TypeError('invalid translation');
      return host({type: 'i18n', request: {action: 'translate', key, args: Object.fromEntries(Object.entries(args).map(([key,value]) => [key,String(value)]))}});
    },
    formatNumber(value, digits) { return host({type: 'i18n', request: {action: 'number', value, digits}}); },
    formatPercent(value) { return host({type: 'i18n', request: {action: 'percent', value}}); },
    formatCompact(value) { return host({type: 'i18n', request: {action: 'compact', value}}); },
    formatCurrency(value, code) { return host({type: 'i18n', request: {action: 'currency', value, code}}); },
    formatDate(year, month, day, short = false) { return host({type: 'i18n', request: {action: 'date', year, month, day, short}}); },
    formatTime(hour, minute, second, hour12 = false) { return host({type: 'i18n', request: {action: 'time', hour, minute, second, hour12}}); },
    formatList(items) { return host({type: 'i18n', request: {action: 'list', items}}); },
    share(options) {
      if (!options || typeof options !== 'object' || Array.isArray(options) ||
          Object.keys(options).some(key => !['subject', 'text', 'url', 'files'].includes(key)) ||
          ['subject', 'text', 'url'].some(key => options[key] !== undefined && typeof options[key] !== 'string') ||
          (options.files !== undefined && (!Array.isArray(options.files) || options.files.length > 64 ||
            options.files.some(file => typeof file !== 'string')))) return Promise.reject(new TypeError('invalid share options'));
      return desktop({type: 'share', options});
    },
    openUrl(url) {
      if (typeof url !== 'string') return Promise.reject(new TypeError('URL must be a string'));
      return desktop({type: 'open_url', url});
    },
    files(request) {
      if (!request || typeof request !== 'object' || Array.isArray(request)) return Promise.reject(new TypeError('invalid file request'));
      return desktop({type: 'files', request});
    },
    clipboard(request) {
      if (!request || typeof request !== 'object' || Array.isArray(request)) return Promise.reject(new TypeError('invalid clipboard request'));
      return desktop({type: 'clipboard', request});
    },
    updater(request = {action: "snapshot"}) {
      if (!request || typeof request !== "object" || Array.isArray(request)) return Promise.reject(new TypeError("invalid updater request"));
      return desktop({type: "updater", request});
    },
    performance() { return desktop({type: 'performance'}); },
    deepLinks(request) { return desktop({type: 'deep_links', request}); },
    setAppAddress(address) { return desktop({type: 'deep_links', request: {action: 'address', address}}); },
    websocket(request) {
      if (!request || typeof request !== 'object' || Array.isArray(request)) return Promise.reject(new TypeError('invalid WebSocket request'));
      return desktop({type: 'web_socket', request});
    },
    fetch(request) {
      if (!request || typeof request !== 'object' || Array.isArray(request)) return Promise.reject(new TypeError('invalid HTTP request'));
      return desktop({type: 'fetch', request});
    },
    openAuthSession(request) {
      if (!request || typeof request !== 'object' || Array.isArray(request) ||
          typeof request.url !== 'string' || typeof request.callback_scheme !== 'string')
        return Promise.reject(new TypeError('invalid authentication session'));
      return desktop({type: 'auth_session', request: {action: 'open', request}});
    },
    closeAuthSession() { return desktop({type: 'auth_session', request: {action: 'close'}}); },
    haptic(feedback = {type: 'selection'}) {
      return desktop({type: 'haptic', feedback});
    },
    notifications(request) {
      if (!request || typeof request !== 'object' || Array.isArray(request)) return Promise.reject(new TypeError('invalid notification request'));
      return desktop({type: 'notifications', request});
    },
    globalShortcut(request) {
      if (!request || typeof request !== 'object' || Array.isArray(request)) return Promise.reject(new TypeError('invalid global shortcut request'));
      return desktop({type: 'global_shortcut', request});
    },
    speech(request) {
      if (!request || typeof request !== 'object' || Array.isArray(request)) return Promise.reject(new TypeError('invalid speech request'));
      return desktop({type: 'speech', request});
    },
    audio(request) {
      if (!request || typeof request !== 'object' || Array.isArray(request)) return Promise.reject(new TypeError('invalid audio request'));
      return desktop({type: 'audio', request});
    },
    setTray(tray) { return desktop({type: "set_tray", tray}); },
    trayEvents() { return desktop({type: "tray_events"}); },
    evaluateWeb(path, script) {
      if (!Array.isArray(path) || path.length > 32 || path.some(i => !Number.isSafeInteger(i) || i < 0) ||
          typeof script !== 'string' || bytes(script) > 1024 * 1024) return Promise.reject(new TypeError('invalid web evaluation'));
      return desktop({type: 'evaluate_web', path, script});
    },
    stopScroll(path) {
      if (!Array.isArray(path) || path.length > 32 || path.some(i => !Number.isSafeInteger(i) || i < 0))
        return Promise.reject(new TypeError('invalid scroll path'));
      return desktop({type: 'stop_scroll', path});
    },
    zoomTo(path, scale, anchor = [0, 0]) {
      if (!Array.isArray(path) || path.length > 32 || path.some(i => !Number.isSafeInteger(i) || i < 0) ||
          !Number.isFinite(scale) || scale <= 0 || !Array.isArray(anchor) || anchor.length !== 2 ||
          !anchor.every(Number.isFinite)) return Promise.reject(new TypeError('invalid viewport zoom'));
      return desktop({type: 'zoom_to', path, scale, anchor});
    },
    window(id, options, onChange) {
      if (typeof id !== 'string' || !id || bytes(id) > 128 ||
          (options !== null && (typeof options !== 'object' || Array.isArray(options))) ||
          (onChange !== undefined && typeof onChange !== 'function')) throw new TypeError('invalid window declaration');
      let entry = windows.get(id);
      if (!entry) {
        if (windows.size >= 64) throw new RangeError('window declaration limit');
        entry = { key: 'window:' + String(++sequence), state: null, options: null, onChange };
        windows.set(id, entry);
        callbacks.set(entry.key, value => {
          if (Object.hasOwn(value, 'state')) entry.state = value.state;
          entry.onChange?.(value);
        });
      }
      entry.onChange = onChange;
      const update = next => {
        host({ type: 'window', id, key: entry.key, options: next });
        entry.options = next;
      };
      update(options);
      return Object.freeze({
        get state() { return entry.state; },
        update,
        close() { update(null); },
        release() { update(null); callbacks.delete(entry.key); windows.delete(id); },
      });
    },
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
