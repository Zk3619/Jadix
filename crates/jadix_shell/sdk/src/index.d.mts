import type { ComponentProperties, ComponentEvents } from './generated.mjs';
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
export interface Handle { session: string; sequence: string }
export interface EventHandle<Payload> extends Handle { readonly __payload?: Payload }
export interface StateHandle<Value> extends Handle { readonly __state?: Value }
/** UTF-8 byte ranges, sorted/nonoverlapping; at most 64 per Text node. */
export interface InlineStyle {
  range: [number, number];
  foreground?: [number, number, number, number]; background?: [number, number, number, number];
  bold?: boolean; italic?: boolean; underline?: boolean; strikethrough?: boolean;
}
export interface NodeStyle {
  text_overflow?: "wrap" | "clip" | "ellipsis";
  max_lines?: number;
  inline_styles?: InlineStyle[];
  width?: number; height?: number; min_width?: number; min_height?: number;
  max_width?: number; max_height?: number; axis?: 'row' | 'column'; gap?: number;
  padding?: [number, number, number, number]; grow?: number; shrink?: number;
  foreground?: [number, number, number, number]; background?: [number, number, number, number];
  font_family?: string; font_size?: number; font_weight?: number;
}
export interface ElementSpec { component: string; key?: string; props: object; style: NodeStyle; events: object; children: ElementSpec[] }
export type Props<Name extends keyof ComponentProperties> = ComponentProperties[Name] & { style?: NodeStyle; events?: ComponentEvents[Name] };
export declare const protocol: 1;
export declare const sdkVersion: '0.1.0';
export declare function createElement<Name extends keyof ComponentProperties>(name: Name, props: Props<Name>, children?: ElementSpec[]): ElementSpec;
export declare function Box(props: Props<'Box'> & { children?: ElementSpec[] }): ElementSpec;
export declare function Text(props: Props<'Text'>): ElementSpec;
export declare function Button(props: Props<'Button'>): ElementSpec;
export declare function Input(props: Props<'Input'>): ElementSpec;
export declare function Textarea(props: Props<'Textarea'>): ElementSpec;
export declare function Checkbox(props: Props<'Checkbox'>): ElementSpec;
export declare function Switch(props: Props<'Switch'>): ElementSpec;
export declare function Select(props: Props<'Select'>): ElementSpec;
export declare function Image(props: Props<'Image'>): ElementSpec;
export declare function DialogContent(props: Props<'DialogContent'> & { children?: ElementSpec[] }): ElementSpec;
export declare function PopoverContent(props: Props<'PopoverContent'> & { children?: ElementSpec[] }): ElementSpec;
export declare function Scroll(props: Props<'Scroll'> & { children?: ElementSpec[] }): ElementSpec;
export interface Envelope { protocol: 1; session: string; command: object }
export interface Client {
  mount(root: string, tree: ElementSpec): Promise<unknown>;
  writeState<Value extends Json>(handle: StateHandle<Value>, value: Value): Promise<unknown>;
  event<Payload extends Json>(handle: EventHandle<Payload>, payload: Payload): Promise<unknown>;
  call<Result = Json>(method: string, args: Json): { request: string; result: Promise<Result>; cancel(): Promise<unknown> };
}
export declare function createClient(transport: (envelope: Envelope) => unknown | Promise<unknown>, session: string): Client;
export interface ScriptApplication {
  readonly session: string; readonly root: string;
  event<Name extends keyof ComponentEvents, Event extends keyof ComponentEvents[Name]>(component: Name, event: Event, callback: (payload: NonNullable<ComponentEvents[Name][Event]> extends EventHandle<infer P> ? P : never) => void): NonNullable<ComponentEvents[Name][Event]> & { release(): void };
  mount(tree: ElementSpec): void;
  /** Replace props on mounted Text/Button/Input/Textarea/Checkbox/Switch/Select leaves, in ordered publication. No structure/style/event changes. */
  patch(patches: { path: number[]; props: ComponentProperties['Text'] | ComponentProperties['Button'] | ComponentProperties['Input'] | ComponentProperties['Textarea'] | ComponentProperties['Checkbox'] | ComponentProperties['Switch'] | ComponentProperties['Select'] }[]): void;
}
export declare function createApplication(): ScriptApplication;
