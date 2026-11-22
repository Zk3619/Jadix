import { Box, Button, Input, Select, Text } from './index.mjs';
import {createVirtualList} from './virtual-list.mjs';

const categories = ['Smileys & emotion', 'People & body', 'Components', 'Animals & nature', 'Food & drink', 'Travel & places', 'Activities', 'Objects', 'Symbols', 'Flags', 'Recently used'];
const tones = ['Default skin tone', 'Light skin tone', 'Medium-light skin tone', 'Medium skin tone', 'Medium-dark skin tone', 'Dark skin tone'];
const option = (value, label) => ({value, label, enabled: true});
const panelKeys = ['up', 'down', 'left', 'right', 'home', 'end', 'pageUp', 'pageDown', 'enter', 'escape'];
const navigationKeys = query => ['up', 'down', 'pageUp', 'pageDown', 'enter', 'escape', ...(query ? [] : ['left', 'right'])];

/** Common controls compose the picker; jadix_core owns search, sections and
 * navigation. Release the controller with its containing application tree. */
export function createEmojiPicker(app, {id, query = {}, columns = 8, rowHeight = 40, itemSize = 36, height = null, rtl = false, disabled = false,
    controlledQuery = false, searchable = true, showFilters = false, placeholder = 'Search emoji', groupLabels = categories, toneLabels = tones, header, footer, empty,
    autoFocus = false, accessibilityLabel = 'Emoji picker', searchAccessibilityLabel, searchPrefix, searchSuffix,
    parts = {}, renderItem, renderGroup, onSelect, onSelectEntry, onHighlightChange, onQueryChange, onGroupChange,
    onCancel, onChange} = {}) {
  if (typeof id !== 'string' || !id || id.length > 128 || typeof onChange !== 'function' ||
      !Number.isInteger(columns) || columns < 1 || columns > 64 || groupLabels.length > 11 || toneLabels.length !== 6)
    throw new TypeError('picker id, columns, group labels and onChange required');
  if (![rowHeight, itemSize, ...(height === null ? [] : [height])].every(value => Number.isFinite(value) && value > 0) || itemSize > rowHeight)
    throw new TypeError('picker dimensions must be positive and itemSize must fit rowHeight');
  groupLabels = Array.from({length: 11}, (_, id) => groupLabels[id] ?? '');
  const headers = groupLabels.flatMap((label, id) => renderGroup || label ? [id] : []);
  let state = {query: '', group: null, tone: null, ...query, offset: 0, limit: 128};
  let layout = app.layoutEmoji(state, columns, headers), highlighted = null, highlightedEntry = null, composing = false;
  if (!disabled && state.query.trim() && layout.total) {
    highlighted = 0; highlightedEntry = app.searchEmoji({...state, offset: 0, limit: 1}).items[0];
    onHighlightChange?.({...highlightedEntry});
  }
  let visibleGroup = null, pointerPosition = null;
  let released = false, list, focusRequest = autoFocus && !disabled ? 1 : 0;
  const bindings = [], cells = new Map();
  const live = () => { if (released) throw new Error('emoji picker released'); };
  const customize = (name, element) => parts[name]?.(element) ?? element;
  const keyed = (key, element) => ({...element, key: `${id}__${key}`});
  const bind = (component, event, callback) => {
    const handle = app.event(component, event, value => { live(); if (!disabled) callback(value); });
    bindings.push(handle); return handle;
  };
  const choose = entry => { if (entry && !disabled) { onSelect?.(entry.text); onSelectEntry?.({...entry}); } };
  const clearCells = () => { for (const cell of cells.values()) { cell.handle.release(); cell.pointerMove.release(); } cells.clear(); };
  const setHighlight = result => {
    if (highlighted !== result.index) {
      highlighted = result.index; highlightedEntry = result.entry;
      onHighlightChange?.(result.entry ? {...result.entry} : null);
    }
    if (result.row !== null) list.scrollTo(result.row, false);
  };
  const update = changes => {
    live();
    const previous = state;
    const next = {...state, ...changes, offset: 0, limit: 128};
    const result = app.layoutEmoji(next, columns, headers);
    state = next; layout = result; clearCells();
    list?.invalidateVisibleRange();
    const previousEntry = highlightedEntry;
    const queryChanged = previous.query !== state.query;
    let retained = !queryChanged && previousEntry && !disabled
      ? app.locateEmoji(state, columns, previousEntry.text, headers) : null;
    if (retained?.index === null && previousEntry.base !== previousEntry.text) retained = app.locateEmoji(state, columns, previousEntry.base, headers);
    highlighted = queryChanged ? (!disabled && state.query.trim() && layout.total ? 0 : null) : (retained?.index ?? null);
    highlightedEntry = highlighted === null ? null : retained?.entry ?? app.searchEmoji({...state, offset: 0, limit: 1}).items[0];
    if (JSON.stringify(previousEntry) !== JSON.stringify(highlightedEntry)) onHighlightChange?.(highlightedEntry ? {...highlightedEntry} : null);
    list.update({count: layout.rows.length, ...(queryChanged ? {offset: 0} : {})}, false);
    if (retained?.row !== null && retained?.row !== undefined) list.scrollTo(retained.row, false, 'nearest', false);
    if (!controlledQuery && previous.query !== state.query) onQueryChange?.(state.query);
    onChange({...state});
  };
  const proposeQuery = value => {
    if (controlledQuery) {
      onQueryChange?.(value);
      // Reproject the authoritative value even when the parent rejects it.
      onChange({...state});
    } else update({query: value});
  };
  const move = key => {
    if (key === 'escape') { if (state.query) proposeQuery(''); else onCancel?.(); return; }
    if (key === 'enter') { choose(highlightedEntry); return; }
    setHighlight(app.navigateEmoji(state, columns, highlighted, key, rtl, Math.max(1, list.range.visible_end - list.range.visible_start), headers));
    onChange({...state});
  };
  const navigate = event => {
    if (composing || event.shift || event.control || !(searchable ? navigationKeys(state.query) : panelKeys).includes(event.key)) return;
    move(event.key);
  };
  let search, navigation, group, tone;
  try {
    if (searchable) search = bind('Input', 'edit', event => { composing = !!event.composing; if (!composing) proposeQuery(event.value); });
    navigation = bind(searchable ? 'Input' : 'Button', 'navigate', navigate);
    if (showFilters) group = bind('Select', 'select', event => update({group: event.value === 'all' ? null : Number(event.value)}));
    if (showFilters) tone = bind('Select', 'select', event => update({tone: event.value === 'default' ? null : Number(event.value)}));
    list = createVirtualList(app, {id: `${id}__scroll`, count: layout.rows.length, itemExtent: rowHeight, height, smoothScroll: true,
      onChange: () => onChange({...state}),
      onVisibleRangeChange: range => {
        if (state.query.trim()) return;
        const group = layout.sections[range.start] ?? null;
        if (group !== visibleGroup) { visibleGroup = group; onGroupChange?.(group); }
      },
      onRangeChange: range => {
        const mounted = new Set();
        for (let row = range.start; row < range.end; row++) {
          const descriptor = layout.rows[row];
          if (descriptor.kind === 'items') for (let i = descriptor.value.start; i < descriptor.value.end; i++) mounted.add(i);
        }
        for (const [index, cell] of cells) if (!mounted.has(index)) { cell.handle.release(); cell.pointerMove.release(); cells.delete(index); }
      },
      renderItem: row => {
        const descriptor = layout.rows[row];
        if (descriptor.kind === 'group') {
          const heading = renderGroup?.({id: descriptor.value, label: groupLabels[descriptor.value], first: row === 0}) ?? Text({text: groupLabels[descriptor.value]});
          return customize('group', {...heading, props: {...heading.props, accessibleRole: 'heading', accessibleName: groupLabels[descriptor.value]}});
        }
        const {start, end} = descriptor.value;
        const page = app.searchEmoji({...state, offset: start, limit: end - start});
        return customize('row', Box({style: {axis: 'row', gap: 4}, children: Array.from({length: columns}, (_, column) => {
          const entry = page.items[column];
          if (!entry) return keyed(`empty_${row}_${column}`, Box({style: {width: 0, height: itemSize, grow: 1}}));
          const index = start + column;
          let cell = cells.get(index);
          if (!cell) {
            cell = {entry, content: renderItem?.({...entry}), handle: app.event('Button', 'activate', () => { live(); choose(cell.entry); }),
              pointerMove: app.event('Button', 'pointerMove', event => {
                live(); if (disabled || !Number.isFinite(event.x) || !Number.isFinite(event.y)) return;
                if (pointerPosition?.[0] === event.x && pointerPosition?.[1] === event.y) return;
                pointerPosition = [event.x, event.y];
                if (highlighted === index) return;
                highlighted = index; highlightedEntry = cell.entry;
                onHighlightChange?.({...cell.entry}); onChange({...state});
              })};
            cells.set(index, cell);
          }
          const props = {label: entry.text, disabled, accessibleRole: 'option', accessibleName: entry.name, accessibleSelected: index === highlighted, style: {width: 0, height: itemSize, grow: 1,
            background: index === highlighted ? [80, 130, 220, 80] : [0, 0, 0, 0]}, events: {activate: cell.handle, pointerMove: cell.pointerMove}};
          const content = cell.content;
          return keyed(`emoji_${index}`, customize('item', Button({...props, ...(content ? {children: [content]} : {})})));
        })}));
      },
    });
  } catch (error) { list?.release(); clearCells(); for (const handle of bindings) handle.release(); throw error; }
  return Object.freeze({
    get query() { return {...state}; },
    matchedCount() { live(); return layout.total; },
    entry(emoji) { live(); return app.emojiEntry(state, emoji); },
    update,
    focus() { live(); if (!disabled) { focusRequest++; onChange({...state}); } },
    highlight(emoji) {
      live(); if (disabled) return false;
      const result = app.locateEmoji(state, columns, emoji, headers);
      if (result.index === null) return false;
      setHighlight(result); onChange({...state}); return true;
    },
    navigate(key) { live(); if (!panelKeys.includes(key)) throw new TypeError('invalid navigation key'); if (!disabled && !composing) move(key); },
    scrollToGroup(group, animated = true) {
      live(); if (disabled) return false; const row = state.query.trim() ? -1 : layout.sections.indexOf(group);
      if (row >= 0) { list.scrollTo(row, false, 'start', animated); onChange({...state}); } return row >= 0;
    },
    render(style = {}) {
      live(); const order = state.groups ?? [10, 0, 1, 3, 4, 5, 6, 7, 8, 9];
      const focus = {focusRequest, accessibleControls: [`${id}__scroll`], ...(highlighted === null ? {} : {accessibleActiveDescendant: `${id}__emoji_${highlighted}`}), accessibleName: searchAccessibilityLabel ?? accessibilityLabel, ...(highlightedEntry ? {accessibleDescription: highlightedEntry.name} : {})};
      return keyed('root', Box({accessibleName: accessibilityLabel, style: {axis: 'column', gap: 4, ...style}, children: [
        ...(header ? [header] : []),
        ...(searchable ? [keyed('search_bar', customize('searchBar', Box({style: {axis: 'row', gap: 4}, children: [
          ...(searchPrefix ? [searchPrefix] : []),
          keyed('search', customize('search', Input({value: state.query, placeholder, maxLength: 256, disabled, ...focus,
            navigationKeys: navigationKeys(state.query), events: {edit: search, navigate: navigation}}))),
          ...(searchSuffix ? [searchSuffix] : []),
        ]})))] : [keyed('keyboard', Button({label: accessibilityLabel, disabled, ...focus,
          navigationKeys: panelKeys, events: {navigate: navigation}}))]),
        ...(showFilters ? [Box({style: {axis: 'row', gap: 4}, children: [
          keyed('categories', Select({accessibleName: 'Category', value: state.group === null ? 'all' : String(state.group), disabled, options: [option('all', 'All categories'), ...order.map(index => option(String(index), groupLabels[index]))], events: {select: group}})),
          keyed('tones', Select({accessibleName: 'Skin tone', value: state.tone === null ? 'default' : String(state.tone), disabled, options: toneLabels.map((name, index) => option(index ? String(index) : 'default', name)), events: {select: tone}})),
        ]})] : []),
        ...(layout.total ? [customize('list', list.render({}, {accessibleRole: 'listbox', accessibleName: accessibilityLabel}))] : [empty ?? Text({text: 'No emoji found'})]),
        ...(footer ? [footer] : []),
      ]}));
    },
    release() { if (!released) { released = true; list.release(); clearCells(); for (const handle of bindings) handle.release(); } },
  });
}
export const EmojiPicker = ({picker, style}) => picker.render(style);
