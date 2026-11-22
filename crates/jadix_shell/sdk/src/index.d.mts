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
export type RGBA = [number, number, number, number];
export interface TextAppearance {
  selectable?: boolean;
  italic?: boolean; line_height?: number; line_height_multiple?: number; letter_spacing?: number;
  align?: 'start' | 'end' | 'left' | 'right' | 'center' | 'justify';
  direction?: 'auto' | 'ltr' | 'rtl'; vertical_align?: 'top' | 'center' | 'bottom'; vertical_align_fraction?: number;
  transform?: 'none' | 'uppercase' | 'lowercase' | 'capitalize';
  variants?: Array<'normal' | 'small_caps' | 'oldstyle_numbers' | 'lining_numbers' | 'tabular_numbers' | 'proportional_numbers'>;
  underline?: boolean; overline?: boolean; strikethrough?: boolean;
  decoration_style?: 'solid' | 'double' | 'dotted' | 'dashed' | 'wavy';
  decoration_color?: RGBA; decoration_thickness?: number; underline_offset?: number;
  /** Text shadows have zero spread and cannot be inset. */
  shadow?: ViewShadow; placeholder_color?: RGBA; selection_color?: RGBA; caret_color?: RGBA;
}
export interface ViewShadow { offset?: [number, number]; blur?: number; spread?: number; color?: RGBA; inset?: boolean }
export interface ViewAppearance {
  opacity?: number;
  /** Column-major homogeneous 4 x 4 matrix. */
  transform?: [number, number, number, number, number, number, number, number, number, number, number, number, number, number, number, number];
  background?: RGBA; tint?: RGBA;
  /** Top-left, top-right, bottom-right, bottom-left. */
  corners?: [number, number, number, number];
  /** Top, right, bottom, left. */
  border_widths?: [number, number, number, number]; border_colors?: [RGBA, RGBA, RGBA, RGBA];
  outline_width?: number; outline_offset?: number; outline_color?: RGBA;
  shadows?: ViewShadow[]; clip?: boolean;
  continuous_corners?: boolean; border_style?: StrokeStyle; outline_style?: StrokeStyle;
  origin?: { fraction?: [number, number]; offset?: [number, number]; z?: number };
  backface_hidden?: boolean; hidden?: boolean; cursor?: ViewCursor; accent?: RGBA;
  blend_mode?: BlendMode; backgrounds?: ViewGradient[]; masks?: ViewGradient[];
  filters?: ViewFilter[]; backdrop_filters?: ViewFilter[];
  backdrop_progressive?: { start: number; end: number; min_mix: number };
  backdrop?: { material?: BackdropMaterial; blending?: 'within_window' | 'behind_window'; state?: 'active' | 'inactive' | 'follows_window_active_state' };
  glass?: 'none' | 'regular' | 'clear'; glass_tint?: RGBA; control?: 'native' | 'consistent' | 'plain';
  transition?: ViewTransition;
  animation?: ViewAnimation;
}
/** Milliseconds. Properties: colors=1, opacity=2, transform=4, shadow=8, shape=16, all=31. */
export interface ViewTransition {
  properties?: number; duration?: number; delay?: number; easing?: [number, number, number, number];
  spring?: { stiffness?: number; damping?: number; mass?: number };
}
export interface ViewAnimation {
  keyframes?: 'spin' | 'ping' | 'pulse' | 'bounce'; duration?: number; delay?: number;
  easing?: [number, number, number, number]; iterations?: number | null; frame_rate?: number;
}
export type StrokeStyle = 'solid' | 'dashed' | 'dotted';
export type ViewCursor = 'default' | 'auto' | 'pointer' | 'text' | 'crosshair' | 'grab' | 'grabbing' | 'wait' | 'progress' | 'help' | 'move' | 'zoom_in' | 'zoom_out' | 'not_allowed' | 'resize_horizontal' | 'resize_vertical' | 'resize_north_east_south_west' | 'resize_north_west_south_east' | 'none';
export type BlendMode = 'normal' | 'multiply' | 'screen' | 'overlay' | 'darken' | 'lighten' | 'color_dodge' | 'color_burn' | 'hard_light' | 'soft_light' | 'difference' | 'exclusion' | 'hue' | 'saturation' | 'color' | 'luminosity' | 'plus_darker' | 'plus_lighter';
export type BackdropMaterial = 'popover' | 'titlebar' | 'selection' | 'menu' | 'sidebar' | 'header_view' | 'sheet' | 'window_background' | 'hud_window' | 'full_screen_ui' | 'tool_tip' | 'content_background' | 'under_window_background' | 'under_page_background';
export interface ViewGradient {
  geometry: { kind: 'linear'; angle: number } | { kind: 'linear_corner'; right: boolean; bottom: boolean } |
    { kind: 'radial'; circle: boolean; size: 'closest_side' | 'closest_corner' | 'farthest_side' | 'farthest_corner'; center: [number, number] } |
    { kind: 'conic'; angle: number; center: [number, number] };
  stops: { color: RGBA; position?: { fraction: number } | { points: number } }[];
  repeating?: boolean;
}
export type ViewFilter = { kind: 'blur' | 'brightness' | 'contrast' | 'grayscale' | 'hue_rotate' | 'invert' | 'opacity' | 'saturate' | 'sepia'; value: number } | { kind: 'drop_shadow'; value: ViewShadow };
/** AppKit panel style at the start of entry or the end of exit. */
export interface PopoverAppearance {
  view?: ViewAppearance;
  opacity?: number;
  scale?: number;
  offset?: [number, number];
  background?: [number, number, number, number];
  /** Clipping radius in logical points. */
  corner_radius?: number;
}
export interface NodeStyle {
  text?: TextAppearance;
  scroll_to?: [number, number];
  scroll_behavior?: 'instant' | 'smooth';
  view?: ViewAppearance;
  text_overflow?: "wrap" | "clip" | "ellipsis" | "ellipsis_start" | "ellipsis_middle";
  max_lines?: number;
  inline_styles?: InlineStyle[];
  /** AppKit popover transition durations in milliseconds. */
  popover_enter_ms?: number;
  popover_exit_ms?: number;
  popover_starting?: PopoverAppearance;
  popover_ending?: PopoverAppearance;
  image_fit?: 'contain' | 'cover' | 'fill' | 'none' | 'scale_down';
  /** X and Y fractions of the remaining space, plus point offsets from the top left. */
  image_position?: string | { fraction: [number, number]; offset: [number, number] };
  image_placeholder?: string;
  image_fade_ms?: number;
  image_blur_radius?: number;
  width?: number; height?: number; min_width?: number; min_height?: number;
  max_width?: number; max_height?: number; axis?: 'row' | 'column'; gap?: number;
  align_items?: 'start' | 'end' | 'center' | 'stretch' | 'baseline';
  padding?: [number, number, number, number]; grow?: number; shrink?: number;
  /** Horizontal and vertical native scroller visibility. */
  scrollbars?: [boolean, boolean];
  /** Native indicator insets in logical points: top, right, bottom, left. */
  scrollbar_inset?: [number, number, number, number];
  /** AppKit scroll elasticity. Omit to use the system default. */
  scroll_bounces?: boolean;
  scroll_refresh?: { refreshing?: boolean; threshold?: number; inset?: number }
  scroll_snap?: { paging?: boolean; interval?: number; offsets?: number[]; alignment?: 'start' | 'center' | 'end'; deceleration?: number; one_step?: boolean }
  scroll_enabled?: boolean
  keyboard_taps?: 'never' | 'always' | 'handled';
  keyboard_dismiss_mode?: 'none' | 'on_drag' | 'interactive';
  scroll_zoom?: { min?: number; max?: number; initial?: number; gestures?: boolean };
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
export declare function Button(props: Props<'Button'> & { children?: ElementSpec[] }): ElementSpec;
export declare function Input(props: Props<'Input'>): ElementSpec;
export declare function Textarea(props: Props<'Textarea'>): ElementSpec;
export declare function Checkbox(props: Props<'Checkbox'>): ElementSpec;
export declare function Switch(props: Props<'Switch'>): ElementSpec;
export declare function Select(props: Props<'Select'>): ElementSpec;
export declare function Image(props: Props<'Image'>): ElementSpec;
export declare function DialogContent(props: Props<'DialogContent'> & { children?: ElementSpec[] }): ElementSpec;
export declare function PopoverContent(props: Props<'PopoverContent'> & { children?: ElementSpec[] }): ElementSpec;
export declare function Scroll(props: Props<'Scroll'> & { children?: ElementSpec[] }): ElementSpec;
export interface CivilDate { year: number; month: number; day: number }
export interface TimeOfDay { hour: number; minute: number; second?: number }
export type ShaderUniform =
  {type: 'float' | 'int' | 'uint'; value: number} |
  {type: 'vec2'; value: [number, number]} |
  {type: 'vec3'; value: [number, number, number]} |
  {type: 'vec4'; value: [number, number, number, number]} |
  {type: 'mat4'; value: [number, number, number, number, number, number, number, number, number, number, number, number, number, number, number, number]} |
  {type: 'vec4_array'; value: [number, number, number, number][]};
export type HostedContent =
  {type: 'custom'; kind: string; properties: {[key: string]: Json}} |
  {type: 'date'; value: CivilDate | null; minimum: CivilDate | null; maximum: CivilDate | null; mode: 'calendar' | 'field'; locale: string | null; enabled: boolean} |
  {type: 'time'; value: TimeOfDay; seconds: boolean; enabled: boolean} |
  {type: 'color'; rgba: RGBA; alpha: boolean; enabled: boolean} |
  {type: 'web'; url: string | null; html: string | null; base_url: string | null; origins: string[]} |
  {type: 'media'; source: string; audio: boolean; paused: boolean; looping: boolean; controls: boolean; muted: boolean;
    progress_interval_ms: number; volume: number; rate: number; seek: number | null; seek_revision: number; fit: 'contain' | 'cover' | 'fill'} |
  {type: 'canvas'; commands: ({type: 'fill'; rect: [number, number, number, number]; color: RGBA} |
    {type: 'stroke'; points: [number, number][]; color: RGBA; width: number; closed: boolean} |
    {type: 'text'; text: string; position: [number, number]; size: number; color: RGBA})[]} |
  {type: 'shader'; language?: 'glsl' | 'wgsl'; source: string; function: string; uniforms: number[]; frames_per_second: number;
    paused?: boolean; animated?: boolean; time_seconds?: number | null;
    parameters?: Record<string, ShaderUniform>; textures?: Record<string, string>;
    composition?: {type: 'background'} | {type: 'effect'} | {type: 'transition'; value: string; duration_ms?: number; fallback?: 'fade' | 'none'};
    progress?: number | null; max_sample_offset?: number} |
  {type: 'menu_button'; label: string; enabled: boolean; items: HostedMenuItem[]} |
  {type: 'tab_bar'; items: HostedTab[]; selected: string | null; enabled: boolean; hidden: boolean} |
  {type: 'segments'; items: {id: string; label: string; enabled: boolean; symbol?: string | null; accessibility_label?: string | null}[]; selected: string | null; enabled: boolean};
export type HostedProps = Omit<Props<'Hosted'>, 'specification'>;
export interface EmojiQuery { query?: string; group?: number | null; tone?: number | null; offset?: number; limit?: number; groups?: number[] | null; recent?: string[]; annotations?: {emoji: string; label: string; keywords?: string[]}[]; emoji_version?: string | null }
export interface EmojiEntry {text: string; base: string; name: string; group: number; section: number; version: string; has_skin_tones: boolean; skin_tone: number | null; keywords: string[]}
export interface EmojiPage {total: number; items: EmojiEntry[]}
export type NavigationKey = 'up' | 'down' | 'left' | 'right' | 'home' | 'end' | 'pageUp' | 'pageDown' | 'enter' | 'escape' | 'tab';
export type EmojiRow = {kind: 'group'; value: number} | {kind: 'items'; value: {start: number; end: number}};
export interface EmojiLayout {total: number; rows: EmojiRow[]; sections: (number | null)[]}
export interface VirtualRangeOptions {count: number; extent: number; gap?: number; offset: number; viewport: number; overscan?: number; focus?: number; align?: 'nearest' | 'start' | 'center' | 'end'}
export interface VirtualRange {start: number; end: number; visible_start: number; visible_end: number; offset: number; total: number; before: number; after: number}
export interface VirtualListController {
  readonly range: VirtualRange;
  invalidateVisibleRange(): void;
  update(options?: Partial<VirtualRangeOptions>, notify?: boolean): void;
  scrollTo(index: number, notify?: boolean, align?: 'nearest' | 'start' | 'center' | 'end', animated?: boolean): void;
  render(style?: NodeStyle, accessibility?: Pick<ComponentProperties['Scroll'], 'accessibleName' | 'accessibleDescription' | 'accessibleRole' | 'accessibleActiveDescendant' | 'accessibleControls'>): ElementSpec;
  release(): void;
}
export declare function createVirtualList(app: ScriptApplication, options: {
  id: string; count: number; itemExtent: number; gap?: number; height?: number | null; overscan?: number; smoothScroll?: boolean; menuScroll?: boolean;
  renderItem: (index: number) => ElementSpec; onChange: () => void; onRangeChange?: (range: VirtualRange) => void; onVisibleRangeChange?: (range: {start: number; end: number}) => void;
}): VirtualListController;
export declare function VirtualList(props: {list: VirtualListController; style?: NodeStyle}): ElementSpec;
export interface EmojiPickerController {
  readonly query: EmojiQuery;
  update(query: EmojiQuery): void;
  navigate(key: NavigationKey): void;
  matchedCount(): number;
  entry(emoji: string): EmojiEntry | null;
  focus(): void;
  highlight(emoji: string): boolean;
  scrollToGroup(group: number, animated?: boolean): boolean;
  render(style?: NodeStyle): ElementSpec;
  release(): void;
}
export type EmojiPickerParts = Partial<Record<'searchBar' | 'search' | 'list' | 'row' | 'item' | 'group', (element: ElementSpec) => ElementSpec>>;
export declare function createEmojiPicker(app: ScriptApplication, options: {
  id: string; query?: EmojiQuery; controlledQuery?: boolean; columns?: number; rowHeight?: number; itemSize?: number; height?: number; rtl?: boolean; disabled?: boolean; searchable?: boolean; showFilters?: boolean;
  autoFocus?: boolean; accessibilityLabel?: string; searchAccessibilityLabel?: string; searchPrefix?: ElementSpec; searchSuffix?: ElementSpec;
  placeholder?: string; groupLabels?: string[]; toneLabels?: string[]; header?: ElementSpec; footer?: ElementSpec; empty?: ElementSpec;
  parts?: EmojiPickerParts;
  renderItem?: (entry: EmojiEntry) => ElementSpec; renderGroup?: (group: {id: number; label: string; first: boolean}) => ElementSpec;
  onSelect?: (value: string) => void; onSelectEntry?: (entry: EmojiEntry) => void;
  onHighlightChange?: (entry: EmojiEntry | null) => void; onQueryChange?: (query: string) => void;
  onGroupChange?: (group: number | null) => void; onCancel?: () => void;
  onChange: (query: EmojiQuery) => void;
}): EmojiPickerController;
export declare function EmojiPicker(props: {picker: EmojiPickerController; style?: NodeStyle}): ElementSpec;
export declare function Hosted(props: HostedProps & {content: HostedContent; children?: ElementSpec[]}): ElementSpec;
export declare function MenuButton(props: HostedProps & {label: string; items: HostedMenuItem[]; disabled?: boolean; children?: ElementSpec[]}): ElementSpec;
export interface HostedTab {id: string; label: string; enabled: boolean; symbol?: string | null; accessibility_label?: string | null; badge?: string | null}
export declare function TabBar(props: HostedProps & {items: HostedTab[]; selected?: string | null; hidden?: boolean; disabled?: boolean}): ElementSpec;
export declare function Embed(props: HostedProps & {kind: string; properties?: {[key: string]: Json}}): ElementSpec;
export interface DatePickerProps extends HostedProps {
  value?: CivilDate | null; minimum?: CivilDate | null; maximum?: CivilDate | null; locale?: string | null; disabled?: boolean;
}
export declare function Calendar(props: DatePickerProps): ElementSpec;
export declare function DatePicker(props: DatePickerProps): ElementSpec;
export declare function TimeField(props: HostedProps & {value?: TimeOfDay; seconds?: boolean; disabled?: boolean}): ElementSpec;
export declare function ColorPicker(props: HostedProps & {value?: RGBA; alpha?: boolean; disabled?: boolean}): ElementSpec;
export interface Envelope { protocol: 1; session: string; command: object }
export interface Client {
  mount(root: string, tree: ElementSpec): Promise<unknown>;
  writeState<Value extends Json>(handle: StateHandle<Value>, value: Value): Promise<unknown>;
  event<Payload extends Json>(handle: EventHandle<Payload>, payload: Payload): Promise<unknown>;
  call<Result = Json>(method: string, args: Json): { request: string; result: Promise<Result>; cancel(): Promise<unknown> };
}
export declare function createClient(transport: (envelope: Envelope) => unknown | Promise<unknown>, session: string): Client;
/** Opaque session-scoped file capability issued by trusted host code. */
export type ShareFileHandle = string & { readonly __shareFile: unique symbol };
export interface SelectedFile {handle: ShareFileHandle; name: string | null; directory: boolean}
export interface FilePanelOptions {content_types?: string[]; directory?: ShareFileHandle; title?: string; prompt?: string}
export type FilePickerRequest = {operation: 'pick_paths'; selection?: 'files' | 'folders' | 'files_and_folders'; multiple?: boolean; options?: FilePanelOptions} |
  {operation: 'pick_save_path'; file_name?: string; options?: FilePanelOptions};
export interface ClipboardItem {mime: string; bytes: number[]}
export type ClipboardRequest = {operation: 'read'; mime_types: string[]} | {operation: 'write'; items: ClipboardItem[]} | {operation: 'clear' | 'formats'};
export type ClipboardResponse = {type: 'item'; value: ClipboardItem | null} | {type: 'formats'; value: string[]} | {type: 'complete'};
export interface ShareOptions { subject?: string; text?: string; url?: string; files?: ShareFileHandle[] }
export type ShareOutcome = {status: 'shared'; service: string} | {status: 'dismissed' | 'replaced'};
export interface LocaleSnapshot {
  locale: string; fallback: string; timezone: string | null;
  direction: 'ltr' | 'rtl'; availableLocales: string[]; generation: string;
}
export type AudioSource = {type: 'url'; url: string} | {type: 'file'; path: ShareFileHandle} | {type: 'bytes'; base64: string};
export interface AudioSettings {volume: number; rate: number; muted: boolean; looping: boolean}
export interface AudioSnapshot {ready: boolean; playing: boolean; buffering: boolean; ended: boolean; position: number; duration: number | null; buffered: number | null; error: string | null}
export type AudioEvent = {type: 'loaded'; duration: number; decoded_bytes: number} | {type: 'load'; duration: number | null} |
  {type: 'progress'; position: number; duration: number | null; buffered: number | null} | {type: 'playing' | 'paused' | 'ended'} |
  {type: 'buffer'; buffering: boolean} | {type: 'failed'; message: string};
export type AudioRequest = {action: 'load'; source: AudioSource} | {action: 'open'; source: AudioSource; settings?: AudioSettings} |
  {action: 'play'; id: number; volume?: number; rate?: number} | {action: 'stop' | 'preload' | 'resume' | 'pause' | 'release' | 'snapshot' | 'events'; id: number} |
  {action: 'seek'; id: number; position: number} | {action: 'set'; id: number; settings: AudioSettings};
export interface SpeechOptions {lang?: string; interim?: boolean}
export type SpeechError = 'not-allowed' | 'no-speech' | 'audio-capture' | 'network' | 'language-not-supported' | 'unavailable' | 'failed';
export type SpeechEvent = {type: 'start'} | {type: 'result'; text: string; is_final: boolean} | {type: 'end'; error: SpeechError | null};
export type SpeechRequest = {action: 'start'; request?: SpeechOptions} | {action: 'stop' | 'abort' | 'events' | 'release'; id: number};
export interface NotificationOptions { title: string; subtitle?: string; body?: string; sound?: boolean; data?: string }
export interface GlobalShortcutOptions { key: string; command?: boolean; control?: boolean; option?: boolean; shift?: boolean }
export type Haptic = {type: 'selection'} | {type: 'impact'; style: 'light' | 'medium' | 'heavy' | 'soft' | 'rigid'} |
  {type: 'notification'; kind: 'success' | 'warning' | 'error'};
export type DesktopServiceError = 'unavailable' | 'denied' | 'conflict' | 'failed';
export interface AuthSessionOptions {url: string; callback_scheme: string; ephemeral?: boolean}
export type AuthSessionReply = {type: 'auth_session'; outcome: {status: 'completed'; url: string} | {status: 'cancelled'}; error: null} |
  {type: 'auth_session'; outcome: null; error: DesktopServiceError};
export type NotificationAuthorization = 'not_determined' | 'denied' | 'authorized' | 'provisional' | 'ephemeral' | 'unknown';
export type DesktopServiceReply = {type: 'complete'; error: DesktopServiceError | null} |
  {type: 'permission'; authorization: NotificationAuthorization; error: DesktopServiceError | null};
export type DesktopServiceEvent = {type: 'click' | 'dismiss' | 'shortcut'};
export interface NotificationActivation {identifier: string; data: string}
export type NotificationRequest = {action: 'permission'; request?: boolean} | {action: 'notify'; notification: NotificationOptions} |
  {action: 'badge'; label: string | null} | {action: 'reply' | 'events' | 'release'; id: number} | {action: 'activations'};
export type GlobalShortcutRequest = {action: 'register'; shortcut: GlobalShortcutOptions} | {action: 'reply' | 'events' | 'release'; id: number};
export interface PerformanceStat {meanMs: number; p95Ms: number; maxMs: number}
export interface PerformanceSnapshot {
  fps: number; refreshRate: number | null; frame: PerformanceStat; frameInterval: PerformanceStat; tickInterval: PerformanceStat;
  js: PerformanceStat; core: PerformanceStat; commitsPerSecond: number; dropped: number; framesMs: number[];
  process: {process_id: number; sample_seconds: number; ui_percent: number | null; process_percent: number | null;
    logical_cpus: number; working_set: number | null; private_bytes: number | null; gpu_percent: number | null;
    gpu_dedicated: number | null; gpu_shared: number | null};
  heap: {allocatedBytes: number; objects: number} | null; liveNodes: number; retiredNodes: number;
}
export interface FetchOptions {
  url: string; method?: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE' | 'HEAD' | 'OPTIONS';
  headers?: [string, string][]; body?: number[]; timeout_ms?: number; max_response_bytes?: number;
}
export type FetchEvent = {type: 'headers'; status: number; url: string; headers: [string, string][]} |
  {type: 'chunk'; bytes: number[]} | {type: 'complete' | 'cancelled'} | {type: 'failed'; message: string};
export interface ApplicationUpdate {
  id: string; version: string; notes: string | null; localizedNotes: Record<string, {url: string; sha256: string}>;
  url: string; sha256: string | null; size: number | null; canInstall: boolean;
}
export interface UpdaterSnapshot {
  generation: number; phase: 'disabled' | 'idle' | 'checking' | 'notes' | 'available' | 'downloading' | 'staging' | 'ready' | 'installing' | 'scheduled' | 'failed';
  available: ApplicationUpdate | null; received: number; error: string | null;
}
export type UpdaterRequest = {action: 'check'; channel?: string; locale?: string} |
  {action: 'snapshot' | 'download' | 'cancel'} | {action: 'install'; restart?: boolean};
export interface ScriptApplication {
  files(request: FilePickerRequest): Promise<SelectedFile[]>;
  files(request: {operation: 'open' | 'reveal' | 'trash' | 'release'; handle: ShareFileHandle}): Promise<void>;
  clipboard(request: ClipboardRequest): Promise<ClipboardResponse>;
  updater(request?: UpdaterRequest): Promise<UpdaterSnapshot>;
  virtualRange(options: VirtualRangeOptions): VirtualRange;
  searchEmoji(query?: EmojiQuery): EmojiPage;
  emojiEntry(query: EmojiQuery, emoji: string): EmojiEntry | null;
  layoutEmoji(query?: EmojiQuery, columns?: number, headers?: number[]): EmojiLayout;
  locateEmoji(query: EmojiQuery, columns: number, emoji: string, headers?: number[]): {index: number | null; row: number | null; entry: EmojiEntry | null};
  navigateEmoji(query: EmojiQuery, columns: number, current: number | null, key: NavigationKey, rtl?: boolean, page_rows?: number, headers?: number[]): {index: number | null; row: number | null; entry: EmojiEntry | null};
  websocket(request: {action: 'connect'; request: {url: string; headers?: [string, string][]; protocols?: string[]; timeout_ms?: number}}): Promise<number>;
  websocket(request: {action: 'send'; id: number; message: {type: 'text'; data: string} | {type: 'binary'; data: number[]}}): Promise<void>;
  websocket(request: {action: 'events'; id: number}): Promise<Array<{type: 'connected'; protocol: string | null} | {type: 'message'; message: {type: 'text'; data: string} | {type: 'binary'; data: number[]}} | {type: 'closed'; code: number | null; reason: string} | {type: 'failed'; message: string}>>;
  websocket(request: {action: 'close'; id: number; code: number; reason: string} | {action: 'release'; id: number}): Promise<void>;
  fetch(request: {action: 'send'; request: FetchOptions}): Promise<number>;
  fetch(request: {action: 'events'; id: number}): Promise<FetchEvent[]>;
  fetch(request: {action: 'cancel' | 'release'; id: number}): Promise<void>;
  deepLinks(request: {action: 'configure'; config: {schemes: string[]; single_instance: boolean; queue_limit: number}}): Promise<null>;
  deepLinks(request: {action: 'events'}): Promise<Array<{url: string; source: 'cold_start' | 'hot_start' | 'handoff'}>>;
  deepLinks(request: {action: 'activation'; config: {identifier: string; arguments?: string[]; file_extensions?: string[]; deep_links?: {schemes: string[]; single_instance: boolean; queue_limit: number}}}): Promise<boolean>;
  deepLinks(request: {action: 'launches'}): Promise<Array<{arguments: string[]; working_directory: string | null}>>;
  deepLinks(request: {action: 'address'; address: string | null}): Promise<null>;
  setAppAddress(address: string | null): Promise<null>;
  performance(): Promise<PerformanceSnapshot>;
  openAuthSession(request: AuthSessionOptions): Promise<AuthSessionReply>;
  closeAuthSession(): Promise<void>;
  haptic(feedback?: Haptic): Promise<DesktopServiceReply>;
  notifications(request: Extract<NotificationRequest, {action: 'permission' | 'notify' | 'badge'}>): Promise<number>;
  notifications(request: {action: 'reply'; id: number}): Promise<DesktopServiceReply | null>;
  notifications(request: {action: 'events'; id: number}): Promise<DesktopServiceEvent[]>;
  notifications(request: {action: 'activations'}): Promise<NotificationActivation[]>;
  notifications(request: NotificationRequest): Promise<number | void | DesktopServiceReply | null | DesktopServiceEvent[] | NotificationActivation[]>;
  globalShortcut(request: {action: 'register'; shortcut: GlobalShortcutOptions}): Promise<number>;
  globalShortcut(request: {action: 'reply'; id: number}): Promise<DesktopServiceReply | null>;
  globalShortcut(request: {action: 'events'; id: number}): Promise<DesktopServiceEvent[]>;
  globalShortcut(request: GlobalShortcutRequest): Promise<number | void | DesktopServiceReply | null | DesktopServiceEvent[]>;
  speech(request: {action: 'start'; request?: SpeechOptions}): Promise<number>;
  speech(request: {action: 'events'; id: number}): Promise<SpeechEvent[]>;
  speech(request: SpeechRequest): Promise<void | number | SpeechEvent[]>;
  audio(request: Extract<AudioRequest, {action: 'load' | 'open'}>): Promise<number>;
  audio(request: {action: 'snapshot'; id: number}): Promise<AudioSnapshot>;
  audio(request: {action: 'events'; id: number}): Promise<AudioEvent[]>;
  audio(request: AudioRequest): Promise<void | number | AudioSnapshot | AudioEvent[]>;
  locale(): LocaleSnapshot;
  onLocaleChange(callback: (snapshot: LocaleSnapshot) => void): () => void;
  setLocale(locale?: string, fallback?: string, timezone?: string): LocaleSnapshot;
  resetLocale(): LocaleSnapshot;
  availableLocales(): string[];
  t(key: string, args?: Record<string, string | number | boolean>): string;
  formatNumber(value: number, digits?: number): string;
  formatPercent(value: number): string;
  formatCompact(value: number): string;
  formatCurrency(value: number, code: string): string;
  formatDate(year: number, month: number, day: number, short?: boolean): string;
  formatTime(hour: number, minute: number, second?: number, hour12?: boolean): string;
  formatList(items: string[]): string;
  setTray(tray: Tray | null): Promise<void>;
  trayEvents(): Promise<string[]>;
  evaluateWeb(path: number[], script: string): Promise<Json>;
  stopScroll(path: number[]): Promise<void>;
  zoomTo(path: number[], scale: number, anchor?: [number, number]): Promise<void>;
  /** Requires desktop.share. Paths are refused; files must be host-granted handles. */
  share(options: ShareOptions): Promise<ShareOutcome>;
  /** Requires desktop.open_url or desktop.open_url.<scheme>. The host router runs first. */
  openUrl(url: string): Promise<boolean>;
  readonly session: string; readonly root: string;
  /** Application-owned JSON only, at most 64 KiB UTF-8; register once per runtime. */
  persistence(save: () => Json, restore: (value: Json) => void): void;
  /** Requires the desktop.windows grant in signed packages. Stable IDs update
   * the same child root; null removes the declaration. Root disposal closes it. */
  window(id: string, options: WindowOptions | null, onChange?: (event: WindowEvent) => void): WindowDeclaration;
  event<Name extends keyof ComponentEvents, Event extends keyof ComponentEvents[Name]>(component: Name, event: Event, callback: (payload: NonNullable<ComponentEvents[Name][Event]> extends EventHandle<infer P> ? P : never) => void): NonNullable<ComponentEvents[Name][Event]> & { release(): void };
  mount(tree: ElementSpec): void;
  /** Replace props on mounted Text/Button/Input/Textarea/Checkbox/Switch/Select leaves, in ordered publication. No structure/style/event changes. */
  patch(patches: { path: number[]; props: ComponentProperties['Text'] | ComponentProperties['Button'] | ComponentProperties['Input'] | ComponentProperties['Textarea'] | ComponentProperties['Checkbox'] | ComponentProperties['Switch'] | ComponentProperties['Select'] }[]): void;
}
export declare function createApplication(): ScriptApplication;

export type WindowMode = 'normal' | 'maximized' | 'minimized' | 'full_screen';
export interface WindowAttributes {
  title?: string; resizable?: boolean; close_on_press?: boolean; close_on_escape?: boolean;
  close_on_blur?: boolean; dismiss_on_close?: boolean;
  level?: 'normal' | 'floating' | 'above_dock' | 'desktop_icon';
  workspaces?: 'current' | 'all'; participates_in_cycle?: boolean;
  screen_orientations?: 'landscape_and_portrait' | 'landscape' | 'portrait';
  appearance?: 'inherit' | 'light' | 'dark'; title_bar?: 'standard' | 'transparent' | 'hidden';
  content_size?: {width: number; height: number}; min_content_size?: {width: number; height: number};
  max_content_size?: {width: number; height: number}; position?: {x: number; y: number};
  mode?: WindowMode; visible?: boolean;
}
export interface WindowOptions {
  kind?: 'normal' | 'dialog' | 'panel' | 'popup' | 'overlay'; width?: number; height?: number;
  modal?: boolean; initial_mode?: WindowMode; initially_visible?: boolean;
  attributes?: WindowAttributes; content?: {factory: string; argument?: string};
}
export interface WindowFrame {x: number; y: number; width: number; height: number}
export interface WindowState {
  id: string | null; frame: WindowFrame; normal_frame: WindowFrame | null;
  content_width: number; content_height: number; backing_scale: number;
  display: {id: number | null; frame: WindowFrame; work_area: WindowFrame} | null;
  title_bar_insets: {top: number; left: number; right: number};
  mode: WindowMode; key: boolean; visible: boolean; occluded: boolean;
}
export interface WindowEvent {id: string; state?: WindowState | null; closed?: boolean; error?: string}
export interface WindowDeclaration {
  readonly state: WindowState | null;
  update(options: WindowOptions | null): void;
  close(): void;
  release(): void;
}

export interface TrayTitleRun { text: string; color?: [number, number, number, number] | null }
export interface HostedMenuItem { id: string; label: string; enabled: boolean; checked: boolean; children?: HostedMenuItem[]; symbol?: string | null; accelerator?: string | null; accessibility_label?: string | null; destructive?: boolean; inline?: boolean }
export interface Tray { title?: TrayTitleRun[]; placement?: "left" | "right"; tooltip?: string; symbol?: string | null; menu?: HostedMenuItem[] }
