# jadix_native

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

`AccessibleRole::Grid { rows, columns }` and `GridCell { row, column }` use
zero-based logical coordinates on semantic Container nodes. Before publication,
the shared model checks signed-32-bit dimensions, a nearest Grid ancestor,
in-range coordinates and one mounted cell per coordinate in that Grid. Windows
exposes Grid/GridItem UIA patterns for these containers, including cells nested
under separate frozen panes. GetItem only locates mounted cells and returns
unavailable for a valid logical coordinate without a mounted provider. Native
EDIT children retain their system input providers. Count/coordinate/name changes
raise property events; provider calls reject invalidated state. Actual UIA client
scalar getters may return defaults after removal, so liveness must not be inferred
from a scalar alone. GridColumnHeader identifies a unique, in-range column on a
Text or Container node under the nearest Grid. Windows exposes Table/TableItem
patterns linking mounted column headers to their current logical columns, with
row-major orientation and empty row-header arrays. UIA clients may normalize a
removed item's header lookup to an empty array; direct provider calls reject
invalidated cells and owners. This does not implement row headers, virtual item
realization, range selection, screen-reader acceptance or mobile/DOM Grid patterns.

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

`HeadlessRoot::commit` validates ordered operations against a candidate tree before
publishing a revision. Empty commits do not advance the revision or request work.
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
API 28 / 36 AVD scenarios exercise touch, injected keys, rollback, recreation and
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
Six host scenarios compile the production C++ against installed SDK headers and
execute registered callbacks through a mocked ArkUI API, including disposal and
failure cleanup. On Windows, reproduce them with:

```powershell
powershell -File scripts/checks/native-ohos-select-cpp.ps1 -Sdk <OpenHarmony-SDK> -Output <new-receipt.json>
```

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

The headless validator clones its reference tree for each nonempty batch and
retains retired node identities until disposal. It is a correctness model;
large-tree commit cost remains proportional to the retained tree. Default
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

| Portable component | Android API 28 / 36 AVD | Edge 156 / Firefox 157 / Playwright WebKit 27.2 DOM | UIKit |
| --- | --- | --- | --- |
| Text, container, Button, Input, Scroll | Runtime scenarios | Runtime scenarios | Source; Apple validation pending |
| Textarea, Checkbox, embedded PNG Image | Runtime scenarios | Runtime scenarios | Source; Apple validation pending |
| Switch | API 28 / 36 AVD: native Switch, two retained row scales and process recovery | Native checkbox input with switch role; no custom switch-track styling | UISwitch source; Apple validation pending |
| Select (options and basic compound API) | API 28 / 36 AVD: native Spinner/popup, injected touch/keys, rollback, recreation and disposal | Retained native select; controlled values, disabled options, keyboard, reset and disposal scenarios | UIButton/UIMenu source and programmatic probe entry; Apple compilation/runtime pending |
| Dialog and nested Dialog | Native windows, shared input, Back, IME avoidance, recreation | Native top layer, shared input, Tab, Escape, backdrop, disposal | UIWindow source; Apple validation pending |
| Popover (Builder and compound RSX) | API 28 / 36 AVD: native PopupWindow, nested panels, InputConnection, scroll/flip, dismissal, recreation and disposal | Native manual HTML Popover, nested panels, controlled dismissal, scroll anchoring and retained input | Retained UIView overlay source and isolated programmatic probes; Apple compilation/runtime pending |

Only Chromium has CDP IME evidence; other engines use synthetic composition
probes, not native IME. Playwright WebKit does not establish Safari validation.
The tested WebKit build caps single-line values at 524288 UTF-16 code units.
Controlled input and textarea publication rejects a value if native readback
differs, rather than silently accepting platform truncation.

These observations do not establish other minimum-version coverage or screen-reader
validation. iOS VoiceOver, Android TalkBack and browser assistive technology still
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
