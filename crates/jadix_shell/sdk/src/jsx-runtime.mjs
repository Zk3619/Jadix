import { Box } from './index.mjs';

export const Fragment = Symbol.for('jadix.fragment');
export function jsx(component, props = {}, key) {
  if (key !== undefined) {
    if (typeof key !== 'string' || !key || key.length > 256) throw new TypeError('invalid element key');
    let size = 0;
    for (const character of key) {
      const code = character.codePointAt(0);
      if (code >= 0xd800 && code <= 0xdfff) throw new TypeError('invalid element key');
      size += code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4;
    }
    if (size > 256) throw new TypeError('invalid element key');
  }
  const { children, ...attributes } = props;
  const normalized = children === undefined ? [] : Array.isArray(children) ? children : [children];
  if (component === Fragment) return { ...Box({ children: normalized }), ...(key === undefined ? {} : { key }) };
  if (typeof component !== 'function') throw new TypeError('unregistered JSX component');
  const element = component(children === undefined ? attributes : { ...attributes, children: normalized });
  return key === undefined ? element : { ...element, key };
}
export const jsxs = jsx;
