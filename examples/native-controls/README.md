# Native controls

`cargo run -p jadix_native_controls --features windows --bin native-radio` runs
the portable compound RadioGroup product with real Windows radio buttons.
Its `--probe NONCE NEW_REPORT` checks native checked state, stable control
identity, one group Tab entry, disabled-item skipping, keyboard focus and zero
idle commits. Programmatic messages are separate from physical keyboard and
screen-reader acceptance. Android uses RadioButton, DOM uses radio semantics,
UIKit selected-button source remains unvalidated, and OHOS explicitly rejects
radio groups until its native implementation exists.

Run consolidated engineering checks with
`python scripts/native.py validate --suite python --suite windows --suite wasm --output evidence/NEW_VALIDATE.json`.
The default runs Python and the current host suite. Windows includes portable
contracts; selecting both is rejected to avoid duplicate validation. Failures
retain command logs and a failed report. Use
`python scripts/native.py audit-validation --validation evidence/NEW_VALIDATE.json --output evidence/NEW_AUDIT.json`
to check source and log identities later. These reports never imply complete
roadmap, physical input, accessibility, performance-superiority or release acceptance.

`cargo run -p jadix_native_controls --features windows --bin native-tabs` runs a
portable compound RSX Tabs product using real Windows buttons and EDIT controls.
It demonstrates controlled selection, lazy panels, disabled-item skipping,
horizontal Home/arrow navigation and native focus. `--probe NONCE NEW_REPORT`
checks native editing, fresh identities after panel remount, retained business
text and zero idle native commits. Windows retains one native trigger tab stop
and Tab enters the active panel. TabList/TabItem/TabPanel roles and selected
state have a separate UIA regression. Windows TabItems expose SelectionItem and
their shared TabList exposes single required Selection/GetSelection; fold
triggers expose actionable ExpandCollapse and Invoke. Destroyed providers reject
actions. These programmatic UIA checks do not imply screen-reader acceptance.
Native undo/selection across panel
remount and physical IME remain unaccepted.
The shared projection defers conditional visibility changes during composition;
its synthetic composition regression is separate from physical input acceptance.

`cargo run -p jadix_native_controls --features windows --bin native-collapsible`
runs controlled compound RSX using system BUTTON/EDIT controls. It supports
disabled triggers, lazy mount, expanded descriptors, collapse/unmount and
business text restoration with a fresh editor identity. `--probe NONCE NEW_REPORT`
checks those operations, exclusive sibling switching with retained editor text,
and zero idle native commits. The product groups Account details and Preferences
using `exclusive={true} group="account"`; groups are scoped to their authoring
owner. Build/run also work via
`python scripts/native.py build --bin native-collapsible --offline --output NEW_BUILD.json`
and `python scripts/native.py run --artifact NEW_BUILD.json --output NEW_RUN.json`.
The projection composition guard has a synthetic regression. Physical IME,
screen readers and ExpandCollapse patterns are unaccepted; animated height,
focus restoration and native undo across collapse are not
implemented.

The same product verifies disclosure close-focus restoration against both the
shared model and Win32 `GetFocus`, including programmatic Accordion switching
and preserving focus on a user-activated alternative header. Projection hosts
pass their current focused node to `snapshot_with_focus` and apply its queued
focus request after presenting the resulting tree. Physical keyboard, IME and
screen-reader acceptance remain separate.

The same product includes an Accordion FAQ with two enabled headers and one
disabled header. Its programmatic Win32 probe checks Up/Down wrap, Home/End,
model and HWND focus, navigation without selection, single-panel switching and
collapse-all. Shared Builder/compound RSX tests also cover multiple selection,
lazy content and composition commit before unmount. These observations do not
establish physical keyboard or assistive-technology acceptance.

Windows product slices also include `native-gpu-chart` and `native-tsx`:

```powershell
cargo run -p jadix_native_controls --features native-gpu --bin native-gpu-chart
node examples/native-controls/shell/build.mjs --check
cargo run -p jadix_native_controls --features native-shell --bin native-tsx
cargo run -p jadix_native_controls --features native-shell --bin native-tsx -- --workspace target/native/tsx-workspace
cargo run -p jadix_native_controls --features windows --bin native-images
cargo run -p jadix_native_controls --features native-code --bin native-code -- --workspace target/native/code-workspace
```

The chart mounts a DX12 line plot beside retained native controls and can emit an
actual GPU readback image in its probe. The TSX product runs the compiled SDK
tree through QuickJS and `NativeMount`, with keyed native input movement and
undo. Its development-signed package probe replaces releases 1 → 2 → 1, rejects
a broken signed update and preserves a versioned business checkpoint. The native
package store confirms activation only after successful VM initialization and
native mounting, retains explicit rollback, and verifies packages again on restart.
The probe leaves an unconfirmed update in a process that exits, then starts an
independent process from the last healthy disk package, restores Unicode business
data and checks the recreated native editor's text. `--workspace` opens the
same store on subsequent launches, saves committed edits after 250 ms of quiet
with a two-second maximum delay, and checkpoints business data on close. Composition
pauses autosave; failed saves retain the latest value and retry after five seconds.
Idle workspaces disarm the autosave timer. Close failures keep the window open.
The probe exits a separate process after an automatic checkpoint, then verifies
the restored Unicode value in another process's native EDIT. Each business save
now publishes a bounded, checksummed recovery journal before replacing its
checkpoint. Another probe child obstructs that replacement, keeps the durable
journal and exits; after removing the obstruction, a new process replays it and
checks the restored native EDIT. Replay validates package identity, activation
revision and signed storage metadata before clearing pending activation state.
It does not activate a failed upgrade. Edits within the debounce window still
have no durable protection; the journal is neither an edit history nor a
power-loss acceptance result.
The store checks signed migration declarations and invokes
host migration callbacks when storage versions differ. This product changes
`{draft, count}` in schema 1 to `{profile: {name}, counter}` in schema 2 through
the signed candidate's `migrateStorage` export, running in a separate budgeted
VM. A failed migration keeps the active version. The probe restarts another
process from schema 2 and checks its native EDIT before rolling back. Rollback restores
the previous package's own checkpoint. Native handles are retained within a session; version replacement
creates a new root. These slices do not establish complete mixed GPU widgets,
physical IME, power-loss recovery, or Android/OHOS runtime acceptance. Rebuild TSX with
`node examples/native-controls/shell/build.mjs` after editing its sources.
`node --test examples/native-controls/shell/panel.test.mjs` checks migration
purity, invalid stored shapes and version mismatches. Existing schema-1 data
remains readable; the development fixture is regenerated and re-signed locally.

`native-code` retains a native multiline EDIT for source input and system undo,
with a separate native Text syntax preview generated on one sleeping worker.
It supports UTF-16-correct Unicode search, cancellation, bounded source/line
sizes, atomic source checkpoints and independent-process restoration. The
preview displays at most 24 lines and 160 native nodes; syntax results preserve
the complete source and reject superseded generations or document revisions.
Source editing is plain native text; inline syntax coloring, editor commands,
completion/LSP, rich text, unsaved crash journals and mobile products remain open.

`native-images` loads a local file or HTTP(S) URL without blocking the UI, offers
explicit reload/cache/cancel actions and retains the image HWND. Its probe uses
a generated local PNG and a loopback HTTP server, then checks the real WIC
decoded dimensions, cache hit, background wake and removal cancellation. The
report distinguishes this local scenario from external HTTPS and other hosts.
The product opts file sources into an 8 MiB / 32-entry / 24-hour disk cache under
`LOCALAPPDATA/Jadix/NativeImages` (temporary-directory fallback). Probe runs use
an isolated owned cache, remove the source and replace the displayed image,
then reopen the worker and verify WIC decodes the cached 32×16 image again with
the same HWND. This product probe does not claim a process restart; a separate
contract exercises producer process exit and offline reuse in a second process.
HTTP(S) remains memory-only; cache failures are visible in the status text.

One Rust page, one reactive Owner and retained platform controls. The page has
text, two buttons, controlled input and a native scrolling list of 100 rows.
It uses the registered portable `jadix_components` authoring subset without
Jadix GPU rendering or a JavaScript VM. Browser glue loads Wasm and forwards
viewport/font changes; application state and callbacks remain in Rust.

| Host | Implementation | Current validation |
| --- | --- | --- |
| Android API 28+ | Java TextView/Button/EditText/ScrollView, JNI, Taffy measurement | ARM64 and x86-64 APK builds; API 28 and API 36 emulator input, dialog IME and saved-task process recovery |
| Browser | Real HTML controls, DOM intrinsic measurement, Taffy placement | Edge 156, Firefox 157 and Playwright WebKit 27.2 controls, nested dialogs, focus, scroll, fonts and disposal; Chromium CDP IME, synthetic composition probes on other engines |
| iOS 14+ | Swift UILabel/UIButton/UITextField/UIScrollView, C callbacks | Rust ABI tests; Swift/Xcode compilation and device accessibility remain unverified |
| Windows | Win32 STATIC/BUTTON/EDIT/COMBOBOX, retained native child windows, WIC images, modal/anchored portals and OS text measurement | Local shared-page, Unicode/undo, incremental/queue/isolation, embedded child roots, image recovery, nested modal focus and anchored popover probes; physical IME, screen reader and complete component support remain pending |
| OHOS API 20+ native library / API 22 sample HAP | ArkUI NativeNode, Drawing measurement, UIAbility/NodeContent and NAPI callbacks; Tree/TreeItem role and selected-state mapping; checkbox, switch and radio checked-state mapping; expanded state exposed only as descriptive text | SDK20 ARM64/x86-64 native cross-checks compile; unsigned API22 HAP builds; mocked production C++ verifies checked/unchecked accessibility states; emulator runtime, accessibility-tree exposure and screen-reader output remain unverified; IDREF relations, Tabs and ExpandCollapse patterns are not implemented |

Run the Windows shared Rust page, bounded 100k-row editable list, or two-axis
100k-row × 1000-column generated-value table:

```powershell
cargo run -p jadix_native_controls --features windows --bin native-windows
cargo run -p jadix_native_controls --features windows,extended --bin native-windows
cargo run -p jadix_native_controls --features windows --bin native-workbench
cargo run -p jadix_native_controls --features windows --bin native-table
cargo run -p jadix_native_controls --features windows --bin native-tree
cargo run -p jadix_native_controls --features windows --bin native-dock -- --workspace D:/NativeDockWorkspace
```

The table toolbar reverses rows, filters even rows, resizes row zero and toggles
one frozen row and column. Four native scroll panes retain keyed editors while
synchronizing either header or leading-column scrolling with the body; frozen
mode hides scrollbars to preserve the full header height. `set_frozen(rows,
columns)` accepts multiple prefixes within the viewport and 256-cell budget.
`NodeStyle::scrollbars` defaults to both visible and currently controls Windows
scroll views; other adapters do not yet apply this display policy. Its
native probe also reverses columns, changes a column width and checks actual
native cell geometry, scroll anchoring, stable cell identities and undo. This
is an editable virtual-table slice. On Windows, a semantic container around each
mounted EDIT exposes UIA GridItem (zero-based live row/column, span one and the
containing Grid); the outer container exposes logical Grid dimensions and
GetItem for mounted cells across all four panes. Sorting/filtering updates these
coordinates while retaining keyed native editors and their Value/Text providers.
Invalid coordinates and unavailable mounted items are rejected. The provider
publishes count/coordinate/name changes; receipt by assistive technology is not
accepted. Native UIA clients can return zero for a removed item's scalar getter,
so a scalar value alone does not establish liveness. The tests also check owner
lookup, removed-item lookup and direct provider invalidation. The product enables
the table's optional column-header band: native Text controls follow the frozen
and scrolling columns, retaining business-key identity across reorder. UIA Table
and TableItem patterns expose mounted column headers and their matching cells;
row headers are absent. Disabling headers removes their controls and associations,
and re-enabling uses fresh node identities. An invalidated UIA client's header
lookup may become an empty array, while the direct provider rejects that lookup.
Offscreen item realization, VirtualizedItem/ItemContainer, selection ranges,
screen readers and the full DataTable component contract remain separate work.
Shared Grid metadata does not establish Android/OHOS/DOM grid support.
`focus_cell(row_key, column_key)` reveals a logical cell using measured axes,
frozen extents and the current sorted/filtered business keys. Windows bindings
provide Tab/Shift+Tab (row-major), Enter/Shift+Enter (vertical), PageUp/PageDown
and Ctrl+arrows/Home/End. At the first/last Tab boundary, focus returns to the
toolbar. Ordinary arrows/Home/End keep native EDIT caret and selection behavior;
Alt shortcuts and active composition bypass the grid binding. Programmatic
navigation also waits while any retained draft is composing. Local return keeps
the mounted editor and native undo; visiting an unmounted cell creates an editor
and does not reconstruct its native undo history. The Windows product probe
records posted key messages, keyed targets, viewport frames and native/model
focus agreement. Physical keyboard, actual IME interaction, other adapters and
screen-reader navigation require separate acceptance.
The tree sample has 100,002 business nodes with bounded native row mounting,
expansion, selection, lazy-load cancellation and generation checks. Its Win32
buttons support arrows, Home and End through an explicit composite-navigation
binding. Logical navigation reveals and focuses the actual retained row. The
real virtual row exposes Win32 TreeItem ControlType and Legacy selected state.
TreeItems expose actionable SelectionItem/ExpandCollapse UIA patterns; the full
logical virtual hierarchy and Tree container selection remain implementation
work. OHOS checked-state mapping now reaches the ArkUI accessibility-state object
for native checkbox, switch and radio controls, with a production C++ mock
contract for checked and unchecked values. It is not NativeNode or screen-reader
acceptance; arbitrary IDREF relations and native expanded/collapsed state remain
unsupported by the current built-in NativeNode API mapping.

The Dock product uses native tab buttons and retained multiline editors, with
split panels, floating native windows, layout undo/redo and bounded workspace
checkpoints. Drag a tab to another tab, a panel edge, a panel center, or outside
the application to reorder, split, merge or float it. Escape, capture loss and
unbinding cancel a pointer drag. The toolbar also provides explicit docking and
cancel commands. Current pointer capture is implemented for Windows; other
hosts report that this binding is unavailable.

Moving between panels in one window retains the editor HWND and native undo.
Moving between roots rebuilds the native editor and restores committed business
text; its native selection/undo state is not transferred. Active composition
blocks layout changes and window closure. Deferred close drains queued edits,
saves the workspace, and cancels closure on a save failure. A checkpoint replaces
the previous file through a synced sibling temporary file; reopening with the
same directory restores documents, tabs, splits and floating window bounds.
Recovery is from the last saved checkpoint; unsaved crash recovery, external
file watching, syntax/rich-text editing, resize handles and dedicated Tab/UIA
providers remain separate work. The probe tests an independent recovery process,
real native capture through programmatic mouse messages and close-time edit
draining; physical pointer/IME and screen-reader acceptance are still pending.

These executables accept `--probe NONCE NEW_REPORT_JSON`; messages are
programmatic, and reports retain that boundary.

The shared-page probe exports a redacted `inspection` tree with frames, node
identities, layout/cache counts, dirty nodes and resource budgets. Virtual
editors retain IME previews across relayout and commit their business values
when composition ends. The Android protocol-2 APK is compiled independently;
the JVM ledger harness checks its transaction rules without claiming Android
View or IME runtime acceptance.

Create and deliver an independent local consumer using the public portable
authoring API and the selected checkout's path dependencies:

```powershell
python scripts/native.py create D:/NativeApp --name native-app --output evidence/create.json
python scripts/native.py build --project D:/NativeApp --bin native-app --offline --output evidence/build.json
python scripts/native.py run --artifact evidence/build.json --output evidence/run.json
python scripts/native.py inspect --artifact evidence/build.json --output evidence/inspect.json
python scripts/native.py package --artifact evidence/build.json --output evidence/package.json
python scripts/native.py verify --package evidence/package-files/native-windows.zip --output evidence/verify.json
```

Outputs must be new. Build reports retain source and executable identities;
the workspace manifest and lock file are checked before and after compilation.
package verification checks all members and the build report. These commands
produce unsigned Windows development delivery, with publisher authentication,
redistributables, installers and registry consumption still separate.

Build and inspect an OHOS native library or unsigned HAP through the same retained
artifact workflow. Choose one installed SDK explicitly; hashes and compiler
success are not device or runtime acceptance:

```powershell
python scripts/native.py build --platform ohos --sdk 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony' --architecture x86_64 --offline --output evidence/ohos-api22.json
python scripts/native.py build --platform ohos --sdk 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony' --architecture x86_64 --features popover --offline --output evidence/ohos-popover-api22.json
python scripts/native.py build --platform ohos --sdk 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony' --architecture x86_64 --features selection --offline --output evidence/ohos-selection-api22.json
python scripts/native.py inspect --artifact evidence/ohos-api22.json --output evidence/ohos-inspect.json
python scripts/native.py build --platform ohos --sdk 'C:/Users/JUHAI-ZK/AppData/Local/OpenHarmony/Sdk/20' --architecture arm64 --native-only --offline --output evidence/ohos-api20-arm64.json
```

The build freezes the SDK metadata, target ABI, workspace manifest and lock file,
shared source dependencies, native library and optional unsigned HAP identities.
The OHOS page now includes the shared controlled editor dialog and nested
confirmation. Its host retains logical portal order separately from native child
indices, blocks automatic dismissal until the Rust owner accepts the event, and
advertises Dialog, Popover and Select only when the runtime provides ArkUI_NativeDialogAPI_1.
The optional Popover product uses the existing shared profile editor, nested
options and anchored panels. Its controllers use nonmodal mode; shared layout
computes window coordinates, viewport flipping and anchor dependencies. Build
reports retain the requested feature set and reject a substituted product.
Generated projects use their own manifests and cannot accept sample feature
overrides. Hosted Rust ABI tests exercise ordering, positioning, removal,
dismiss revisions and queue ownership with a mock native ABI. They do not execute
ArkUI or the C++ callback. Actual display, nonmodal background pointer routing,
Back/outside dismissal, IME and focus restoration remain unverified. The Selection
product composes a retained native Button trigger with native option buttons in a
Scroll/Column and modal NativeDialog. It closes before owner overlay stack changes
and during option changes, removal or disposal; unrelated style publication keeps
an unchanged popup. ABI tests cover controlled Unicode options, revisions, disabled
and unknown event rejection and publication suppression. Six host scenarios also
execute production C++ creation, registered callbacks and disposal with installed
SDK headers and a mocked ArkUI API. They cover theme updates, large-font row
height, disabled-skipping/wrapping Up/Down, Home/End, scroll-to-focus, Escape,
close-before-selection and partial-failure cleanup. API20 C++ checks and the API22
x86_64 unsigned Selection HAP build pass. Actual ArkUI popup interaction, native
Enter/Space activation, keyboard dispatch, focus and accessibility remain unverified.

The sample and generated OHOS page include an on-demand inspector. Open Inspect,
then Refresh to capture the mounted root, node frames, dirty nodes, layout and
resource budgets. Show text explicitly opts into ordinary text; password values
and placeholders remain hidden. Closing the panel resets that opt-in. The
root-checked nativeInspect NAPI entry also reports Rust host control counts,
overlay order and bounded queue usage without polling events or exposing queued
payloads. The panel does not refresh on a timer and the event drain no longer logs
business state. These are model and adapter observations; the ArkUI accessibility
tree, actual visibility, input routing and assistive technology require separate
runtime inspection. Artifact inspect remains a package identity check.

The OHOS library cannot be run with the Windows probe or packaged by the Windows
ZIP command. Builds install nothing and establish no ArkUI, accessibility or
screen-reader runtime acceptance.

Build the Android debug sample on Windows with `ANDROID_HOME`, `NDK_HOME` and
`JAVA_HOME` configured. The script currently uses Android platform 36,
build-tools 36.1.0, JDK 17 and the NDK Clang driver for API 28:

```powershell
python scripts/native.py build --platform android --android-sdk "$env:ANDROID_HOME" --ndk "$env:NDK_HOME" --java-home "$env:JAVA_HOME" --android-target x86_64-linux-android --offline --output evidence/android-x86_64.json
python scripts/native.py inspect --artifact evidence/android-x86_64.json --output evidence/android-inspect.json
python examples/native-controls/build-android.py --target aarch64-linux-android --output target/native/android
adb install target/native/android/native-controls.apk
adb shell am start -n dev.jadix.nativeui/.MainActivity
```

The sample APK is debuggable and signed with an output-local debug key. It also
contains `NativeChecks`, an Android instrumentation runner for input connections,
state updates, scrolling, Activity recreation and stale-root rejection. Product
packaging must supply its own application, signing and release policy.
The builder records workspace, framework, and sample source identities and rejects
source changes observed during compilation or packaging. It also records selected
SDK, NDK and JDK tool identities and rejects tool or native-library changes during
packaging. Unified inspection checks the retained build report, whole APK hash,
unique target library, packaged library hash and ELF ABI. Inspection does not
rerun Android signature verification. Its debug signature is
not publisher authenticity or release evidence; APK creation does not establish
device runtime or accessibility acceptance.

To create and exercise a separate Android consumer with its own application ID:

```powershell
python scripts/native.py create D:/NativeAndroid --name native-app --platform android --output evidence/android-create.json
python scripts/native.py build --platform android --project D:/NativeAndroid --android-sdk "$env:ANDROID_HOME" --ndk "$env:NDK_HOME" --java-home "$env:JAVA_HOME" --android-target x86_64-linux-android --offline --output evidence/android-project.json
python scripts/native.py run --artifact evidence/android-project.json --adb "$env:ANDROID_HOME/platform-tools/adb.exe" --serial emulator-5580 --output evidence/android-project-run.json
python scripts/native.py package --artifact evidence/android-project.json --output evidence/android-project-package.json
```

The generated README describes checkpoint limits and ownership cleanup. Its runner
checks programmatic native edits, retained controls, checkpoint rejection/restoration
and retired roots. Add `--process-recovery` to exercise a separate activity task:
edit the native form, background it, kill its background process, confirm the old
PID disappears, and reopen the retained task with a new PID and the same business
values. This checks Activity saved-task restoration; disk journaling, power loss,
physical input, accessibility and production release require separate evidence.

Without `--project`, the unified Android command builds the fixed controls sample
and requires explicit tool paths. It accepts `--extended` for dialog, switch and editor probes and
`--large-list` for 1000 retained rows. Use the standalone builder for selection,
popover or notes products. `--release` optimizes Rust while retaining a debuggable development
APK. Unified `run` reuses the maintained Android acceptance runner with an explicit
ADB executable and device serial:

```powershell
python scripts/native.py run --artifact evidence/android-build.json --adb C:/Android/Sdk/platform-tools/adb.exe --serial emulator-5580 --process-recovery --output evidence/android-run.json
```

It verifies the selected serial, supported APK ABI and API level before installation,
binds a fresh nonce to the APK, runner and device receipts, and requires owned-install
cleanup to restore the package registry. The command retains discovery and child
logs on failure. Emulators, physical input, TalkBack and production signing have
separate acceptance boundaries. Unified `package` and `verify` also retain Android
APKs and OHOS native-only or unsigned HAP builds in portable ZIPs, together with
both build reports and their hashes. The ZIP is unsigned; the embedded Android APK
keeps its development signature. Verification works without the source checkout
and rejects payloads whose hashes disagree with either report. OHOS consumers can
also be created and built independently:

```powershell
python scripts/native.py create D:/NativeOhos --name native-app --platform ohos --output evidence/ohos-create.json
python scripts/native.py build --platform ohos --project D:/NativeOhos --sdk "C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony" --architecture x86_64 --offline --output evidence/ohos-project.json
python scripts/native.py inspect --artifact evidence/ohos-project.json --output evidence/ohos-inspect.json
python scripts/native.py package --artifact evidence/ohos-project.json --output evidence/ohos-package.json
```

The generated UIAbility mounts a Rust form through ArkUI NodeContent. Build and
inspection bind its bundle ID and ability to the generated configuration, verify
the NAPI library ABI, and freeze the consumer, framework and lockfile identities.
An API22 SDK produces an unsigned HAP; `--native-only` also accepts API20.
OHOS `run` and mobile release packaging remain implementation gaps. HAP
compilation does not establish target behavior.

The Windows `native-inspector` product opens a sample application and a separate
native tool window. Select a mounted node to correlate its presentation identity,
layout, last commit, resource limits and actual HWND class, geometry and state.
The tool follows application edits and suppresses unchanged inspector commits.
Body text is hidden until explicitly enabled; password values remain hidden.
The selector is limited to 256 mounted nodes and reports omitted nodes. This is
same-process presentation and Win32 observation; UIA and assistive technology
acceptance remain separate.

```powershell
python scripts/native.py build --platform windows --bin native-inspector --offline --output evidence/inspector-build.json
python scripts/native.py run --artifact evidence/inspector-build.json --output evidence/inspector-run.json
cargo run --offline -p jadix_native_controls --features windows --bin native-inspector
```

The maintained Android acceptance runner requires the matching build.json and
refuses an existing package, including retained uninstalled registrations. It
installs without replacement and removes only an install that reported Success,
on both successful and failed checks. Pass --keep-installed only when a following
performance check needs this run's successful sample; failures still clean it.
UI tree dumps use fresh temporary paths and are removed after collection.

Build the browser sample with `wasm32-unknown-unknown` and `wasm-bindgen-cli`
matching the `wasm-bindgen` version in the workspace lockfile:

```text
python examples/native-controls/build-web.py --output target/native/web
python -m http.server 8080 --directory target/native/web
```

Open the local server in a browser. Controls remain inspectable DOM elements;
the sample uses no Canvas. The browser adapter preserves unchanged input values
and native scrolling. Font completion and viewport changes invalidate platform
measurements. The current composition range is inferred from the selection at
composition start and the current caret because HTML input does not expose the
platform IME marked range directly. Physical IME and screen-reader checks remain
separate from automated browser protocol scenarios.
Each mount reads the current container bounds, including after disposal and
replacement. Native Tab/Shift+Tab checks cover all eight extended controls in
authored order, complete focused-control visibility at compact viewport height,
and navigation after 25 mount/dispose cycles. A known installed Chromium browser
can be selected explicitly when Playwright channel discovery cannot find it.
The notes workflow and scale runners support the same browser selection.

The tested WebKit 27.2 build caps assigned single-line values at 524288 UTF-16
code units, even with a larger `maxlength`. The adapter reports a platform error
if a published controlled input or textarea value is not represented exactly.
This does not enlarge the shared UTF-8 event budget. Browser button activation
explicitly focuses its control before dispatch, so native dialogs can restore
their trigger across pointer and keyboard dismissal paths.

On macOS, install the Rust `aarch64-apple-ios-sim` target and use Xcode's selected
SDK to build the Swift host and Rust static library:

```text
bash examples/native-controls/build-ios.sh aarch64-apple-ios-sim target/native/ios
```

For a device, select `aarch64-apple-ios` and configure signing/provisioning in the
host application. The script can sign with `CODE_SIGN_IDENTITY`; it does not
create provisioning profiles. The sample handles keyboard overlap and Dynamic
Type notifications. Compilation, lifecycle behavior and VoiceOver require Apple
tools and devices before this host can be considered validated.

Root disposal precedes Owner release. Android Activity recreation saves committed
business values, creates a new root identity and rejects callbacks from the old
root. Composing drafts remain platform input state and are not saved as committed
business values. A failed platform publication clears the root; the application
must choose a recovery UI or construct a fresh root.

The shared sample accepts saved JSON up to 2 MiB, enough to round-trip its two
editable fields at the event protocol's individual 1 MiB envelope limit. Android
writes committed business state to a per-Activity `AtomicFile` in private files;
the lifecycle Bundle carries its UUID, keeping large text out of Binder. Normal
Activity finish deletes that file. Missing, malformed or unreadable state reports
an error and opens a fresh usable root. This is lifecycle recovery rather than a
durable document repository. The recovery cache permits at most 16 identities
and 16 MiB in total, with the existing 2 MiB per-state ceiling. Atomic temporary
and backup files count toward the cache inventory; a replacement has at most
one additional per-state payload allowance during writing. Quota errors keep
the previous saved value and report the failed save. Writes reclaim only owned
canonical-UUID cache files older than seven days; active Activity leases and
the identity currently being written are protected. Closing the final lease
allows later expiry. This bounded recovery cache must not store durable user
documents. A saved task older than the retention window can open a fresh page
with an explicit recovery error if its cache was reclaimed.
The native subtree does not also save a second copy of editor state in the Bundle.
Android explicitly removes the platform's implicit single-line length filter;
the wire resource bound still applies. Rejected native callback events restore
the last published control value and expose an error through `NativeView.takeError()`.

Supported authoring includes text, rectangular flex containers, buttons, single-line
text input and one vertical scrolling axis. Portable Textarea, Checkbox, Switch and Image
are available through matching Builder/RSX entry points. Pass `--extended` to the
Android/Web build script to include their demonstration page; the iOS build includes
extensions. Android recreates committed
checkbox/note state alongside the base business values. The Airplane mode Switch
and Accept Checkbox share one checked Signal, demonstrating native-to-Rust
round trips and controlled synchronization between two retained controls. Android
uses android.widget.Switch; DOM uses a native checkbox input with role="switch".
UIKit has a corresponding UISwitch/caption mapping awaiting Apple validation.
Lists retain all rows.
Images currently accept only bounded embedded PNG data URIs. Their declared
dimensions and resource aspect ratio remain stable when decoding fails; a gray
placeholder and the alternative text remain available. The extended page includes
one valid and one corrupt image. The extended page also demonstrates a native
Dialog and nested Dialog with the same input Signal, deferred retained content,
scrolling body, contextual close and keyboard dismissal. Android routes modal IME
insets back to viewport layout; DOM contains Tab navigation and restores trigger
focus. UIKit has corresponding UIWindow source awaiting Apple validation.

Android extended acceptance also runs a dedicated eight-control Tab/Shift+Tab
probe with the soft keyboard suppressed. It checks native focus, full visibility
and unchanged Unicode business state in initial/compact viewports and after
25 remounts, at 100 and 1000 rows. IME and Dialog retain their separate original
checks. The keyboard probe emits fresh-token structured receipts and a native
screenshot; injected keys do not establish physical keyboard or TalkBack support.

Extended Android acceptance runs a second native editing probe with independent
fresh tokens: Unicode Input/Textarea copy, cut, paste, undo/redo, history after
publication and same-parent reorder, and controlled-reset history clearing. It
also verifies password paste, blocked copy/cut and public-state isolation.
Clipboard content is restored and native screenshots/structured receipts are
retained. These injected shortcuts run with soft input suppressed; physical IME
and cross-parent history require separate validation.
Custom overlay color/opacity/blur, panel classes and hug-content Dialog styles
return an explicit unsupported error. Remote images, richer selection
controls and virtualization require further component work. These platform implementations do not enable arbitrary GPU styles
or a DOM CSS authoring API. When switching APK output directories, use
`--keystore PATH` to reuse the existing debug signing key for an update.
An explicit keystore path must already exist; the builder only creates a new
development key inside its mutable output directory.

Use Python 3.11 or newer for the build scripts. `build-web.py` checks the
wasm-bindgen CLI version against Cargo.lock before building. Both scripts resolve
artifacts through Cargo metadata, including a configured target directory, and
write `build.json` with the profile, lockfile and binary hashes. Android compiles
Java into a fresh temporary directory on each build so removed classes cannot
survive in a reused output directory. An output directory below a frozen
`snapshot.json` is rejected.

Pass `--release` to either script to optimize the Rust library. Android still uses
the sample's debuggable manifest and development signing key; production manifests,
application identity, provisioning and release signing belong to the host product.
The Web host exposes `jadixNative.take_error()` to consume the latest rejected
native event. Event errors return through the callback without throwing across
Rust borrow guards; later valid edits and clicks can continue. A failed platform
publication still requires a fresh root.

The sample retains 100 rows by default. Pass `--large-list` to the Android or Web
build script to exercise the same controls with 1000 retained rows. The build
report records this choice; it does not enable list virtualization.

The separate Web, Android or iOS `--selection` page and OHOS `--features selection`
page demonstrate Select with the same Rust Signal, Unicode option labels,
a disabled option/control, controlled reset and an
independent counter. It reports zero retained list rows and cannot be combined
with `--extended`, `--large-list` or `--notes`. Its Cargo `selection` feature also
excludes `gpu` and other sample page features. Android packages this page separately
as `dev.jadix.selection`, with a native Spinner and dropdown. The iOS builder emits
`JadixSelect.app` with the same isolated identifier and `uikit,selection` features.
Its UIButton/UIMenu source supports controlled labels, disabled actions and
retained menu identity, awaiting Apple compilation/runtime validation. Optional
`--checks` compiles a separate programmatic Select probe; the simulator runner
requires its exact ten checks, two controls, three options, Dynamic Type metrics
and explicit programmatic event authority. These probes do not establish native
menu touch, keyboard or VoiceOver acceptance. The primary selector now uses basic
SelectTrigger/SelectValue/SelectContent/SelectItem RSX; the disabled selector keeps
the options Builder. Build reports record selection_authoring accordingly. Grouped
or rich items, compound root default/disabled/open flags and custom popup styling
remain outside this subset; unsupported structures/options fail projection.

The isolated Android/Web `--popover --release` page exercises Builder and compound RSX
against the same Rust model: nested native top-layer panels, Unicode editing,
contextual close, independent panels and scrolled anchors. It cannot be combined
with other page flags or gpu. Build reports require popover=true and zero retained
rows. Browser-generated touch, pointer and keyboard actions are distinguished
from native-API/DOM probes in its recorded scenarios. Android uses the separate
dev.jadix.popover package and real PopupWindow controls. Its
instrumentation runner checks APK/SO hashes,
requires an absent registered package, validates fifteen native scenarios and
removes only its own install with a complete package-registry comparison. API 28
and 36 AVD probes cover native InputConnection, independent/nested panels,
scroll/flip, keyboard dismissal, disposal and Activity recreation. Native edit,
scroll and resize calls are explicitly programmatic; this does not establish
physical IME, TalkBack or production signing.

The iOS builder also accepts --popover, selecting uikit,popover and the independent
JadixPopover.app / dev.jadix.popover identity. Its retained UIView overlay and
native editor mapping await Apple compilation and runtime validation. Optional
--checks includes a separate twelve-scenario programmatic probe for anchors,
Unicode/preedit, controlled reset, nested/opening order, scroll/flip, disposal
and fresh-root restoration. Receipts require exact application identity, check
names, geometry and programmatic event authority. Local Rust C ABI and offline
tool tests validate engineering paths; they do not establish UIKit execution,
touch, hardware keys, physical IME or VoiceOver.

The separate **Jadix Notes** product sample uses `--notes` with either build
script. Its `notes` Cargo feature selects the same Rust notebook workflow for
Android and Wasm; it cannot be combined with the controls demo flags. The Android
application identity is `dev.jadix.notebook`, separate from the controls sample.
The iOS builder also accepts `--notes`, selecting the same Rust workflow and a
Swift document host. This UIKit notebook source has no Apple compilation or
runtime acceptance in the current environment.

Notes supports multiple saved documents, Unicode titles and multiline text,
search, opening saved notes, explicit Save, guarded navigation while a draft is
dirty, Revert, confirmed deletion, and versioned JSON export/import. The JSON
transfer field lets users copy a backup or confirm replacement of the notebook.
It permits 128 documents, 256 UTF-8 bytes per title, 32 KiB per body and 512 KiB
per encoded notebook. Empty titles, duplicate identities, unsupported schema
versions and oversized data are rejected before replacing saved documents.
Browser checks can select Chromium, Firefox or the Playwright WebKit build and
record the observed engine version. WebKit execution does not establish Safari
application support. Semantic snapshots supplement the native controls; they do
not establish operating-system screen-reader or physical IME acceptance.

Android stores committed documents in a private atomic file without the recovery
cache expiry policy; normal Activity finish keeps these documents. Its separate
bounded recovery cache may restore an unsaved draft only when its archived
documents still match the durable notebook. The browser stores committed
documents in origin-local `localStorage`; unsaved drafts remain transient.
Save success appears only after the host acknowledges storage. Storage failures
remain visible, retain the previous document and allow retry or export. Malformed
stored bytes are preserved until the user explicitly confirms a valid Import.
Observed stale writes from another window/tab are rejected; this is not an
atomic browser multi-tab compare-and-swap guarantee. Storage can be removed by
the OS, browser or user; keep exported backups for important notes. These hosts
do not provide encryption, cloud sync, power-loss certification or release signing.

On macOS, `build-ios.py` builds controls, notes, selection or popover for an
ARM64 simulator or device. `build-ios.sh` keeps the target/output positional
interface and forwards further arguments to that builder. The builder resolves
the Rust library through Cargo metadata, rejects archived or existing bundles,
and records SDK/compiler identities and hashes in `build.json`. Simulator
bundles use ad-hoc signing. Device bundles remain unsigned unless
`CODE_SIGN_IDENTITY` is configured; an optional existing `--provisioning-profile`
is copied into the bundle. Signing verification does not establish distribution
or device-runtime acceptance.

The UIKit source connects state/restore and storage acknowledgements through the
C ABI. Notes documents use a bounded, protected Application Support file with
atomic replacement, UTF-8 validation, read-back and stale-writer checks. The
Swift source preserves unavailable documents until confirmed Import and keeps
drafts in memory. These policies and UIKit edit-error rollback still require
Apple compilation, lifecycle, input and VoiceOver validation.

Simulator builds may opt into `--checks` to include programmatic UIKit probes.
These probes are excluded from normal builds and rejected for device builds;
they do not establish physical input, VoiceOver or distribution support.


# iOS keyboard accessory

Pass `--consistent-ui` to `build-ios.py` to enable the optional `jadix_native/consistent-ui` feature and attach an iOS-localized Done toolbar to native text inputs and text areas. The button ends editing through UIKit so normal blur events are delivered. This option does not replace the operating system keyboard or change the default build.

