# jadix_native

Opt into `consistent-ui` and call `NativeRoot::present_consistent(&source, viewport, &environment, &theme)` to project shared `ConsistentTheme` colors, typography, spacing and minimum control sizes before the same native layout. Authoring snapshots remain unchanged, so repeated font scaling does not accumulate. Explicit nondefault style values are retained; values equal to defaults receive theme tokens, including separate enabled and disabled control/text colors. On iOS, the native-controls build also accepts `--consistent-ui` to opt into a localized system Done keyboard accessory for native text inputs and text areas. Use `present` to retain the original native styling. This is a shared appearance policy over real OS controls, not a claim of identical platform pixels or a custom renderer. Projection clones the tree and costs O(N). Native focus, IME, selection, undo, accessibility, high contrast and system control chrome remain owned by the host.

With `ohos`, `ohos_environment::OhosConfiguration::from_json(bytes, lifecycle)` converts actual ArkUI Configuration callbacks into validated shared environment snapshots. The companion [observer](platforms/ohos/environment.ets) registers application environment callbacks, publishes the initial context configuration and returns an idempotent unsubscriber. Feed snapshots into a root-local EnvironmentHost, then republish the same source with present_consistent. Keep the lifecycle supplied by the UIAbility/page; configuration does not infer foreground state. Unknown color modes remain None; malformed locale/font scaling is rejected. The observer is an explicit integration helper, not automatic OHOS system_snapshot sampling.

The miniprogram module adapts the shared NativeRoot, Taffy layout and revisioned protocol 2 to WXWebAssembly and native WeChat components. The [Rust sample and build instructions](../../examples/miniprogram/README.md) cover twelve node kinds, controlled Unicode events, themes, lifecycle, dialogs and popovers. Full visual, device, IME and advanced style coverage has separate limits.


This package adapts shared Jadix presentation contracts to system views. The primary UI entry is `jadix`; optional `jadix_shell` consumes this adapter for script applications. Shared contracts live in `jadix_core` / `jadix_layout`, without a GPU or VM dependency.

On macOS 14+, feature `appkit` exposes `jadix_native::appkit::AppKitHost`.
Integrate `platforms/macos/JadixNative.h` as the Swift bridging header and mount
`JadixNativeView` inside the application's window. Compile
`platforms/macos/JadixTextField.m` with Objective-C ARC and link it into the same
host target. The adapter supplies system
controls, in-window Dialog and Popover, and AppKit row and column accessibility
elements for mounted grid cells. Popover uses the anchor's layout direction and
chooses the side with more room when the preferred side is too small. AppKit
honors `PopoverContent`'s `margin` in whole points around the window safe area
and supports independent Escape and outside-click dismissal.
Portable Rust builders and native RSX expose `on_open_change` / `onOpenChange`
for trigger, outside-press, and Escape requests. Choose `controlled_open` /
`controlledOpen` to apply requests explicitly in the callback, or
`default_open` / `defaultOpen` to initialize local state. The existing
`open` / `openSignal` signal updates automatically on those requests.
Direct writes to the open signal do not emit a request. The
adapter reveals focused views in nested scroll containers, reports native
scroll offsets through the shared `EventPayload::Scroll` contract, and
implements host `scroll_to` for mounted scroll views. Scroll notifications
are coalesced on the AppKit main queue and ignore unchanged offsets. Shared
`VirtualList` already mounts its visible rows plus overscan, and its optional
`scroll_ahead(true)` predicts up to one viewport during continuous scrolling.
Portable `Scroll` elements can hide both scrollers with `scrollbars(false)`
and disable elasticity with `scroll_bounces(false)`. Shell uses
`style.scrollbars` in horizontal and vertical order and `style.scroll_bounces`.
AppKit uses the system elasticity default when `scroll_bounces` is omitted.
AppKit applies each mounted-node snapshot and preserves `NSView` identity while
the node remains mounted. A bounded pool also reuses up to 64 detached static
text labels and a separate pool holds up to 64 plain Box containers. Controls,
editors, image views and semantically marked containers are released when their
nodes leave the snapshot. AppKit does not select a host-specific prefetch
policy, so nsh's urgent-span prebinding remains open. The adapter
also uses Core Text font cascades for labels and applies Unicode scalar
limits to user edits in plain, secure, and multiline fields. Single-paragraph,
single-line clipped labels preserve the right edge when the paragraph runs
right to left. Use `.flex_row().items(AlignItems::Baseline)` in portable
Rust or `style.align_items: 'baseline'` on a Shell row to align text labels.
AppKit reports the first baseline of the measured label at its fitted size.
AppKit decodes embedded PNGs, local and HTTPS images on a serial
ImageIO queue at the view's backing pixel size. Pass `asset:/images/sample.jpg`
as the image source for a file under the main app bundle's `Contents/Resources`,
or pass a local `file:///` or remote `https://` URL. With an automatic axis, the first successful
decode supplies the oriented intrinsic size for layout. Set both dimensions in
the node style to reserve space while the image loads. AppKit uses the system
Reduce Motion setting for fade-in and sequence playback. It shares decoded
frames for bundle resources and embedded PNGs, and merges simultaneous HTTPS
requests for the same URL. AppKit uses the shared `ImageFit` and `ImagePosition`
styles for static images and animations. Images are contained and centered by
default. Use `.object_fit(ImageFit::Cover)` with
`.object_position(ImagePosition::new([1.0, 0.0], [-8.0, 0.0]))` to align the
fitted image at the top right with an 8-point inset. Native RSX exposes
`objectFit` and `objectPosition`. `ImagePosition::parse("right 8px top 0")`
gives the same position. The shell SDK accepts the position string or the
numeric object in `style.image_position`, alongside `style.image_fit`.
AppKit shows `style.image_placeholder` while the main image loads and keeps it
if the main image fails. The placeholder uses the same source formats and
resource limits as the main image. `style.image_fade_ms` sets the fade duration
for loaded images. A value of `0` disables the fade.
AppKit also supports base64 data URLs for JPEG, GIF, WebP, BMP, TIFF, ICO and
ICNS through the `inline_raster_sources` capability. Additional inline formats
take their intrinsic size from ImageIO after decoding.
The AppKit host also accepts static UTF-8 SVG from base64 data URLs, local
files, bundle assets and HTTPS, for both the main image and the placeholder.
The `inline_svg_sources` capability advertises data URL support. The host
rasterizes at the backing pixel scale and uses the SVG viewport for intrinsic
size. SVG is limited to 1 MiB and supports self-contained vector artwork and
system fonts.
Portable Rust exposes the main image lifecycle through `on_load_event` on
`ImageBuilder` and `onLoadEvent` in native RSX. The shell SDK exposes
`loadStart`, `progress`, `load`, `error` and `loadEnd` event handles. Remote
progress reports cumulative bytes as `loaded` and the expected byte count as
`total`, with zero for unknown totals. A successful load reports the oriented
original dimensions in pixels. Changes to the backing scale or target size
reuse the current source lifecycle.

The macOS host can restrict HTTPS images by declaring `JadixRemoteImageHosts`
as an array of strings in the main app's `Info.plist`. Each entry matches a
complete hostname without regard to case. AppKit checks the main image,
placeholder, and each HTTP redirect. An empty array blocks remote images.
Omitting the key allows any HTTPS host. This application policy is the upper
bound for every root. A host can narrow one root before mounting.

```swift
try nativeView.restrictRemoteImages(to: JadixRemoteImagePolicy(
    hosts: ["cdn.example.com"],
    pathPrefixes: ["cdn.example.com": ["/images"]]
))
try nativeView.mount()
```

Per-root transfer queues, redirect checks and memory-only HTTP caches use the
policy for the lifetime of that mount. Closing the root invalidates its session.

`JadixRemoteImagePathPrefixes` optionally maps hostnames to arrays of
absolute path prefixes, for example `{"cdn.example.com": ["/images"]}`.
Prefixes match complete path segments. A trailing slash is optional and `/`
allows every valid path on that host. Rules ignore the query and match
path bytes after one percent-decoding pass. Ambiguous path segments are
rejected. A configured map permits only HTTPS port 443 and is intersected
with `JadixRemoteImageHosts` when both keys are set. The same rules apply
to the placeholder, redirects and the final response URL.

The [native-controls example](../../examples/native-controls/README.md)
includes a macOS host with independent roots in resizable windows, the system
Edit menu and a local build command.
The example honors the opt-in `AppKitWindowAttributes::dismiss_on_close`
policy for secondary windows. User close, Escape, and blur can hide the
window and preserve its root. A subsequent show by ID reuses the root.

AppKit labels align clipped and ellipsized paragraphs to their writing start.
Core Text wraps the leading lines to the label width. At the last line allowed
by the height and line limit, `clip` uses glyph positions to keep the original
paragraph's start edge visible. `ellipsis`, `ellipsis_start`, and
`ellipsis_middle` place the ellipsis at the end, start, or middle of the
remaining text. Measurement and drawing use the same line layout. The label
keeps its inline styles and exposes the original text to accessibility.

Use the portable builder's `window_drag(true)` or RSX `windowDrag = true`
to make a view a window drag region. A child can set the property to false to
exclude itself and its subtree. Native interactive controls retain their input
handling, and double-click uses the system title bar preference.
The title bar strip also serves as a drag region when the host hides its title
and extends the content under a transparent title bar.

Swift hosts can call `JadixNativeView.pickPaths` or `pickSavePath` on a mounted
root. The panel opens as a sheet in that root's visible window or as a separate
window if the owner is hidden, minimized, or off-screen. Selection supports
files, folders, or both, type filters, and multiple URLs. The save panel accepts
a suggested filename. Both accept a starting directory and a confirmation
button label. Closing or moving the root cancels its panel.

Rust applications can use `AppKitHost::request_file` with `AppKitFileRequest`
to show those panels, open a path, reveal it in Finder, or move it to the Trash.
Poll the result receiver with `try_recv` on the UI thread. Cancelling a panel
returns an empty successful result. Disposing the root completes outstanding
requests with an error.

`AppKitHost::show_alert` presents an AppKit system alert as a sheet in the
root's visible window. The Swift host can use `preferredSheetWindow` to choose
the key, titled window in its window group for alerts and file panels. When no
eligible window is available, standalone alerts are presented one at a time in
request order. Both presentations use the owner's effective appearance or the
root's appearance when no owner is available.
`AppKitAlert` sets the title, message, severity, button labels, Return action,
Escape action and optional text field. The field supports an initial value,
placeholder and accessibility label. On initial focus, `select_text_on_focus`
selects the initial value. Otherwise the caret starts at the end. A required
empty field disables actions whose button does not handle Escape. The request
returns an ID and a result receiver. The reply
carries the pressed button's zero-based index and the text field value, if
present. `close_alert` dismisses a visible or queued alert by request ID and
yields a reply with no button. AppKit queues standard sheets and applies its
native presentation rules for critical sheets. The root's disposal completes
pending alert requests with an error.

`AppKitHost::show_share` opens the AppKit share picker for text, an absolute
non-file URL, or local files with absolute paths. `AppKitShare` also accepts a
subject and an anchor node in the current root. If the anchor is unavailable,
the picker opens at the center of the root in a visible window. AppKit presents
the picker on a later run-loop turn and delivers the items to the user's chosen
service. The receiver reports the service title on success. It also reports
dismissal, replacement by a newer request, or an error. For services that
omit completion callbacks, the host observes return to the originating window.
Poll the receiver with `try_recv` on the UI thread.

`AppKitHost::show_context_menu` accepts a mounted node, shared `MenuItem`
values, and a locale. It presents an AppKit menu anchored to the visible node.
The receiver yields the selected command ID after its callback runs, or
`Ok(None)` when the menu is dismissed. Submenus, separators, disabled items,
checked and radio selection states, localized role captions, and supported
key equivalents use the shared menu model. This menu executes the supplied
command callback.

`AppKitHost::set_window_menu` installs a window's application menu from the
same model and locale. The key window supplies the active menu, and
`clear_window_menu` restores the embedding application's menu when that root
owns the menu bar. The optional zero-based submenu index designates AppKit's
Window menu. Standard roles use the AppKit responder chain. Custom commands
resolve to Rust callbacks for the installed menu revision.
Use `MenuItem::with_icon(MenuIcon { source, template })` on a command or
submenu. AppKit uses the native image URI syntax and root HTTPS policy. It
fits the first frame into a 16-point square and uses template tinting when
requested. Menu image source strings have a combined 1 MiB UTF-8 limit.

`NativeRoot::set_app_address` records the address for the current root and
forwards changes to AppKit. Swift hosts can set or clear it with
`JadixNativeView.setAppAddress`. Set `handoffEnabled` to opt in to Handoff.
While the root's window is key, HTTP and HTTPS addresses become browsing
activities. Other absolute addresses with a custom scheme use `<bundle id>.url`
and the `url` user-info key. Relative and file addresses stay in the root.
The root withdraws its activity on focus loss and invalidates it on replacement
or disposal.

The app's Info.plist declares `<bundle id>.url` in `NSUserActivityTypes` and
its custom URL schemes in `CFBundleURLTypes`. Forward AppKit's activity
continuation to `continueUserActivity` and ordinary URL delivery to `openURL`.
Both paths call `onOpenURL` for a declared custom scheme. The callback reports
whether the app handled the URL.

`AppKitHost::request_window` submits an open, update, mode change, focus,
visibility change, or close request by string ID.
The Swift app installs `JadixNativeView.onWindowRequest` to choose the window and
its Rust root. Open requests carry an initial title, content size and window
kind and can set `initial_mode` and `initially_visible` for a new window.
An initially hidden window mounts its root and applies any pending mode when it
first shows. Open and update requests accept `AppKitWindowAttributes` for title,
resizability, standard or transparent title bars, hidden titles, and content
size, size limits, and screen position in points. Updates preserve omitted attributes. Swift hosts can
apply the decoded attributes to their `NSWindow` with `Attributes.apply(to:)`.
For an open window, retain an `AppKitWindowBinding` for its ID and pass the
latest application-owned attributes to `sync_attributes` on the UI thread.
Drive it again after AppKit processes requests while it reports `Ok(false)`.
It sends only changed fields and records them when AppKit acknowledges them.
`observed_state` reads the actual state from the matching window's root host.
`bind_state` mirrors those observations to a Rust signal and clears the value
when the root detaches or is disposed. Use `set_mode`, `set_visible`, and
`restore` to submit requests from UI actions. The signal updates when AppKit
reports the window state. When the root host is stored in a `RefCell`, wrap the
borrow and the state update in a reactivity `batch` so effects run after release.
Use `AppKitHost::window_declarations` to obtain a handle for Rust components.
Call `mount` once per stable ID from the component's reactive owner. The options
closure can read signals. Return `Some(options)` to declare the window and
`None` to remove it. Owner cleanup also removes the declaration. AppKit replies
drive the next update or close automatically. Read request failures with
`try_recv` on the returned receiver. Kind, initial size, mode, visibility, and
modality apply at creation. To reopen a child the user closed, remove and re-add
its declaration.

For callers managing their own lifecycle, `AppKitDeclaredWindow::sync` takes
the latest options by reference. Call it again after AppKit replies while it
returns `Ok(false)`.

The window host chooses the child's content and reports actual destruction to
the declaring root. The native-controls host closes declared children when
their declaring window closes.
The native-controls example mounts an independent Rust root for each new
ID and raises an existing ID. It supports resizable normal windows, dialogs
centered over a parent, floating panels, borderless popups across Spaces, and
transparent overlays with pointer pass-through. Normal windows support
restore, maximize, minimize and system full screen. The reply acknowledges
the request. AppKit can finish a full-screen change on a later run loop turn.
`SetVisibility` orders the existing window in or out while retaining its root.
Dialogs and panels default to the requesting window as their parent. A visible
dialog opened with `modal: Some(true)` blocks input and user-initiated closing
in its parent window. Closing a parent closes its owned windows, including
hidden windows.
Dialogs and popups close on Escape when there is no marked text. Popups close
on blur by default. The Swift view uses `jadix_native_create_with_services` in
the Rust bridge. It passes file, window, alert and share request callbacks and
uses the three-slot measurement ABI. The host delivers results through
`jadix_native_file_result`, `jadix_native_window_result`,
`jadix_native_alert_result` and `jadix_native_share_result`. The mounted
view registers context menus through `jadix_native_enable_context_menus` and
delivers selection through `jadix_native_context_menu_result`. Requests run
on the main thread after the Rust call returns.
The view delivers application menu commands through
`jadix_native_menu_command`. Resolve the command with
`AppKitHost::menu_command` and release the root borrow before invoking it.

`AppKitHost::window_state` returns the latest observation of the window
containing that root. `set_window_state_handler` subscribes to changes in frame,
content size, backing scale, window mode, key focus, visibility and occlusion.
The observation includes `normal_frame` once the host has seen a normal window
frame. The standalone host captures that frame before the zoom animation.
Embedded hosts can forward `windowShouldZoom(_:toFrame:)` to the view's
`prepareForWindowZoom()` after allowing the zoom.
An explicit `Focus` request to the standalone host activates the application
when the target is a key-capable normal or dialog window.
When AppKit reports a screen for the window, the state includes its display ID
when available, full frame, and current work area. The window and display frames
use AppKit screen coordinates in points. The Swift view combines
pending AppKit notifications on the main queue and submits the current state to
`jadix_native_window_state`. The Rust host skips duplicate observations and
delivers `None` after detachment or disposal if it previously held a state.

Swift hosts can observe Popover changes with `JadixNativeView.onPopoverEvent`.
The callback gets the node ID and reports when the panel opens or closes,
Escape or outside-press dismissal attempts, and changes to the resolved position.
Positions use the flipped root coordinate system. An open Popover without a
visible anchor has no position. Callbacks run on the main queue after the
current native update returns.

AppKit also sends dismissal attempts to the portable Rust builder
(`on_cancel`), native RSX (`onCancel`) and Shell (`PopoverContent.events.cancel`).
Escape and outside presses reach these callbacks even when the corresponding
dismissal policy keeps the panel open. The versioned dismissal request follows
only if the same panel is still open and dismissal is allowed.
AppKit reports the resolved frame, placement and available size to
`on_position_change`, `onPositionChange` and Shell's `positionChange` event.
AppKit publishes the first open when a mounted panel has a visible anchor.
Rust and Shell receive open and close while the panel node is mounted. The
portable builder uses `on_open` and `on_close`, and native RSX uses
`onOpen` and `onClose`. Swift also observes removal of a previously opened
panel.
A previously positioned panel with an anchor that leaves view reports empty
geometry.

AppKit Popovers support optional transition durations in milliseconds. Set
`enterDuration` and `exitDuration` in native RSX, or call `popover_fade` on the
portable panel. Use `popupStartingStyle` and `popupEndingStyle` in native RSX
or `popover_appearance` on the portable panel to supply
`jadix::presentation::style::PopoverAppearance`. Each endpoint supports
opacity, uniform scale and translation in right/down points. The default
endpoint is transparent with unit scale, zero translation and the panel's
normal background. An endpoint may also set an RGBA `background` color and a
`corner_radius` in logical points. AppKit animates both on the clipping layer.
Durations
default to zero. AppKit honors Reduce Motion and removes a closing panel
from input and accessibility as the exit starts. A mounted panel reports
close after the transition finishes. Reopening during the exit cancels
that close.

The host mounts on the main actor and calls `JadixNativeView.close()` when
releasing its root. AppKitWindowRequest provides ID-based window operations.
Rust can receive close presses, close causes and intercepted quit requests
through `AppKitHost::set_window_event_handler`. Swift delegates forward these
events with `JadixNativeView.reportWindowEvent(_:)` before disposing the root.
`AppKitWindowState::title_bar_insets` supplies the top and side clearances for
custom chrome with a hidden title bar. The values are zero with the standard
or transparent title bar and in system full screen.
The embedding Swift application owns its window registry and lifecycle. It can
retain a `JadixExitConfirmation` and forward `applicationShouldTerminate`,
`windowShouldClose` for its primary window, and `windowWillClose`. AppKit waits
for the asynchronous decision before quitting. A quit request can join an
already pending primary-window close, and approval from either source quits the
application. Termination requests time out after 110 seconds. The presenter
can supply its own UI or use `presentSystemAlert`. The AppKit example enables
this behavior with `--confirm-exit`. Its application delegate collects shared
`on_app_quit` futures through `AppKitQuitCleanup` and polls them on the main run
loop for up to 200 ms before replying to AppKit. When the last window closes,
the delegate collects its owner-scoped handlers before disposing the root.
On system Quit, remaining Rust roots are disposed in `applicationWillTerminate`.
An embedding application can provide a `prepareToTerminate` callback to defer
the framework's reply until cleanup completes.

Shared grid cells and column headers carry row or
column spans. Grid validation rejects zero and out-of-range spans. It checks
overlaps by scanning span boundaries, with storage proportional to mounted nodes.
Windows UIA reports cell spans and resolves covered coordinates to the same
cell. AppKit reports actual cell spans, maps covered coordinates to their
mounted cells, and exposes every intersecting column header. Grid row and
column counts come from the descriptor. AppKit creates accessibility column
objects at mounted cell and header start indices and uses ranges for coverage
queries. Independent layout metadata for empty rows and columns remains open.
Popover transition classes and additional animatable style properties remain open.

When an AppKit single-line field acquires the window's field editor, it restores
the horizontal start of left, natural, and justified text if the selection's
caret remains visible. AppKit keeps its native alignment for centered and
right-aligned text. Enabled buttons, checkboxes, and switches apply authored
title foreground colors and refresh them on appearance changes.

Optional `code` exposes `syntax::SyntaxWorker`, reusing `jadix_document` immutable
snapshots and its syntect parser. One worker owns one native Input/Textarea and
keeps one running parse, one replaceable pending job and one replaceable result.
Requests retain complete node identity, request generation and document revision;
polling also checks the current native-model text before accepting spans. It
admits 128 KiB source, 4096 lines, 8 KiB per line and a 64-byte language token;
more than 2048 spans becomes one plain span preserving the source bytes. Worker
data has no reactive/UI owner and sleeps on a condition variable while idle.
Cancellation supersedes results, and removal/revocation closes the worker without
joining on the UI thread. The bounded parser invocation already running is not
interrupted; this API does not promise a hard parser execution-time deadline.
The `native-code` sample combines a real Windows editor, Unicode search, source
checkpoints and a separate native syntax preview. Inline colored editing,
completion/LSP, rich text and mobile editor bindings remain separate work.

The optional `motion` feature adds native layout transitions without a GPU
dependency. `MotionValue` reuses `jadix_animation` timing, spring and bounded
decay generators, accepts monotonic caller time, preserves spring velocity when
retargeted, validates numeric inputs and finishes every track within 20 seconds.
`LayoutMotion::new(root.model().lifetime())` binds at most 256 node transitions
to one root. Use `transition(node, Some(Transition::default()))`, then
`NativeRoot::present_with_motion` with the same authored tree and viewport.
Layout computes target rectangles before motion projects relative positions and
sizes; unbound descendants follow the projected parent without double motion.
Reparenting and zero-size hiding snap, removed nodes release tracks, and a failed
native publication revokes both the root and its motion. Widgets retain their
native identities during geometry updates.

The application supplies `MotionPolicy::{Full, Reduced, Paused}`. Reduced snaps
to current targets. Paused freezes the motion clock and requests no frames;
resuming excludes the paused wall time. `next_frame_delay()` returns 16 ms only
while an animation needs another frame. Wait on platform events when it returns
`None`; this API starts no worker, timer or global animation loop. On Windows,
`WindowsHost::motion_policy()` reads the client-area animation preference and
pauses hidden or minimized windows. Other hosts must supply their own lifecycle
and accessibility preference. The current real Windows test preserves EDIT
text, selection and undo throughout a layout transition. This does not establish
native opacity, GPU transitions, touch gestures, interactive overlay animation,
or animation acceptance on Android/OHOS.

Windows now has a retained Win32 host behind feature `windows`; OHOS has an
ArkUI NativeNode host behind feature `ohos` on `target_env = "ohos"`. The shared
example and local delivery commands are documented in
[native-controls](../../examples/native-controls/README.md). Windows exposes
twelve node kinds; OHOS exposes nine base kinds, including bounded embedded PNG
Image nodes, and Dialog/Popover/Select when the runtime supplies ArkUI_NativeDialogAPI_1.
OHOS uses modal dialog controllers for Dialog and nonmodal controllers for
Popover. Both retain controlled dismissal, anchor and logical portal dependencies,
and focus updates after presentation. A missing dialog interface leaves these
kinds unsupported before publication. The default OHOS page includes the nested
editor dialog; the build tool also accepts the shared Popover and Selection products. Rust ABI
contract tests and SDK/HAP builds cover implementation boundaries. Actual ArkUI
display, nonmodal pointer routing, Back/outside dismissal, IME, focus restoration
and assistive technology still require an OHOS run. OHOS Select composes a native
Button trigger, Scroll/Column options and modal NativeDialog controller; it does
not establish a platform combobox role. Its registered key callbacks implement
Up/Down, Home/End and Escape; actual ArkUI keyboard behavior remains unverified.
Image sources are validated PNG data URIs capped by the shared source, dimension
and pixel budgets; HTTP/file URI loading is not delegated to ArkUI. Unsupported
kinds fail before publication. Neither adapter establishes complete component
or platform parity.

Win32 palette publication maps foreground/background colors, inherits container
backgrounds and follows the system high-contrast colors. Palette-only changes
reuse text measurements. Basic native image, dialog and popover roles are
checked against UIA; the legacy dialog-role bridge currently appears as Pane,
and does not establish a complete modal WindowPattern provider.

`AccessibleRole::Grid { rows, columns }`, `GridCell { row, column, row_span,
column_span }` and `GridColumnHeader { column, column_span }` use zero-based
logical coordinates on semantic Container nodes. Cell spans cover a rectangular
area, and headers cover a range of columns. Before publication, the shared model
checks signed-32-bit dimensions, a nearest Grid ancestor, nonzero in-range spans
and one mounted cell or header per covered coordinate. Windows exposes Grid,
GridItem, Table and TableItem UIA patterns, reports cell spans, resolves GetItem
for every covered coordinate and associates column headers across overlapping
spans, including cells nested under separate frozen panes. AppKit maps covered
coordinates to their mounted cell, reports actual row and column ranges, and
includes spanning cells in each covered column's accessibility projection.
Each cell exposes the distinct column headers intersecting its range. Empty rows
and columns still have no independent shared layout metadata. GetItem only locates mounted cells and
returns unavailable for a valid logical coordinate without a mounted provider.
Native EDIT children retain their system input providers. Count, coordinate,
span and name changes raise UIA property events; provider calls reject
invalidated state. Actual UIA client scalar getters may return defaults after
removal, so liveness must not be inferred from a scalar alone. UIA clients may
normalize a removed item's header lookup to an empty array; direct provider calls
reject invalidated cells and owners. This does not implement row headers, virtual
item realization, range selection, screen-reader acceptance or mobile/DOM Grid
patterns.

`NativeRoot::focus` sends one validated focus operation with an exact platform
receipt. Opt-in `PlatformHost::bind_navigation` routes composite arrow/Home/End
keys on supported hosts; unsupported hosts return false rather than advertising
bindings. Default builds retain no GPU dependency.

Feature `windows-gpu` mounts a bounded DX12 line plot in an owned empty native
container. The native input, button and accessible status remain system controls.
Surfaces resize with the HWND, suspend while hidden, recover a lost/outdated
surface with one retry, and are dropped before the native node. Device loss
reports an error. The Windows product probe includes GPU pixel readback, native
undo/identity checks, surface recreation and zero idle submissions. This is a
chart slice, not the complete Jadix GPU component tree or GPUI embedding. Android
and OHOS GPU integration, physical input-to-photon and composition remain open.

`NativeRoot::with_lifetime` attaches a host to an existing root lifetime, including
a Shell session mount. Revocation rejects publication/events and disposes the
host on its next poll; the UI loop must continue servicing that lifecycle.

`images::ImageLoader` loads bounded local PNG sources on one worker; Windows
also fetches HTTP(S) with WinHTTP. Source/result queues, bindings, PNG bytes,
dimensions and memory-cache entries/bytes have separate limits. Requests and
reloads retain a generation per complete NodeId. Polling rejects removed nodes
and stale generations, while root/loader disposal closes both channels and
cancels retained work. The worker never joins on the UI thread. Cache age applies
to new bindings; an active source stays retained until reload or cancellation.
Explicit reload bypasses the cache. The UI receives an `Arc<str>`
data URI to publish through its controlled tree and ordinary root budgets.

`ImageLoader::with_disk_cache` opts file and eligible URL sources into `ImageDiskCache`.
The caller supplies an absolute cache directory, record byte/entry limits and an
age from 1 ms to 24 hours. The worker manages a `jadix-images-v1` child directory;
record limits include the 56-byte header, with at most 128 records / 64 MiB.
Disk I/O stays on the source worker. An OS file lock excludes another writer;
lock or I/O failures leave ordinary source loading available and appear in
`ImageResult::disk_error`. Diagnostics expose counts and worker activity without
source values. `disk_hit` distinguishes persistent hits from memory hits.
Cache disposal closes channels without joining; the lock releases when the
worker exits, observable through `inspection().worker_active`.

Source-derived SHA-256 names and checksummed records detect accidental corruption;
this is neither encryption nor authenticated content. Corruption falls back to
the source, expiry does not extend on a disk hit, reload removes the old disk
record before fetching, and writes use a temporary record followed by rename.
Entries evict by last disk access. New loaders/processes can reuse unexpired
entries after the original file disappears. Callers must choose a directory
appropriate for their application's data ownership; this cache is not a crash
journal or a guarantee of persistence across power loss. Memory-cache age and
active-binding retention still follow the policy above.

Windows URL responses enter memory or disk cache only with an explicit, unique,
valid `max-age` and Date. Freshness subtracts apparent age, Age and request delay;
body transfer, completion queue time and subsequent cache hits do not renew it.
The HTTP lifetime is capped at 24 hours, then bounded by the configured disk age
or memory age. Responses with no-store, no-cache, private, Vary, Set-Cookie,
Pragma, Content-Range, ambiguous lifetime or malformed metadata still load but
are not cached. There is no heuristic freshness, conditional revalidation or
stale-on-error fallback. Reload invalidates memory and disk before fetching;
failure cannot resurrect the old entry. Credential-bearing URL authorities are
rejected before lookup. External HTTPS and other hosts' network transports
require their own runtime acceptance.

WinHTTP retains TLS verification, disables authentication/cookies and redirects,
uses asynchronous request handles, stage timeouts and cancellation/transfer
deadlines. The source worker checks cancellation every 20 ms while waiting for
send, headers or a body chunk; cancellation closes that request instead of waiting
for its stage timeout. Callback-owned buffers remain alive until handle closing,
and callbacks never touch a UI root. A stalled-header/body loopback regression
checks that cancellation closes the peer connection and lets the same worker
complete its next local-file job within 1.5 seconds. This is a regression bound,
not a measured latency guarantee. External HTTPS, persistent network cache and
non-Windows network transports remain unverified or unimplemented as appropriate. Metadata validation
precedes the platform PNG decoder; a valid header does not establish successful
decode. `WindowsHost::waker` wakes the normal message loop, and
`image_diagnostics` reports actual decoded dimensions without exporting sources.

Windows `install_font` registers a bounded TTF/OTF collection with GDI and retains
its source bytes as a process-private resource. Each host admits eight leases and
8 MiB of source data; duplicate bytes reuse their lease. Windows family names,
rather than typographic/Mac aliases, select the real native font. Installation
invalidates the host environment and refreshes existing HFONTs on the next poll;
publish the next layout using `host.environment()` to remeasure. `remove_font`
rejects foreign leases and mounted family references. Root disposal drops native
control fonts before private registrations. `font_resources` and
`font_diagnostics` expose resource counts and actual selected GDI metrics. This
does not establish a cross-platform font/shaping/fallback protocol or persistent
font cache.

Hosts may apply `CommitBatch` incrementally and return the exact `ApplyReceipt`.
The default host method preserves snapshot consumers. Shared roots validate one
candidate, publish changed batches and revoke a root on execution/receipt failure.
Retired identities use exact interval storage, with separate logical-count and
range-count budgets; applications that deliberately raise the former still
remain bounded by the latter. Events retain the complete NodeId when received.

Android now transfers protocol-2 delta operations through JNI, validates the
root and exact revision in a Java candidate ledger, applies changed native
properties and returns an independently checked apply receipt. Structural
changes still reconcile sibling order. A newer APK build does not inherit the
runtime acceptance of older snapshot APKs.

`NativeRoot::inspection(false)` exports identities, frames, event eligibility,
resource budgets, measurement/cache counts and the last commit's dirty nodes.
Text is omitted by default; password values remain omitted even when text is
requested. These are presentation diagnostics, alongside separate native UIA
or platform accessibility inspection.

Portable presentation validation, Taffy layout and optional platform hosts. The
default build uses `jadix_core` without GPU, window or VM dependencies. Optional
`android` and `dom` features enable JNI and browser controls; `uikit` exposes the
C callback host used by the accompanying Swift implementation.

`HeadlessRoot::commit` validates the complete ordered batch before publishing
a revision; structure-changing batches use a complete candidate tree. Empty commits do not advance the revision or request work.
`reconcile` accepts retained `PresentationNode` snapshots, computes creates,
updates, moves and removals, and suppresses unchanged snapshots. Surviving children
move out before an obsolete parent is removed. Backend capabilities reject
unsupported node kinds, editing, scrolling and focus before publication.
Moves preserve node identities; removal retires identities and clears focus.
Events validate root, live node, control kind, enabled state and edit revision.
UTF-16 selection and composition ranges reject split surrogate pairs.
UIKit keeps `UITextView` content sizing and scrolling inside the native editor.
Only descriptors of kind `scroll` receive shared outer content sizing or emit
shared scroll events. The simulator checks require multiline Rust round trips,
selection/content-size retention on environment publication, and isolated editor
scrolling in both controls and notes. These checks require an actual Apple run;
receipt-validator fixtures and Windows Rust tests do not establish UIKit acceptance.
UIKit scales fonts with the host view’s traits for both detached measurement
controls and retained controls. A content-size-category change refreshes fonts
without rewriting editor content, then invalidates shared measurement caches.
The host owns this font refresh rather than combining it with automatic child
font scaling; unchanged geometry need not force a presentation commit. The
simulator probe records large/accessibility/large font and measurement values,
checks editing retention, and rejects missing, nonfinite or unscaled receipts.
The iOS example clears cached keyboard occlusion on keyboard hide and background
notifications, and refreshes the environment when active. A viewport resize or
focus change reveals the retained editor through logical outer Scroll ancestors;
UITextView keeps ownership of its internal scrolling. Simulator checks exercise
programmatic notification/safe-area geometry and focused-control visibility,
which still require Apple execution and do not establish OS lifecycle acceptance.

Run `cargo run -p jadix_native --example headless` for a windowless button flow.
The existing framework provides `jadix::authoring::native::ElementProjection` for
plain containers/text and typed Button/Input builders, including RSX. It reads
already computed layout bounds and routes events to existing callbacks/signals.
For GPU-free authoring, select `jadix_components` with `default-features = false`
and `features = ["macros", "theme-shadcn"]`. The registered portable subset includes
containers, plain text, Button and controlled Input. It uses the existing macros,
theme and reactive Owner. The shared page in `examples/portable-contract` also
compiles against the GPU author surface. Advanced GPU components and styles remain
outside this portable subset.

`layout::NativeLayout` drives Taffy outer layout with a host measurement callback.
Results must echo the node, content/style/environment revisions and constraints in
the supplied measurement key. Content, fonts, constraints and environment changes
invalidate cached measurements; unchanged leaves reuse them. Invalid measurements
leave all published frames unchanged. The host increments the environment revision
when display scale, locale, font availability or platform typography changes.

For coordinated shutdown, pass clones of one `RootLifetime` to
`HeadlessRoot::with_lifetime`, `ElementProjection::with_lifetime` and
`HybridCoordinator::with_lifetime`. Revoking any clone disables node access,
commits, events, geometry publication, hit testing and focus across these consumers.
Shell hosts can obtain this lifetime from `Session::mount_root`; session shutdown
revokes it before releasing the reactive Owner. Existing `new` constructors create
independent lifetimes, even when given the same root ID. Standalone hosts must
dispose their participants before disposing the Owner.

Revocation is immediate logical invalidation. Retained model allocations are
released by explicit disposal or drop; platform view destruction remains the host's
responsibility. Dropping a lifetime clone alone does not revoke other consumers.

`host::NativeRoot` measures through its platform host, validates a candidate tree,
and publishes only changed snapshots. A publication failure revokes the root and
clears platform views. Unchanged frames do not publish. Hosts retain platform
controls by node identity, preserving input and scroll state across updates.
DOM structural publication keeps a focused subtree connected during same-parent
reordering by moving surrounding siblings. This preserves native editor undo and
redo history alongside focus and selection. Other moves restore retained focus
and Input/Textarea selection direction with scrolling suppressed. Removed or
disabled controls are not refocused. Cross-parent editor history still requires
separate platform validation.
Normal DOM control focus requests native nearest-edge scrolling, so keyboard
navigation keeps the whole control visible inside clipped scroll ancestors.
The browser example measures its actual mount container on every mount instead
of starting remounted roots at a fixed viewport. Edge 156, Firefox 157 and
Playwright WebKit 27.2 cover complete forward/reverse Tab order across eight
controls at 100/1000 rows, a compact viewport and repeated root replacement.
These are browser keyboard events; initial focus is programmatic and OS
assistive technology remains a separate validation requirement.

The DOM `EventSink` returns `Result<(), String>` from synchronous event dispatch.
Consumers must return validation or application errors instead of swallowing
them; the host then restores rejected Input, Textarea, Checkbox and Switch changes to
their last published values. A root revoked by a publication failure is not
restored. The example also exposes a drainable error through `take_error()`.

The [native controls example](../../examples/native-controls/README.md) mounts
one Rust page through Android widgets, browser DOM or UIKit callbacks. Android
has an emulator instrumentation entry point; DOM has a browser scenario runner.
The Swift implementation still requires compilation and device validation with
Xcode. These implementations do not establish platform transaction atomicity.
The shared `NodeStyle`
supports finite flex geometry, padding/gap, RGBA colors and bounded typography;
transforms, rich text and arbitrary GPU effects need separate capability support.
Portable Textarea, Checkbox, Switch and embedded PNG Image builders/RSX are also available. Their shared
events include versioned UTF-16 edits and controlled boolean toggles. Android
and DOM extension scenarios exercise these controls; UIKit implementations still
require Apple validation. The component schemas are available separately through
`jadix_components::schema::extended_components`, preserving the smaller base
schema used by existing Shell SDK clients.

Switch has its own node/capability and opt-in extended schema. It shares controlled
boolean revision validation with Checkbox. Android uses android.widget.Switch;
DOM uses the browser checkbox input with role="switch", preserving platform
keyboard behavior and appearance. UIKit uses the existing UISwitch/caption host;
its compile and runtime validation remain pending on an Apple toolchain.

Select is an optional extended component with a GPU-free Builder, options RSX
and basic compound RSX surface. DOM maps it to a retained HTMLSelectElement with native keyboard
selection. Controlled values, disabled controls/options, unique bounded options
and string event revisions are validated before accepting changes. A rejected
selection restores the published value. Android uses a retained native Spinner
with disabled option states, theme typography and native popup selection. Its
API 26 / 36 AVD scenarios exercise touch, injected keys, rollback, recreation and
open-popup disposal. UIKit Select has a UIButton/UIMenu source mapping with controlled
labels, disabled actions, retained menu identity and stale-menu generation checks;
its separate probes and packaging await Apple compilation and runtime validation.
OHOS uses native Button option rows with disabled and selected accessibility
states. Its retained trigger and controlled option identity, Unicode labels,
revision filtering, disabled/unknown event rejection and publication suppression
have host ABI coverage. Option changes, removal, disposal and owner overlay stack
changes close the option dialog before its owner. Typography, foreground and
background reach retained option buttons; large fonts increase row height without
replacing an open dialog. Up/Down skip disabled options and wrap; Home/End focus
the first/last enabled option, with scroll-to-focus, and Escape closes without
changing the controlled value. Modified shortcuts remain with the platform.
Native checkbox, switch and radio nodes also receive ArkUI's checked accessibility
state. The SDK20 production C++ contract covers both checked and unchecked
updates. The built-in NativeNode bridge has no arbitrary IDREF relation property.
`OhosHost::from_napi_with_accessibility` is an opt-in alternative that takes a
caller-owned XComponent FrameNode as the ArkUI provider root and publishes the
committed Jadix accessibility tree as virtual elements. Stable element IDs,
parent/child links, component role, labels, descriptions, selected/checked,
enabled/clickable flags and density-scaled bounds are refreshed with each
published tree; supported click actions return through the normal Jadix event
queue. The XComponent must belong to the same live N-API environment and
outlive the host. The existing `from_napi` path remains provider-free. This
provider does not yet synchronize accessibility focus, expose expanded/collapsed
state, notify platform content changes independently of a presentation commit,
or route scroll/text-edit actions. The caller must also ensure the supplied
XComponent represents the same visible surface as the host's NodeContent tree.
Those runtime relationships and screen-reader behavior still need device
validation.
Twenty-one host scenarios compile the production C++ against installed SDK
headers and execute registered callbacks through a mocked ArkUI API, including
disposal, failure cleanup and the XComponent accessibility provider lifecycle.
On Windows, reproduce them with:



The receipt and log are retained separately from disposable target files. API20
C++ checks and the isolated API22 x86_64 Selection HAP build pass. Actual popup
operation, native button activation by Enter/Space, keyboard dispatch, focus
restoration and assistive technology still require runtime validation.

Basic SelectTrigger/SelectValue/SelectContent/SelectItem slots normalize to one
native Select leaf with static Unicode labels and disabled options. The shared
selection sample uses compound RSX for its primary control and the options Builder
for its disabled control; build reports identify this authoring combination.
The compound subset supports controlled value, root/value placeholders and numeric
trigger chrome. It rejects malformed or standalone slots, interactive/dynamic
labels, mixed options/children and custom popup styling. Compound root default_value,
disabled and controlled-open flags, grouped items and rich popup content remain
outside this subset; the options Builder retains its default-value/disabled API.

Images accept only embedded PNG data URIs, with a 1 MiB source limit,
4096 pixels per axis and a 4 megapixel budget.
Header validation precedes platform decoding; it does not prove full PNG validity.
Taffy uses resource dimensions and aspect ratio independently of decode success.
Detected decode failures retain their layout frame, alternative text and a visible
gray placeholder; the status is exposed through platform accessibility metadata.
Android and DOM exercise this behavior; UIKit still requires Apple validation.
Remote fetching and virtual lists are not yet exposed. A retained scrolling list is available through
`jadix_components::scroll_area` without the GPU feature.

The headless validator stages Text/Button label-only updates using addressed
records and final aggregate text accounting. Geometry, style, semantics and
button eligibility must be unchanged; other batches retain the complete
candidate validation path. Duplicate updates use the final descriptor for
resource accounting, and any failure preserves the complete live state. The
visible modal is computed on structural candidates and reused by event admission;
structural changes invalidate it. Ordered node lookup still costs log N. This
does not optimize NativeRoot snapshot layout/publication or terminal output.
Retired identities remain bounded until disposal. Default
`ResourceLimits` bound each root to 4096 live nodes, 65536 retired identities,
8 MiB of UTF-8 text (including labels, placeholders and accessible names),
8 MiB of image source URIs and 16777216 declared image pixels. Hidden Dialog
children count toward these totals, and repeated images count per node.
`HeadlessRoot::with_limits` and `NativeRoot::with_limits` configure a root at
creation; `resource_limits` and `resource_usage` expose its model accounting.

An over-budget commit returns `BackendError::ResourceLimit` without changing
the committed tree, revision, focus or usage. NativeRoot checks tree budgets
before platform measurement or publication. Atomic batches use their final
tree size, allowing replacement at capacity. Retired identities are never
silently evicted; reaching that ceiling requires disposing the root and
creating a fresh one. Disposal clears the retained usage. These are model
payload and declared-pixel budgets, not a process RSS or decoder-allocation
guarantee; platform widgets, caches, transient copies and image decoding have
additional overhead. Business-state persistence belongs to the host product.

Portable callbacks and bound-text refresh restore the component's reactive Owner
and Context. Resources created by a callback belong to that Owner and are disposed
with it; callers still dispose the presentation root before releasing its Owner.

Portable Dialog uses the existing `dialog`/`dialog_scope`, `DialogTrigger`,
`dialog_content` and RSX interfaces. Its content is built on first open and retained
until the owning root is disposed. `DialogBody` provides a native scrolling region;
Header, Title, Description and Footer are content slots. The default close control
is a native button. Controlled open state and `dialog_close` share the same Owner
and Context as the rest of the page.

Dialog layout is a viewport portal outside normal flex flow. The top modal alone
receives platform events; hidden content and background callbacks are rejected.
Dismissals carry monotonic revisions. Android uses `android.app.Dialog`, while DOM
uses `HTMLDialogElement.showModal`, forward/reverse Tab containment, Escape and
native focus restoration. DOM button activation explicitly focuses its control
before dispatch so dialog restoration retains the activation trigger on WebKit
as well as Chromium and Firefox. Both adapters support nested dialogs and remove their
modal windows on disposal. The UIKit source uses a separate UIWindow with modal
accessibility and Escape handling; it remains uncompiled and unverified on Apple.

The portable Dialog currently uses default native chrome. Custom overlay color,
opacity, blur, `hug_content` and panel classes return `ProjectionError::Unsupported`
rather than changing rendering silently. Finite `NodeStyle` panel customization
through `panel.base` remains available. Menus and other GPU overlays need their
own platform contracts.

| Portable component | Android API 26 / 36 AVD | Edge 156 / Firefox 157 / Playwright WebKit 27.2 DOM | UIKit |
| --- | --- | --- | --- |
| Text, container, Button, Input, Scroll | Runtime scenarios | Runtime scenarios | Source; Apple validation pending |
| Textarea, Checkbox, embedded PNG Image | Runtime scenarios | Runtime scenarios | Source; Apple validation pending |
| Switch | API 26 / 36 AVD: native Switch, two retained row scales and process recovery | Native checkbox input with switch role; no custom switch-track styling | UISwitch source; Apple validation pending |
| Select (options and basic compound API) | API 26 / 36 AVD: native Spinner/popup, injected touch/keys, rollback, recreation and disposal | Retained native select; controlled values, disabled options, keyboard, reset and disposal scenarios | UIButton/UIMenu source and programmatic probe entry; Apple compilation/runtime pending |
| Dialog and nested Dialog | Native windows, shared input, Back, IME avoidance, recreation | Native top layer, shared input, Tab, Escape, backdrop, disposal | UIWindow source; Apple validation pending |
| Popover (Builder and compound RSX) | API 26 / 36 AVD: native PopupWindow, nested panels, InputConnection, scroll/flip, dismissal, recreation and disposal | Native manual HTML Popover, nested panels, controlled dismissal, scroll anchoring and retained input | Retained UIView overlay source and isolated programmatic probes; Apple compilation/runtime pending |

Only Chromium has CDP IME evidence; other engines use synthetic composition
probes, not native IME. Playwright WebKit does not establish Safari validation.
The tested WebKit build caps single-line values at 524288 UTF-16 code units.
Controlled input and textarea publication rejects a value if native readback
differs, rather than silently accepting platform truncation.

The APK minimum is API 26; this acceptance run used an API 36 emulator and does not establish
API 26 runtime or screen-reader validation. iOS VoiceOver, Android TalkBack and browser assistive technology still
require target-specific verification.

The Android input adapter removes the platform's implicit single-line character
filter so committed values do not silently truncate before reaching the shared
protocol. The protocol's bounded event envelope still applies. Rejected platform
events preserve the last published control value and can be observed with
`NativeView.takeError()`; low-level JNI calls continue to return errors directly.
The native subtree leaves lifecycle persistence to its host, preventing duplicate
editor text in the platform view-state Bundle.

Android publication reorders retained children within one parent without detaching
their native views. API 28 and 36 AVD probes cover Input, Password and Textarea
focus, reverse UTF-16 selections, composing spans and subsequent InputConnection
commits at 100 and 1000 retained rows. Cross-parent moves and physical IME sessions
require separate validation.

Android mounting uses the attached host viewport and safe padding when available;
before attachment it starts with display metrics and follows normal resize
publication. This keeps compact roots correct after unmount/mount without a
window resize. API 28 and 36 x86-64 AVD checks cover all eight controls with
injected Tab/Shift+Tab at 100 and 1000 rows: initial, compact viewport and after
25 remounts. Each observation requires stable native focus, complete control
visibility, retained identities and unchanged Unicode business state. The
dedicated keyboard probe suppresses soft input; existing IME/Dialog checks run
in a separate Activity. These checks do not establish physical keyboard or
TalkBack acceptance.

Android native editing checks also cover injected Ctrl+C/X/V and Ctrl+Z/Shift+Z
for Input and Textarea, using Chinese, Arabic, emoji and multiline values. Native
undo/redo survives unchanged-value publication and same-parent reorder; a changed
controlled value clears the prior edit history. Each step checks the retained
editor and Rust business value. Password paste is supported while native copy
and cut are rejected without changing the clipboard or public Name state. The
probe restores clipboard content and runs separately from IME checks. API 28/36
AVD observations do not establish physical keyboards, cross-parent edit history
or OS clipboard metadata/URI-permission preservation.

The Android controls acceptance runner verifies the requested APK and scenario
against build.json before using the device. It refuses any existing controls
registration, installs without replacement, and restores the package registry
after successful checks or probe failure. The explicit --keep-installed option
retains a successful sample for subsequent resource collection. UI tree dumps
use fresh temporary paths and are removed after collection.

The shared presentation protocol includes a headless Popover foundation with a
root-scoped anchor, twelve physical placements, side flipping, viewport insets,
controlled dismissal revisions and dependency-ordered portal layout. DOM advertises
Popover only when the native showPopover/hidePopover functions are available.
Android advertises its native PopupWindow implementation. The UIKit callback
host advertises the accompanying NativePopover.swift implementation; Apple
compilation and runtime validation remain pending. A custom PlatformHost must
explicitly implement this capability. Unsupported hosts reject panels before
measurement or publication.

The headless contract requires live, same-root anchors and acyclic overlay
dependencies. Removing an anchor while retaining its panel rejects the atomic
commit. An open visible panel cannot reference a hidden anchor. Closed panels
skip measurement and clear descendant frames. Placement start/end refer to
physical left/top and right/bottom; logical RTL alignment and assistive technology
require separate validation. Panels use an 8-unit viewport inset and a default width
of 320; dimensions and minimums are bounded to the available viewport. Padding
that cannot fit that viewport rejects layout without publishing partial frames.

GPU-free `popover`/`popover_scope`, PopoverTrigger, PopoverContent and header/title/
description slots share the existing Owner and Context. Content is built on first
open and retained until disposal. Portable defaults use a 288-unit panel and a
4-unit gap. Width accepts a finite nonnegative number or px string; side offset
must be a whole nonnegative u16 value. Numeric panel chrome is available through
panel.base. Unsupported options, classes, scroll locking and strategies other
than flip reject projection. Anchor names must resolve to exactly one actual
projected node, including when content precedes a custom trigger.

The DOM host uses native manual top-layer panels with ARIA anchor relations and
native editor children. It positions against the root/browser viewport intersection,
tracks native scrolling, focuses the first control, restores the trigger after
closing, and follows panel opening order for Escape and outside dismissal. Escape
already consumed or originating outside the root is left to its owner. Panel
disposal removes listeners and anchor ARIA state; stale callbacks cannot reach a
fresh root. The isolated --popover Web sample and three-engine runner exercise
these paths with browser-generated touch, pointer and keyboard input. Native API
calls and DOM probes are separately recorded. This does not establish physical
touch/IME, OS screen readers, Safari or UIKit Popover acceptance.

Android retains each panel and its native controls in a focusable PopupWindow.
It positions against actual screen anchors and the visible root viewport, follows
native scrolling, flips near an edge, and dismisses in opening order. Escape/Back
and contextual close update the same controlled Rust state; closing restores
trigger focus when appropriate. Disposal removes open windows before retiring
the root. API 28 uses public window-relative coordinates; API 29 and newer use
public screen layout and non-touch-modal APIs. An outside pointer closes the
panel on both tested versions; activation of the underlying control by that same
pointer is not guaranteed on API 28.

The isolated Android --popover release sample runs fifteen scenarios on API 28
and 36 x86-64 AVDs with the same APK. They cover injected touch/keys, retained
native Unicode editing, programmatic InputConnection preedit/commit, scrolled
anchors, bottom-edge flipping, independent/nested panels, ten unmount cycles and
Activity recreation with restored business state. This does not validate physical
IME, TalkBack, other system versions, every placement, Popover/Dialog composition
or UIKit. The sample remains development-signed.

The UIKit source retains a nonmodal UIView overlay in the anchor window. Native
coordinates and the safe viewport determine placement; scroll and environment
updates reposition retained content. Opening order controls dismissal, and
teardown detaches gestures and callbacks. Native editors keep marked text during
unrelated publication; a changed controlled value still replaces it. This source
requires both JadixNativeView.swift and NativePopover.swift in the Swift target.
The isolated iOS --popover sample and its optional twelve-check simulator probe
cover programmatic controls, text, scroll, geometry and disposal. Local Rust C ABI
tests and offline packaging/receipt tests pass; Swift syntax parsing is a grammar
check only. These results do not establish Swift compilation, UIKit execution,
native touch/keys, physical IME, VoiceOver or Popover/Dialog composition.

Windows standalone hosts expose `window_frame` and `set_window_frame` in physical
desktop pixels. `defer_close` keeps a requested window alive for final event
draining and persistence; `close_requested` and `resolve_close` let its owner
accept or cancel that request. Default hosts retain their immediate-close policy.
Programmatic `request_close` goes through the same WM_CLOSE path.

`PlatformHost::bind_drag` is opt-in and reports availability. The Windows binding
accepts up to 128 mounted native buttons, uses a six-logical-pixel threshold and
emits typed `EventPayload::Drag` phases with physical screen coordinates. Moves
coalesce only when consecutive for the same node; start, end, cancellation,
editing and activation share the bounded ordered native queue. Capture loss,
Escape, disabling, unbinding and disposal release capture. Cancelled phases may
finish a still-mounted disabled/hidden node; they cannot activate it. This is a
mouse drag binding, not multi-touch, long-press or scroll arbitration support.


## Shared host preferences

`jadix_native::environment` re-exports `jadix_core::environment`. `NativeRoot::present_environment` keys measurement invalidation by both environment identity and revision; switching to another root with the same revision, or back to the legacy numeric `present` API, cannot reuse stale metrics. It keeps presentation IDs, host controls and editing state. Applications still map preferences to NodeStyle and content explicitly; the new entry point does not infer native theme/RTL support. Inspection exposes the bound identity/revision only when this typed entry is used.

The `windows` feature enables the core's optional read-only system adapter. Real Win32 EDIT regression verifies that environment/font updates preserve the HWND, Unicode text and UTF-16 selection and leave subsequent native undo usable. Other platforms and automatic system-change notifications require separate adapters/validation.

With `motion`, `present_environment_with_motion` uses the same environment cache
identity and derives animation policy from its snapshot. Inactive/background roots
pause the animation clock; returning to active resumes without catching up the
elapsed background time. Active roots with `reduced_motion: Some(true)` snap to
the target; an unavailable preference uses full motion. Frame scheduling remains
the caller's responsibility through `LayoutMotion::next_frame_delay()`.

With both `consistent-ui` and `motion`, `present_consistent_with_motion` applies
the selected theme and font scale before layout and animation, preserving authored
styles. Both entry points reject disposed environments before publication, and a
failed platform publication disposes the root and its animation tracks.
