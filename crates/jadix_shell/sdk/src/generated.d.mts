export interface ComponentProperties {
  Box: {  };
  Text: { "text": string };
  Button: { "label": string; "disabled"?: boolean };
  Input: { "value": string; "placeholder"?: string; "disabled"?: boolean };
  Scroll: {  };
  PopoverContent: { "anchor": string; "open": boolean; "placement"?: string; "gap"?: number; "close_on_outside_click"?: boolean };
  Textarea: { "value": string; "placeholder"?: string; "disabled"?: boolean };
  Checkbox: { "label": string; "checked": boolean; "disabled"?: boolean };
  Switch: { "label": string; "checked": boolean; "disabled"?: boolean };
  Image: { "src": string; "alt"?: string };
  DialogContent: { "open": boolean; "close_on_overlay_click"?: boolean };
  Select: { "options": ReadonlyArray<{ "value": string; "label": string; "enabled": boolean }>; "value": string; "placeholder"?: string; "disabled"?: boolean };
}
export interface ComponentEvents {
  Box: {  };
  Text: {  };
  Button: { "activate"?: EventHandle<null> };
  Input: { "edit"?: EventHandle<{ "value": string; "revision": string; "selection": ReadonlyArray<number>; "composing"?: ReadonlyArray<number> }> };
  Scroll: { "scroll"?: EventHandle<{ "x": number; "y": number }> };
  PopoverContent: { "dismiss"?: EventHandle<{ "backdrop": boolean; "revision": string }> };
  Textarea: { "edit"?: EventHandle<{ "value": string; "revision": string; "selection": ReadonlyArray<number>; "composing"?: ReadonlyArray<number> }> };
  Checkbox: { "toggle"?: EventHandle<{ "checked": boolean; "revision": string }> };
  Switch: { "toggle"?: EventHandle<{ "checked": boolean; "revision": string }> };
  Image: {  };
  DialogContent: { "dismiss"?: EventHandle<{ "backdrop": boolean; "revision": string }> };
  Select: { "select"?: EventHandle<{ "value": string; "revision": string }> };
}
import type { EventHandle } from './index.mjs';
