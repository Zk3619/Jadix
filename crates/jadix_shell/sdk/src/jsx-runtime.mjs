import { Box } from './index.mjs';
import { bytes } from './utf8.mjs';

export const Fragment = Symbol.for('jadix.fragment');
export function jsx(component, props = {}, key) {
  if (key !== undefined) {
    if (typeof key !== 'string' || !key || key.length > 256) throw new TypeError('invalid element key');
    if (bytes(key, 'invalid element key') > 256) throw new TypeError('invalid element key');
  }
  const { children, ...attributes } = props;
  const normalized = children === undefined ? [] : Array.isArray(children) ? children : [children];
  if (component === Fragment) return { ...Box({ children: normalized }), ...(key === undefined ? {} : { key }) };
  if (typeof component !== 'function') throw new TypeError('unregistered JSX component');
  const element = component(children === undefined ? attributes : { ...attributes, children: normalized });
  return key === undefined ? element : { ...element, key };
}
export const jsxs = jsx;
