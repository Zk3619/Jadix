import {Box, Scroll} from './index.mjs';

/** Uniform row geometry is calculated by jadix_core. Only the mounted window
 * is rendered; row keys survive overlapping scroll windows. */
export function createVirtualList(app, {id, count, itemExtent, gap = 0, height = 260, overscan = 2, smoothScroll = false, menuScroll = false,
    renderItem, onChange, onRangeChange, onVisibleRangeChange} = {}) {
  if (typeof id !== 'string' || !id || typeof renderItem !== 'function' || typeof onChange !== 'function')
    throw new TypeError('virtual list id, renderItem and onChange required');
  let state = {count, extent: itemExtent, gap, offset: 0, viewport: height ?? itemExtent, overscan};
  let range = app.virtualRange(state), target, targetSmooth = smoothScroll, released = false, previousVisible;
  const live = () => { if (released) throw new Error('virtual list released'); };
  const calculate = (focus, align) => { range = app.virtualRange({...state, focus, align}); state.offset = range.offset; };
  const scroll = app.event('Scroll', 'scroll', event => {
    live();
    const viewport = event.viewport?.[1] ?? state.viewport;
    if (event.y === state.offset && viewport === state.viewport) return;
    state = {...state, offset: event.y, viewport}; target = undefined;
    calculate(); onChange();
  });
  return Object.freeze({
    get range() { return {...range}; },
    invalidateVisibleRange() { live(); previousVisible = undefined; },
    update(options = {}, notify = true) {
      live(); state = {...state, ...options}; calculate();
      target = [0, range.offset]; targetSmooth = false;
      if (notify) onChange();
    },
    scrollTo(index, notify = true, align = 'nearest', animated = smoothScroll) {
      live();
      const destination = app.virtualRange({...state, focus: index, align});
      target = [0, destination.offset]; targetSmooth = animated;
      if (!animated) { range = destination; state.offset = range.offset; }
      if (notify) onChange();
    },
    render(style = {}, accessibility = {}) {
      live(); onRangeChange?.({...range});
      const visible = {start: range.visible_start, end: range.visible_end};
      if (!previousVisible || previousVisible.start !== visible.start || previousVisible.end !== visible.end) {
        previousVisible = visible; onVisibleRangeChange?.({...visible});
      }
      const rows = [];
      for (let index = range.start; index < range.end; index++) {
        rows.push({...Box({style: {height: state.extent + (index + 1 < state.count ? state.gap : 0), shrink: 0}, children: [renderItem(index)]}), key: `${id}__row_${index}`});
      }
      const spacer = (name, height) => ({...Box({style: {height, shrink: 0}}), key: `${id}__${name}`});
      return {...Scroll({...accessibility, style: {...(height === null ? {grow: 1, min_height: 0} : {height}), ...(menuScroll ? {scrollbars: [false, false], scroll_bounces: false} : {}), ...style, scroll_behavior: (target ? targetSmooth : smoothScroll) ? 'smooth' : 'instant', ...(target ? {scroll_to: target} : {})}, events: {scroll},
        children: [Box({style: {axis: 'column', shrink: 0}, children: [
          spacer('before', range.before), ...rows,
          spacer('after', range.after),
        ]})]}), key: id};
    },
    release() { if (!released) { released = true; scroll.release(); } },
  });
}
export const VirtualList = ({list, style}) => list.render(style);
