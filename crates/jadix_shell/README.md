# jadix_shell

This is the optional script/session/distribution layer. It consumes `jadix` for GPU UI and `jadix_native` for system views, sharing the existing presentation contracts. General UI capabilities must remain usable from Rust without this VM.

## GPU SDK host and HTTPS updates

`gpu-host` mounts the existing SDK and QuickJS `ScriptApplication` into the
ordinary Jadix App. All twelve presentation variants now have GPU mappings:
Container, Text, Button, Input, Textarea, Scroll, Checkbox, Switch, Image, Select,
Dialog and Popover. Editors use the shared text/IME engine; Scroll and overlays
use the shared engine state; Select uses the existing component implementation.
Scroll offsets, selections, toggles and dismissal origin return through the same
session event handles. Removing a host unregisters its overlay state.

Animated portal opacity/backdrop composition uses the ordinary window compositor.
The offscreen `JadixScene` seam currently rejects that pass with `Unsupported`;
its static GPU pixels are validated separately from window overlay interaction.
Dialog backdrop dismissal is opt-in (`close_on_overlay_click: true`), matching
the shared presentation protocol.

Text supports the shared single-line clip/ellipsis policy and TextSpan foreground,
background, weight, italic and underline. Multi-line clamping and inline
strikethrough and rich-span ellipsis return explicit host errors. Image accepts the bounded PNG data URL
contract. `window_app --bundle PATH --width 640 --height 480` loads a development
bundle; production hosts load an authenticated package through `from_package`.

The presentation SDK also accepts `ellipsis_start` and `ellipsis_middle` for
AppKit labels. GPU and terminal hosts render these values as tail ellipsis.

Feature `native-https` adds `ota::download_https_release`, composing the explicit
installed-curl transport with existing signature, resource digest, release floor,
durable trial, health and rollback checks. Configure trusted release keys and the
HTTPS origin in the host. Private TLS acceptance uses development keys and a
private CA; it does not provision a production signing service or OS installer.

`application::ScriptApplication` runs SDK bundles in the existing budgeted QuickJS
VM (`native-vm`), sharing Session authorization, event handles and keyed Mount
with native and terminal hosts. Build the real TSX fixtures with
`node sdk/scripts/build-application.mjs` from this crate. `from_package` verifies
the signed package before executing its entry resource; `development`
is for explicitly trusted development code. No browser/process/network globals
are installed automatically.

`gpu::GpuApplication` (`gpu-host` plus a renderer backend) maps Container, Text,
Button, Checkbox, Switch, Input and Textarea into the ordinary Jadix window App.
Editors reuse the existing text/IME engine. Unsupported component mappings fail
explicitly. Try `cargo run -p jadix_shell --features backend-dx12 --example window_app`
on Windows. `jadix_tui::ShellApplication` provides the character host using the
same VM and SDK; it requires the TUI `shell` feature.

`publish` delivers complete snapshots. `publish_commits` generates keyed
Create/Update/Move/Remove operations and accepts bindings only after host success.
The two publisher modes cannot be mixed. Full mounts still use a whole-tree
keyed diff. Validated Text/Button leaf packets instead emit only addressed
updates, and idle publication does not clone the retained snapshot. A rejected
packet restores only touched contents and keeps its place in the queue; accepted
no-ops do not consume a host revision. Host layout/output costs remain separate,
so this is not end-to-end O(k).

With `native-store`, `ota::DownloadSource`, `Trial` and `Launch` add authenticated
download, durable next-launch trial, first-frame/ten-second health acceptance,
clean-quit acceptance and fallback to the previous healthy signed bundle. The
host supplies the existing jadix_net transport and must enforce TLS, allocation
bounds and cancellation there. Tests use an explicit private loopback HTTP
fixture; this is not an OS whole-application installer.

See the current validation and limitations.

Optional `native-store` adds a filesystem package store on native targets. It
keeps signed manifests and resources in immutable content-addressed directories,
verifies signatures, hashes, permissions, target and version policy again on
every load, and holds an OS file lock for the store's lifetime. A second writer
fails instead of racing an activation. This feature currently uses the Rust
standard library file-lock API and is exercised with Rust 1.95 on Windows.

Use `PackageStore::stage` to persist an unconfirmed candidate, then `candidate`
to load it into a separate session. The application must validate that session's
startup and native mount before calling `accept`. Keep the current session until
acceptance succeeds; use `reject` after failed initialization. Accept replaces a
synced activation pointer and records the previous healthy package. Process exit
before acceptance leaves the previous activation selected; `open` clears that
pending attempt, reports `recovered_pending()`, and revalidates the selected
package. `previous` lets the host initialize a rollback candidate before the
explicit `rollback` switches the pointer. Rollback retains the highest accepted
release, so ordinary staging cannot silently downgrade or replace the same
release with different content. Revocation and the host's minimum release still
apply to rollback and restart.

The store allows eight object directories, up to 16 MiB resources and 256 KiB
manifest per package. Call `collect_unused` to remove unreferenced completed
objects; active, previous, pending and highest accepted identities stay retained.
Abandoned temporary directories are retained and count toward the object budget.
Optional business checkpoints hold at most 256 KiB of JSON per package identity.
`save_business` saves the healthy version; `business` and `previous_business`
read checkpoints after package verification and validate their identity,
namespace and storage version. Checkpoint values are unsigned application data;
the host must validate their schema before using them.

Use `candidate_business` to obtain the candidate's startup value. Equal storage
versions copy the healthy checkpoint. Different versions require the signed
manifest's `migration_from` declaration and an explicit host migration callback;
namespace changes and missing declarations fail before invoking that callback.
Initialize and validate a separate session with the result, then call
`accept_with_business`. It writes the candidate checkpoint before replacing the
healthy activation pointer. Failed initialization or pointer replacement leaves
the previous version selected. Rollback selects that version's own checkpoint;
it does not reverse-migrate edits made under the newer version. Collection also
removes checkpoints for unreferenced identities.

Business writes first publish a synced `business-journal.json`, bounded to
257 KiB, containing the complete checkpoint and a SHA-256 checksum over its
schema, activation revision and data. Checkpoint replacement completes before
the journal is removed. After a failed replacement or process exit, `open`
replays the journal before clearing an abandoned activation and reports
`recovered_business()`. Replay checks the exact activation revision, current or
pending package identity, verified signed package, namespace and storage version.
Malformed, oversized or mismatched journals fail closed and remain available for
diagnosis. An outstanding journal blocks activation changes and collection;
retrying a business save first recovers it. Recovery of a candidate checkpoint
does not activate that candidate or lower the accepted release floor. The checksum
detects corruption; business values remain unsigned and unencrypted. Only one
pending checkpoint journal is retained, rather than an unbounded edit history.

The Windows TSX workspace saves committed edits after 250 ms of quiet, with a
two-second maximum delay, and checkpoints again on close. Composition pauses
autosave; errors retain the pending value and retry after five seconds. Idle
workspaces disarm the timer. The window stays open if closing cannot save or if
composition is pending. Edits before the save deadline have no durable journal.
Its probe also leaves a real failed checkpoint write in one process, exits,
and verifies journal replay into another process's native EDIT. This exercises
process exit and a controlled filesystem obstruction; sudden power loss and
an exhaustive set of crash instruction boundaries remain unverified.
Its development-signed update declares schema 1 → 2 and transforms
`{draft, count}` into `{profile: {name}, counter}` through the candidate's
budgeted `migrateStorage` export. The product probe verifies failed-migration
isolation, schema-2 native editing state in another process and rollback to the
schema-1 checkpoint. This does not establish production update distribution.
Update downloads, trusted-key distribution and protection against an attacker
rewriting the application's own activation state require host policy. File sync
and atomic replacement are tested with process termination; power-loss
durability and Android/OHOS filesystem locking have not been accepted.

Experimental Rust session contract. Uses `jadix_core` directly and does not
depend on `jadix_native`, the GPU framework or a browser runtime. The default
feature set has no JavaScript VM; optional `native-vm` enables a bounded QuickJS
execution probe on native targets.

A session checks the protocol manifest, accepts explicitly granted permissions,
creates one reactive Owner while loading, then activates. Host request admission
checks registration, grants, typed arguments and the caller-supplied pending request limit.
Completion consumes a session-bound request exactly once; cancellation, fault and
disposal revoke late results. Fault and disposal release the reactive Owner.
The host registers a `schema::MethodContract`, calls `request_with` before starting
a task and `complete_with` before delivering its result. Invalid results leave the
request pending so that the host can correct or cancel them. The `register`,
`request` and `complete` convenience methods use null arguments/results.
Cancellation revokes delivery; external side effects need their own
cancellation mechanism. The session stays on the reactive UI thread.

After loading, `mount_root` creates a fresh session-owned `RootLifetime`. Pass
clones to the presentation participants using their `with_lifetime` constructors.
`unmount_root` revokes that root and its `request_for_root` requests while keeping
sibling roots and session-wide requests active. Fault, disposal and session drop
revoke all registered roots before releasing the Owner. External root revocation
also rejects late results and frees their request capacity on the next operation.
Mounts provide logical lifetime ownership; they do not create platform views,
child Owners or grant extra method permissions. `ResourceLimits` bounds requests,
mounts and exported handles before admission. State handles refer to existing
Rust signals; event handles carry typed schemas and reject cross-session access.
Release, unmount and disposal also revoke already queued callback deliveries.

Schemas support null, boolean, finite number, bounded UTF-8 string, array and
objects with required/optional fields. Unknown fields are rejected. Validation
bounds depth to 32, visited values to 4096 and aggregate value/key bytes to 1 MiB.
The `wire` module defines strict JSON envelopes, component trees and decimal-string
identities. Duplicate keys, unknown fields, nonfinite values and oversized messages
are rejected. Product component schemas live in one JSON source; the local
[JavaScript/TSX SDK](sdk/README.md) generates its definitions from that source.

`Session::verify` checks the session manifest only. Hosts separately use
`package::SignedManifest::verify` for Ed25519 signatures, SHA-256 resource digests,
trusted/revoked keys, release floors, compatibility and permission policy before
loading. Package verification does not grant permissions or activate an update.

`runtime::native::BudgetedRuntime` provides memory, stack, execution-time and
microtask limits. It does not install Node, DOM, network or application services.
Feature `native-host` supplies `native::NativeMount` for real `PlatformHost`
implementations. Validated trees retain identities by sibling key and component;
unkeyed children retain positional identity. Removing a node or changing its
component remounts its subtree. The presenter maps the twelve shared portable and extended schemas,
preserves input preedit through unrelated renders, and reconciles rejected
controlled edits with a fresh revision. Events use session-exported handles;
revocation also invalidates already queued deliveries. `PopoverContent` binds an
anchor key to one keyed node across tree moves, resolves forward references,
rejects absent/ambiguous/descendant anchors before native publication, and maps
dismissal back to the session-owned TSX event. AppKit also delivers `cancel`
with `backdrop: true` for an outside press and `backdrop: false` for Escape,
including attempts blocked by the dismissal policy. `positionChange` carries
the resolved `position` with frame, placement and available size. The payload
is empty when a previously positioned panel loses its visible anchor.
`PopoverContent` accepts `margin` in whole points and `close_on_escape`
independently of `close_on_outside_click`. On AppKit, `events.open` and
`events.close` observe committed state changes on a mounted panel.
`style.popover_enter_ms` and `style.popover_exit_ms` set AppKit transition
durations. `style.popover_starting` and `style.popover_ending` accept
`opacity`, uniform `scale`, an `offset` in right/down points, an optional
RGBA `background` and a `corner_radius` in logical points. Unset endpoint
backgrounds retain the panel background.
Omitted endpoints fade, and omitted durations switch immediately. AppKit honors
Reduce Motion. The open event waits for a visible anchor. The close event
follows the transition when the panel stays mounted.
Hosts without popover capability
reject the candidate without publishing it.

The Windows `native-tsx` product compiles actual TSX into an IIFE, executes it in
QuickJS and connects it to retained Win32 controls. Its probe verifies native
editing, keyed movement, undo, permission refusal, late-event cancellation,
keyed popover reanchoring and native dismissal returning to controlled TSX state.
Development-signed packages exercise failed-update preservation, replacement
and explicit rollback with versioned business checkpoints. Publisher credential
management, timers/downloads, GPU panels and Android/OHOS script products remain open. This crate targets trusted
first-party scripts and does not provide an OS security sandbox.

With `native-vm,native-host`, `native::NativeApplication` connects the same
budgeted `ScriptApplication` and bundled SDK to an existing `PlatformHost`.
Call `publish(viewport, environment)`, poll validated native events with `poll`,
then publish again; Drop revokes the shared lifetime and disposes the host.
Native layout still processes snapshots; native batch publication retains keyed
identities. This adapter's headless tests do not establish device acceptance.

`ScriptApplication::publish_commits` skips empty host batches while accepting
their event bindings. Its return value counts accepted SDK mounts, including
noops; resize handling must track actual host calls separately. Publication
failures retain queued mounts for ordered retry, provided the rejecting host
leaves its live tree unchanged. Disposed roots reject further publication.

GPU editor undo/redo is available with `text-undo`, forwarding to the existing
Jadix history engine. `GpuApplication::dispose` and Drop revoke script callbacks
before releasing overlays and scroll resources. Hosts must also remove their
old Element tree from painting/input; retained action callbacks become inert
after application disposal or destruction of their host signal owner.

For windowless GPU rebuilding, use
`renderer.replace_root(&mut root, application.element()?)` before rendering.
This reuses the ordinary window App's keyed reconciler. After rendering,
`renderer.last_frame_stats()` provides resident layout counts and CPU
layout/collect/render/readback timings; failed frames return None. Full trees
are still visited. These metrics are not GPU timestamps or an O(k) guarantee.

For repeated windowless SDK updates, use
`application.update_root(&mut renderer, &mut root)?`. Its first call indexes a
full projection. Later plain Text/Button leaf changes stage replacements and
reconcile only those leaves; unchanged updates project zero SDK nodes. It also
requests the renderer's retained layout and fingerprint/geometry-validated
display-list replay. Structure, container/control changes, invalid paths and
mixing in a fresh `element()` fall back to full projection. Failed frames,
viewport/scale changes and subsequent replacements cancel replay. Keep the root,
renderer and signal owner together; remove the tree when the host is disposed.

`ProjectionStats` counts source SDK nodes visited/projected, not all generated
Elements. Full SDK mounts, inherited-style/geometry and parent fingerprint
combination still have tree costs; this is not end-to-end O(k). `collect_replayed`
counts cached subtrees, not eliminated output bytes. The window App's `run`
builder still uses `element()`; these timings apply to `update_root` consumers.

For sparse text/button updates, send `app.patch([{path:[1],props:{text:'Next'}}])`
from the SDK instead of reconstructing and mounting the whole tree. Paths are
child indices in the currently ordered SDK mount, excluding the GPU wrapper.
Packets replace all props of existing Text/Button leaves; they cannot change
structure, styles, keys or events. A packet accepts at most 1024 distinct paths
of depth at most 32. Invalid/stale paths, wrong properties and released handles
are rejected before host publication. A rejecting host restores changed leaf
contents and leaves the packet queued for ordered retry. Full mount remains the
route for structural/control changes. Do not catch a rejected patch and assume
it applied; publication still needs host acceptance.

The GPU host consumes the accepted path journal without rescanning the source
tree. `SceneRenderer::replace_leaf` invalidates those paint-hash ancestor paths,
allowing unchanged static subtrees to reuse fingerprints. Indexed event lookup
also avoids rebuilding a node map for each event. Ordered maps still have
logarithmic lookup costs, and flat parent hash combination/geometry/output still
scale with N. Packet byte counts and CPU stages are exposed by `work()` and the
renderer statistics; renderer submission/readback is separate from SDK work.

`GpuApplication::retained_window_element(Rc<RefCell<_>>, observer)` provides the
ordinary window App with the same sparse packets through its existing in-place
binding runtime. Borrow conflicts retry at idle. Container/Text/Button and
Checkbox/Switch trees select this path; editors, Select and overlays use the ordinary full root builder
before constructing editor effects, preserving its deferred feedback order.
The `window_app --retained` example exposes it;
`--benchmark-rows N --sparse --bundle .../scaling.js` enables the explicit test
budget used by the same-output benchmark. These flags are development tools.

Fields, controls and scrolls enqueue dirty NodeIds through their existing signals;
the pump enumerates those candidates rather than all registered state. Normal scroll
signal writes notify directly; silent renderer writes use the existing frame-end
scroll subscription. `last_pump_stats()` counts candidates, not runtime notifications,
ordered-map lookups, the shared scroll subscription poll, layout, collect or render.
Those costs remain separate and this API does not promise end-to-end O(k).

Native QuickJS execution keeps the caller/UI thread and uses the already-used
`stacker` mechanism for a temporary segment only when the configured VM budget
plus 256 KiB of host headroom will not fit. The non-parallel rquickjs build does
not refresh its stack top automatically, so the host refreshes it under the
context lock after entering each eval/job segment. The segment is released on
return/unwind; no global Windows `/STACK:8388608` or worker VM is introduced.
Callbacks must remain bounded and must not re-enter the VM. Borrow checks and
resource release remain necessary; they do not replace physical-stack limits.


Application startup and interaction have separate host-owned deadlines.
`ResourceLimits::load_milliseconds` bounds the first source evaluation (default
1000 ms, configurable from 1 to 10000 ms); `turn_milliseconds` still bounds each
interaction and microtask drain (default 20 ms, configurable from 1 to 1000 ms).
Initial queued jobs retain the turn deadline and `jobs_per_turn` limit. A timed
out startup faults the session before publication; the package signature and
authorization path are unchanged. This is an explicit startup admission policy,
not a speedup: source parsing, full SDK mounts and layout still have N costs.
Native VM memory remains 32 MiB and its interpreter stack remains 512 KiB by
default. Explicit full `ResourceLimits` literals must now include the load field;
use `..Default::default()` when retaining host defaults. The SDK counts each
string's UTF-8 bytes once and shares scalar/surrogate checks with JSX keys.
