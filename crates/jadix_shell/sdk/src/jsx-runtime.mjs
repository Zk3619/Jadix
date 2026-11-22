import { Box } from './index.mjs';
import { bytes } from './utf8.mjs';

export const Fragment = Symbol.for('jadix.fragment');
function flatten(children, result = [], depth = 0, budget = { nodes: 0 }) {
  if (++budget.nodes > 4096 || depth > 32) throw new RangeError('children limit');
  if (Array.isArray(children)) {
    for (const child of children) flatten(child, result, depth + 1, budget);
  } else result.push(children);
  return result;
}
export function jsx(component, props = {}, key) {
  if (key !== undefined) {
    if (typeof key !== 'string' || !key || key.length > 256) throw new TypeError('invalid element key');
    if (bytes(key, 'invalid element key') > 256) throw new TypeError('invalid element key');
  }
  const { children, ...attributes } = props;
  const normalized = children === undefined ? [] : flatten(children);
  if (component === Fragment) return { ...Box({ children: normalized }), ...(key === undefined ? {} : { key }) };
  if (typeof component !== 'function') throw new TypeError('unregistered JSX component');
  const element = component(children === undefined ? attributes : { ...attributes, children: normalized });
  return key === undefined ? element : { ...element, key };
}
export const jsxs = jsx;
