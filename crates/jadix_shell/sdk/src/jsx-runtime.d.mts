import type { ElementSpec } from './index.mjs';
export declare const Fragment: unique symbol;
export declare function jsx(component: ((props: never) => ElementSpec) | typeof Fragment, props: object, key?: string): ElementSpec;
export declare const jsxs: typeof jsx;
export namespace JSX {
  type Element = ElementSpec;
  interface IntrinsicAttributes { key?: string }
  interface ElementChildrenAttribute { children: unknown }
}
