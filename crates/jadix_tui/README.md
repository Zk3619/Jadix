# jadix_tui

## Bidirectional text, visual navigation and ellipsis

CellSurface resolves Unicode bidi levels, reorders each wrapped visual line,
mirrors RTL punctuation and preserves grapheme clusters and logical UTF-8 byte
indices. Painting, hit testing and visual Left/Right/Home/End/Up/Down use that
geometry; UTF-16 edit events retain the logical document. Password navigation
uses masked logical order. Arabic contextual shaping remains the terminal's
rendering responsibility, and this is not a full Unicode conformance claim.

Shared `NodeStyle.text_overflow` selects `wrap`, `clip` or `ellipsis`; optional
`max_lines` caps static Text rendering. Editors keep the complete value, while
search still sees text hidden by an ellipsis. Exact-fit rows remain intact.

Windows terminal sessions own a cancellable output worker and a hidden top-level
session monitor. Console Ctrl+C/Break/Close and window session-end notifications
request cooperative cleanup. A close callback retains a bounded grace interval
for the UI owner to restore and persist. Pending restoration remains explicit
when a non-cancellable driver or permanently stopped reader prevents completion;
forced OS termination cannot guarantee cleanup. Real OS logoff/shutdown must be
validated separately from fixture-owned WM_ENDSESSION messages.

Ratatui character UI and Kitty graphics for Jadix. This is a terminal presentation crate, independent of `jadix_native` (non-self-drawn OS views) and GPUI.

## Shared character surface

```sh
cargo run -p jadix_tui --example cell_app
cargo run -p jadix_tui --example cell_app -- --auto-layout --colors 16
cargo run --release -p jadix_tui --example cell_bench
```

`CellSurface` presents `jadix_core::presentation::PresentationNode` trees into a Ratatui Buffer, including non-origin embedded areas, clipped scrolling, controlled editing, typed events, focus and modal scopes. Pass the root's `RootLifetime` and an explicit `[horizontal, vertical]` host-unit size per cell. `present` accepts already laid-out frames; `present_auto` runs shared Taffy/terminal measurement from NodeStyle. It does not mount an entire windowed App or translate every framework style attribute. `cell_app` requires no GPU, pixel geometry or Kitty query.

Call `surface.present(surface.revision(), roots)`, render `&mut surface` in the actual Rect, and feed events through `surface.handle(event)`. Route its shared `PlatformEvent` back to the application. Stable IDs preserve editor/history/scroll/valid drag capture across controlled echoes. Removed/hidden/disabled/modal-excluded targets lose capture. Closed/zero-sized ancestors hide descendants. Invalid/stale candidates preserve live state. Acknowledge Edit/Toggle/Select/Dismiss with the event's monotonic revision, rather than incrementing a separate counter; revision exhaustion fails before editing. Internal selection is directional; outgoing UTF-16 ranges follow the shared ordered-range contract.

Mappings cover text, Input/Textarea, Button, Checkbox, Switch, Select, container/Scroll, modal scopes and Image alt placeholders. Foreground/background/alpha and bold map to cells. Editing reuses TerminalEditor; CellSurface Textarea Up/Down use the actual wrapped grapheme grid, preserve a preferred column across shorter rows, and support Shift selection. Caret row reveal and single-line paste normalization are retained. Password fields mask graphemes and block copy/cut. Shared menus, inline text and general command routing are described below; arbitrary factories, visual Home/End, Bidi and full window command integration remain separate work. Shared automatic layout centers Dialogs and positions/flips/clamps Popovers from their anchors. Presentation and hit order follow overlay dependencies; portal coordinates escape ancestor scroll clips while hidden ancestors remain hidden. The top scope dims its background, blocks background activation and reports revisioned outside-click dismissal according to its policy; the application must publish the closed state. Automatic-layout surfaces track Popover anchors during retained scrolling without a new presentation: at most eight overlay reprojection passes, each O(N). Explicit frames remain caller-owned; hidden/virtual anchor keep-alive is separate work.

Limits: 65,536 nodes, depth 128, 16 MiB candidate strings, eight modal scopes and 32 MiB conservative retained editor text across the surface (current documents/history/local clipboards). Each editor also retains its 1 MiB document / 128-entry / 8 MiB history limits. Over-budget edits fail before publication. These limits exclude container metadata, cell buffers and GPU/image memory.

`CellSurface::set_colors(CellColors::{TrueColor, Indexed256, Palette16, Monochrome})` selects explicit color reduction; examples accept `--colors rgb|256|16|mono`. Changing it invalidates paint without resetting editor state. Conventional xterm palette values guide approximation; terminals may redefine indices 0–15. `Monochrome` preserves default/reset colors, uses reverse for selection and underline/bold for search, and resets scrollbar colors inside the surface area. `CellApplication` applies the same policy to its menus; standalone menus use `TerminalMenu::set_colors`. NO_COLOR is still honored.

Pair `Palette16` with `AnsiColorWriter::new(writer, colors)` below `CrosstermBackend` to emit basic 30–37/40–47/90–97/100–107 SGR instead of indexed SGR. The cell and portable examples use this adapter. Other color policies pass through unchanged. The adapter bounds CSI storage to 128 bytes and scratch output to a 4 KiB chunk, keeps OSC/DCS/APC payloads opaque, and rejects malformed or unfinished control sequences. I/O failure poisons the writer; session cleanup uses the host's separate output path.

`DefaultColorQuery` issues one explicit startup query through an existing writer. Feed normal `WireEvent`s to `observe`, bound the host poll by `deadline`, and apply `poll` results with `surface.set_default_colors`. It creates no stdin reader or timer. A 1ns–2s timeout preserves absent channels as unknown; OSC replies lack request IDs and are not authenticated. Observed background colors replace only the unknown/reset black fallback during alpha composition. Transparent backgrounds stay Reset; mono remains terminal-default colored. The complete 16-entry palette is not queried. `portable_app --query-colors` demonstrates the actual loop. Real rxvt-unicode returned black/white defaults; ConPTY timed out without inventing colors.

`FrameMeter::writer` counts accepted bytes; `backend` counts successful diff cells. Wrap the final writer once and its backend with the same meter, then `meter.measure(|| terminal.draw(...))` to include draw/diff/ANSI flush. `latest()` reads one sample; `samples()` returns at most 120. Failed frames retain partial bytes and are unsuccessful. No timers or frames are scheduled; closure timing does not establish terminal scanout latency.


`present_auto` uses the same extracted engine as `jadix_native::layout::NativeLayout` (a compatibility alias), without depending on OS views. Width/height/min/max, flex row/column/grow/shrink, gap and padding drive Taffy; leaf metrics use the same grapheme/control sanitization as paint/hit. Native point-based overlay defaults (32-unit dialog inset, 16-unit popover inset, 8-unit margin) remain shared: choose an explicit cell size such as [8,16], not implicit one-point pixels. Candidate validation precedes layout; active frames and editing survive failures. Measurement caches retain at most 16 requests per node and are invalidated by content/style/environment changes. The engine retains Taffy identities across candidates. `commit_auto` additionally uses structural delta synchronization; `present_auto` retains its full candidate path. Neither implies end-to-end O(k). PNG intrinsic dimensions retain shared native rules; a character image still paints alt text. See current implementation and validation.

`find(query)`, `find_next(backwards)`, `search_summary()` and `clear_search()` implement case-sensitive literal search over mounted, logically visible in-scope content, including offscreen scroll rows and live editable text. Password content is excluded. Queries are limited to 512 UTF-8 bytes and no controls; only whole graphemes match. At most 4096 results are retained with an explicit truncation flag. Search highlights/reveals without replacing editor selection or focus, and refreshes at paint or explicit search calls after mutations. Unmounted virtual rows, regex, case folding, document search services and OS search are separate. `cell_app` uses Ctrl+F, Enter to retain highlights, F4/Shift+F4 to cycle, Esc to clear.

`BrailleImage::new(&rgba)` is an explicit CPU fallback for raster pictures and existing `RgbaFrame::from_canvas` output: 2×4 Braille dots per cell, alpha over an explicit background, contrast threshold and the same color policies. It samples/fills the caller's Rect without GPU, Kitty upload or retained copies. This intentionally loses resolution, does not infer aspect ratio, and relies on terminal font glyph coverage. `workbench --braille` demonstrates it; offline `--export NEW_DIRECTORY` also writes `braille.ansi` alongside the source Canvas PNG and Kitty stream.

## Mounted portable applications

`set_menubar` / `set_context_menu` accept the same `jadix_core::menu::MenuItem`
used by `NativeMenuItem` in window apps. Nested menus support F10, arrows,
typeahead, mouse capture, disabled items and modified accelerators. Route input
through the application, paint it last, and use `app.needs_frame()` for both
menu and content damage. Global menus pause behind modals and on focus loss.
`portable_app --menus` exercises real shared Signal callbacks. `MenuState` adds controlled checked/radio/role entries. A shared
`CommandRegistry` can invoke commands without a Button. `CommandRouter` adds
1–4-stroke sequences, bounded prefix deadlines, disjoint scopes and editor
opt-in; focus, modal and pointer changes cancel pending prefixes. OS roles
require explicit host callbacks in the terminal.

Portable `InputBuilder::max_length` limits user insertions by UTF-16 units,
without splitting graphemes. Controlled external values and undo retain their
existing contracts. `NodeStyle::inline_styles` has up to 64 sorted UTF-8 byte
ranges for text foreground/background and bold/italic/underline/strike. Invalid
ranges fail before publication; runs do not introduce variable-width fonts or
Bidi layout. Terminal IME input remains committed text, not GUI preedit.

Interactive Windows output now uses an owned synchronous-IO worker with native
cancellation, bounded chunk/queue sizes, deadlines and a shared cancellation
token (`output.cancellation()`). Cleanup performs Console flag/codepage restoration on an owned worker and
returns after at most a 100 ms wait. A pending timeout is not successful
restoration: ConPTY can serialize mode calls behind a stopped consumer.
Non-cancellable drivers may retain both IO and restoration work until IO ends.

For tmux, explicitly call `request_tmux_passthrough()` before the Kitty probe,
and obtain pane geometry with `TerminalViewport::current_tmux`. Enable
`allow-passthrough on` in that server; use a UTF-8 tmux client (`tmux -u`) when
SSH forwards no locale. `terminal_app --tmux --tiles --compress` demonstrates the
path; `--tmux-depth N` supports 1–8 explicit nested passthrough layers.
`mux_probe` and `terminal-mux-check.py` exercise two clients through two layers. No automatic screen/other-multiplexer fallback is assumed. The acceptance
tool uses its own socket/config and never edits the user's tmux server.

`terminal-host-check.py --ssh-only --ssh-rtt-ms 300 --ssh-bandwidth-kib 256`
tests an explicitly supplied private loopback sshd through a delayed TCP relay.
These are simulated byte-stream delay/bandwidth profiles, not WAN packet-loss,
retransmission or public-SSH measurements. See the complete current comparison
in the current capability report.

```sh
cargo run -p jadix_tui --features portable --example portable_app
cargo run --release -p jadix_tui --example protocol_bench
```

`CellApplication::new([8.,16.], build)` owns the existing CPU `jadix::portable::Element` tree and reactive owner. Call `present(area, environment_revision)`, render `&mut app`, and feed terminal events to `app.handle(event)`. Shared callbacks update Signals and the host refreshes controlled values, navigation and requested focus. Focus/Scroll observations update the retained surface directly, without refreshing bindings or repeating Taffy. Use `surface_mut()` for search/color policy; external state changes explicitly call `present` on the UI thread. Unconfigured root dimensions fill the viewport. No polling timer or alternate reactive runtime is added. The example uses existing textarea/button/checkbox and NavigationHistory pages; F3/Ctrl+O opens a modal, F5/Ctrl+F searches, Ctrl+Q exits.

`jadix::portable`, `jadix_components::portable` and explicit `portable_ui!` macros are accessible alongside GPU features. These are CPU authoring types. With `shell`, `ShellApplication` mounts actual
SDK/esbuild TSX through the existing shell QuickJS runtime, using
`ScriptApplication.publish_commits` and `CellSurface.commit_auto`;
`cargo run -p jadix_tui --features shell --example shell_app` demonstrates it.
The GPU host is `jadix_shell::gpu::GpuApplication`. Root disposal closes the lifetime and owner; a projection/present error closes this application because callback bindings may already have changed. Hidden navigation pages retain application Signals but remount presentation IDs, so their terminal undo/caret is not automatically preserved.

`CellSurface::apply_batch` and `apply_batch_auto` accept shared core `CommitBatch` Create/Update/Move/Remove/Focus and return `ApplyReceipt`. Root/previous/revision, structure, visibility/modal focus, retired IDs, 65,536 operations, 16 MiB cumulative descriptor strings and 4,194,304 structural work units are checked before publication. Failed batches preserve active state. Removed IDs cannot be recreated through this protocol. Explicit-frame updates of unchanged leaf types/styles/frames stage only changed descriptors/editors; all fallible validation completes before publication. Structural changes and automatic layout still validate complete candidates. Automatic layout now retains shared Taffy node identities and caches across presentations; changed styles, measurement contexts and children dirty the affected tree. Legacy candidate validation, synchronization and frame publication still scan all nodes; CellApplication does not generate engine mount batches.
`commit_auto` uses the separate retained StructuralLayout protocol: ordinary
Taffy plans visit changed records and affected parent child lists, with full
fallback for overlays, viewport/environment changes and cache reconstruction.
Candidate clone/validation, frame collection and surface publication remain
full-tree work. `ShellApplication` generates real structural batches from keyed
SDK snapshots; `sync_nodes` counts synchronization records, not total work. See 42 capabilities, repeated measurements and real terminal evidence.

`layout_stats()` on CellApplication reports the last successful shared layout: engine nodes created/reused/removed/updated and child lists replaced, plus host measurement counts. Synthetic viewport and open overlay panel nodes count too. An unchanged tree can reuse Taffy results without invoking the host measurement callback, so zero callback cache hits is normal. `surface_mut().release_layout_cache()` releases only Taffy storage, retaining active presentation and bounded host measurement entries; the next present rebuilds it. Environment revision changes and explicit measurement invalidation clear both layout and measurement caches.

```rust,ignore
app.set_shortcuts(vec![jadix_tui::Shortcut {
    code: jadix_tui::crossterm::event::KeyCode::Char('g'),
    modifiers: jadix_tui::crossterm::event::KeyModifiers::CONTROL,
    target: "increment".into(), // visible Button template ID
    scope: None,               // or named ancestor of target and focus
    in_editors: false,         // preserve focused editor shortcuts by default
}])?;
```

Bindings are atomic, at most 128, and accept modified printable characters (Control/Alt/Super) or F1–F24. Duplicate scope/chord pairs are rejected. ASCII characters normalize to lowercase; modifiers match exactly. The nearest focused ancestor scope wins. Only presses invoke callbacks; matching repeats/releases are consumed. Hidden, disabled, retired, background-modal or ambiguous targets are ignored, and names resolve again after remount. Focus loss suppresses commands. This older shortcut API is Button-specific. The shared registry/router and
menus above provide general commands and multi-key sequences; OS global
registration remains host-specific. The portable example binds Ctrl+G with explicit editor opt-in.

`set_scrollbars(true)` enables proportional vertical/horizontal edge tracks with page clicks and captured thumb dragging. They overlay one cell inside Scroll frames, preserve descendant/portal paint order, and cancel capture on invalid identity/scope, focus loss, Tab or release. Partially clipped tracks are hidden; no touch inertia or animated auto-hide is implied. Scroll ranges are cached in one O(N) publication pass, with nested clip/portal boundaries; scrolling still projects geometry and paints the visible surface. `scroll_to(node, [x,y])` validates visible in-scope Scroll targets and finite absolute host-unit offsets, clamps to this range and reports whether it moved.

`application_bench` (requires `portable`) measures the same portable application with retained scrolling versus explicit full re-projection on every scroll. `protocol_bench` compares full present with one geometry-preserving leaf update, including transaction validation and paint/diff. Both use TestBackend, exclude ANSI/TTY and report initial costs separately; they do not establish equal applications or scanout performance against nsh.

`layout_bench` compares retained versus rebuilt Taffy on unchanged/text/style/reordered snapshots, preserving the host measurement cache in both. `business_bench` (requires `portable`) measures named command → shared callback/Signal → full projection/layout → paint/diff. Its baseline releases only the Taffy cache each operation. Both report 500/5,000-node workloads, initial cost and engine work counts; they are same-implementation cache comparisons, not a previous-release binary comparison.

```sh
cargo run --release -p jadix_tui --example layout_bench
cargo run --release -p jadix_tui --features portable --example business_bench
```

## Optional sparse Kitty updates

Call `KittyTerminal::request_tiling(Size::new(16, 8))` before `start`, or run `terminal_app --tiles`. It retains the previous RGBA frame, skips identical pixels, encodes sparse cell-aligned tiles, and falls back to a full keyframe for dense damage. At most 256 fixed tile IDs fit below the fence range. Resize/close delete owned placements; one batch uses existing frame credit. `damage_metrics()` reports encoding work. Full-frame remains the default: dense compressed changes can cost more CPU with tiling.

The pinned Kitty encoder is normalized to explicit placement dimensions and per-cell coordinates so overlay interruptions and sparse terminal cursor jumps cannot invalidate a base row. Real terminal checks validate visible pixels as well as upload bytes. See the capability and performance record and latest nsh source comparison; different application pipelines are explicitly identified.

```sh
cargo run -p jadix_tui --example workbench
cargo run -p jadix_tui --example workbench -- --kitty
cargo run -p jadix_tui --example workbench -- --export NEW_DIRECTORY
```

The example shares a counter between character presentation and a self-drawn Jadix CPU Canvas (including text). `+`/`-` update it; `q`/Esc exit; resize redraws. Idle blocks on input rather than ticking. `--kitty` requires a positive capability query, Unicode-placeholder support and a direct terminal; tmux/screen are currently rejected. `--export` writes an offline ANSI stream and PNG, not a real-terminal acceptance result.

`RgbaFrame::new` accepts tightly packed straight-alpha sRGB RGBA from pictures or Jadix rendering; `RgbaFrame::from_canvas` converts the existing Canvas premultiplied pixels. `KittyView::new` takes a nonzero caller-owned image ID and placement in cells (each dimension 1..=297 for the pinned placeholder encoder). Keep the view between unchanged frames; reuse the ID when replacing content; call `delete` on removal. IO errors invalidate the terminal session: do not assume an upload consumed into a failed draw will retry automatically. The image widget uses Unicode placeholders and direct inline transmission; it does not implement display ACK frame flow control.

Defaults require no GPU, GPUI or JS VM. GPU backends are opt-in (`backend-dx12`, `backend-vulkan`, `backend-gles`, `backend-metal`).

```sh
cargo run -p jadix_tui --features backend-dx12 --example element_workbench
cargo run -p jadix_tui --features backend-dx12 --example element_workbench -- --export NEW_DIRECTORY
```

`JadixScene` renders real `Element` trees through the production layout, SDF, glyphon, image, path/Canvas and clipping pipelines. Device, pipelines, glyph atlases, texture pool, vertex streams, render target and readback staging survive frames. Logical scale is explicit; RGBA output is opaque sRGB. Missing textures return `WouldBlock`; backdrop blur, filters, capture/effects, custom shaders and 3D require further renderer integration. The legacy `render_element_chrome` helper remains chrome-only.

`SceneInput` reuses Jadix's clipped hit paths, pointer capture/target/bubble phases, down/up activation intersection and wheel callbacks. Explicitly cancel gestures for navigation/root replacement; resize, focus loss and wheel input cancel them automatically. For ordinary redraws, call `refresh_gesture` after successful layout: stable named targets retain their press with current callbacks, while replaced/unnamed targets cancel. Tab/Shift+Tab traversal and Enter activation are supported. Text editing and deadlines are composed by the host below; this does not emulate the complete windowed `App` runtime.

`KittyFrameFlow` submits at most two image frames, each followed by a distinct query fence. It stops before drawing under backpressure and invalidates on timeout, owned terminal error, partial write or failed flush. `KittyReplyDecoder` incrementally separates graphics responses from input while preserving unknown bytes. `KittyTerminal` and `StdinEvents` now connect it through one actual input owner in `terminal_app`; legacy workbench examples retain their simpler event loops. No competing reader or private Crossterm byte parser is used. Query completion is transport evidence, not monitor scanout.

`element_workbench --export` renders three genuine GPU pages, replays terminal mouse coordinates into the Element button callback, and emits PNG/ANSI/cleanup evidence. The acceptance script decodes every Kitty chunk and checks byte equality with GPU PNGs, fixed image ownership and changed business state. These offline results are separate from real Kitty/Ghostty/SSH acceptance.

## Interactive runtime

```sh
cargo run -p jadix_tui --features backend-dx12 --example terminal_app
```

`terminal_app` connects the sole `StdinEvents` reader to `KittyTerminal`: a positive query precedes GPU rendering, each upload is fenced, and at most two frames are in flight. While credit is exhausted, inputs update state in order and the next available frame renders the latest state. Query completion proves transport processing, not scanout or placeholder compatibility. Unsupported terminals time out or reject; tmux/screen are rejected explicitly. `--stdio` selects an external controlled protocol host and skips terminal mode changes.

`InputDecoder` handles incremental UTF-8, bracketed paste, CSI-u press/repeat/release, common VT keys, SGR mouse, focus, size reports and Kitty replies. Pasted APC sequences cannot acknowledge frames. Paste is limited to 1 MiB, CSI to 128 bytes and APC/OSC/DCS control strings to 1024; unsupported sequences remain `Unknown`. Windows console input uses native Crossterm records; Unix and redirected pipes use bytes. Only Escape ambiguity uses a 40 ms deadline. Windows pipe readiness checks use 2 ms waits; Unix terminal-size checks cap blocking at 250 ms. Neither causes idle GPU rendering.

`TerminalEditor` uses existing `jadix_document` snapshots, with UTF-8 selections, grapheme movement/deletion, logical-line navigation, Unicode paste, bounded undo/redo and a session-local clipboard. Documents are limited to 1 MiB; history to 128 snapshots / 8 MiB. Editing costs O(document bytes + lines). It preserves state across focus loss and view rebuilds. The example reuses production font hit testing and caret timing, with Tab focus, Enter button activation, F2 animation, focus suspension and zero-size recovery; Ctrl+Q exits. It has no IME preedit. Explicit OSC52 writing is described below.

`TerminalModes` owns raw/alternate/cursor/mouse/paste/focus/keyboard modes and restores them on return or unwind; Windows VT output/input flags are configured explicitly. Unix additionally owns SIGTERM/SIGINT/SIGHUP intent and saved stdout flags. Use `modes.output()` for 16 KiB buffered nonblocking/poll output, cancellation checks and a stalled-write deadline (`enter_with_output_timeout`, default 10 seconds). Input polling and cooperative workers can observe `shutdown_requested()`. The signal handler only writes an atomic flag. `begin_output_cleanup()` is a one-way phase: stop rendering first, discard pending buffered frame bytes, attempt CAN/ST and immediate nonblocking graphics deletion, then restore even after cleanup errors. Restore invalidates old writer clones before returning flags/handlers; a new owner can enter while old closed clones remain alive. `written_bytes()` counts successful Unix OS writes including mode setup/cleanup, separately from accepted FrameMeter bytes. `TerminalOutput::stdio()` preserves ordinary pipe-peer semantics.

Embedded hosts can call `CellApplication::request_exit()` or `ShellApplication::request_exit()` to stop dispatch before disposal. `exit_requested()` becomes true immediately; later presentation and event calls are rejected. The request cancels a pending command prefix and pointer/scrollbar capture without running root cleanup; `dispose()` remains the explicit, idempotent cleanup boundary. The portable and shell examples use Ctrl+Q to exercise this order. Native-window forwarding remains a separate host integration task.

Full output queues may reject escape cleanup even though termios/flags/handlers are restored; examples report this separately as `output_cleanup_complete`. SIGTERM/SIGINT/SIGHUP finish through normal cleanup attempts with exit 0. Noncontrolling hangup may remain an IO-error exit, while controlling SIGHUP follows signal exit. Windows has no corresponding nonblocking deadline/signal implementation yet. Bypassing the session writer, SIGKILL/abort and arbitrary noncooperative blocking tasks are outside this guarantee. The deadline bounds a stalled write, not a continuously progressing whole frame. `KittyTerminal` closes only its image ID. Forced termination cannot run user-space cleanup. The process acceptance covers 13 interaction checks plus timeout, rejection and disconnect, and 9 background/compression checks plus compression rejection/failure, and 14 session-state checks using real DX12/OpenGL frames; the controlled pipe peer is not a terminal emulator, SSH, or cross-OS acceptance. Linux x86_64 and macOS aarch64 default-feature all-target cross-checks pass with an explicit existing Clang toolchain; OS runtime, actual TTY/ConPTY graphics and mode restoration remain separate validation.

## Background resources and compression

`Mailbox<T>` exposes the existing `jadix_tasks::EventSink<T>` contract. Producers return immediately on Full/Closed with their payload; the UI uses `StdinEvents::poll_with` to wait on input and work without a second stdin reader. Windows uses a manual-reset kernel event; Unix uses a nonblocking socket pair in poll. Notify/reset share the queue lock; payloads are dropped outside it. Ready input and tasks alternate. Entry capacity does not imply a byte budget for arbitrary T.

`RequestSlot` owns one resource's generation/cancellation on the UI thread (`!Send/!Sync`). Pass Send `RequestTicket`s to your own executor; connect `ticket.cancellation().link_handle(handle)` to existing `jadix_tasks::TaskHandle` cancellation. `ticket.publish` checks before admission, and the UI must check `slot.accepts(&update.ticket)` again before applying: a queued result may become obsolete. Slot identity, revision, cancellation and closed state all matter. Slot Drop cancels; mailbox Drop closes late senders.

The example runs at most two application-owned resource workers. F3 starts/replaces a request, F4 cancels and F5 demonstrates failure; success uploads the real retained GPU texture on the UI thread, while refresh/failure/cancellation retain the previous image. Teardown cancels and joins these cooperative workers. Unknown external image jobs can still use the 50 ms `WouldBlock` retry fallback; this is not automatic integration of the full windowed `ui_bridge` or every framework task.

Pass `--compress` or call `KittyTerminal::request_compression` before start to negotiate zlib. A separate valid `o=z` query must succeed before upstream ratatui-image compression is used; rejection falls back to raw RGBA. A silent probe still times out. Frames remain f=32 with original dimensions and bounded chunks; GPU credit is checked before rendering/encoding. The acceptance peer inflates with a strict expected-size bound and validates exact pixels in the compression contract test. Current resource-scene payload measurements are in the acceptance report; they are not terminal/SSH latency or nsh performance comparisons.


## Editing sessions, modal scopes and clipboard

`TerminalEditor::apply_external(expected_revision, text)` rejects stale controlled replacements. Equal text keeps selection/history; a changed document keeps valid UTF-8 offsets and clears history belonging to the previous document. `checkpoint_bytes` stores current text, directional selection, undo and redo; `restore_checkpoint` validates the complete schema and budgets before mutation and creates fresh document identities. It never serializes the session-local clipboard. The current document is limited to 1 MiB, history to 128 snapshots / 8 MiB, encoded state to 16 MiB; escape expansion may make a valid document/history too large to save. History contains prior document text, including text the user deleted.

`EditorCheckpoint::new(path)` composes that state with the CPU-only `jadix_document::checkpoint::CheckpointStore`, also re-exported by `jadix_native`. Loading remembers a generation; saving compares it, acquires an exclusive `.lock`, syncs a same-directory temporary file, then renames it. Conflicts, malformed checksums/schema, non-regular paths, existing locks and IO errors preserve existing state. `None` is create-only, never permission to overwrite an unread file. A crash can leave a lock; inspect its owner before explicit recovery. This is not authenticated storage, an adversarial filesystem sandbox or a power-loss durability guarantee.

`CellApplication::enable_checkpoint(path, &["draft"])`, called after first `present`, opts named controlled Input/Textarea fields into automatic startup recovery and save after successful input. It reuses that same store and editor codec; template keys remap to fresh NodeIds. Values, directional selection, undo/redo and named editor focus recover together. The allowlist is 1–64 unique keys of at most 256 bytes; the complete encoded payload is at most 16 MiB, and restored retained editor text must satisfy the surface's 32 MiB limit. Enabled non-password editors with Signal bindings are required. Initial recovery requires saved keys to be mounted and active; hidden pages retain their last saved editor snapshots while attached.

Automatic saves use synchronous atomic file I/O and serialize the bounded editor state. They are opt-in and are not an O(k) frame performance claim. Unchanged payloads do not rewrite disk. Save conflicts return after the local input commits, preserve the previous file, and keep the failed recovery attachment available for explicit handling. Call `save_checkpoint` after external state changes and before explicit disposal; Drop does not hide fallible saves. Business signals, page/scroll state, passwords, clipboard and transient modal/capture state remain application-owned. `portable_app --checkpoint draft.json` is the runnable host entry. The owned ConPTY restart check also kills its own child after a completed save and verifies recovery, without claiming forced-exit terminal cleanup or power-loss durability.


```sh
cargo run -p jadix_tui --features backend-dx12 --example terminal_app -- --checkpoint draft.json
# Explicitly allow selected text to be sent to the terminal clipboard:
cargo run -p jadix_tui --features backend-dx12 --example terminal_app -- --checkpoint draft.json --clipboard
```

F7 saves a snapshot, F8 reloads, F6 opens a modal, and F9 sends the selected text through OSC52 only when enabled. Disk work uses the example's bounded workers/mailbox; serialization runs on the UI thread. Newer local edits reject an asynchronous reload; a save completion records its generation without replacing later local edits. Startup loads valid state; missing files retain the initial document, while invalid state reports an error without overwriting the file. The example limits concurrent workers to two and file work to one. `--io-delay-ms` is controlled acceptance latency injection.

`SceneInput::push_modal(root, id)` must follow layout. It traps Tab/Shift+Tab in that named subtree, restricts Enter and pointer callbacks, cancels pending gestures, and saves prior focus. Escape/pop restores valid previous focus or the first remaining target. `refresh_gesture` also unwinds removed modal roots. Nesting is limited to eight, duplicate/empty/missing scopes fail explicitly. Hosts must route text editing/application shortcuts to the active scope, as the example does; the close event must not also edit the background document. OS pointer capture and screen-reader announcements are separate capabilities.

`TerminalClipboard::new(false)` is the default policy. Explicitly enabled `copy` sends base64 UTF-8 in OSC52, rejects empty selections (no implicit clear) and payloads above 100 KiB, and reports IO errors. It never queries the clipboard or exports automatically. A successful write is transport output, not confirmation that a terminal authorized or changed the OS clipboard. The controlled peer validates these bytes; real terminal/SSH clipboard acceptance remains outstanding.


## Environment, theme and reduced motion

GPU scene callers rebuild views with `scene.replace_root(&mut root, next)` before
`scene.render(...)`. This reuses the window App's keyed reconciliation, retaining
layout identities and unmount hooks across sibling moves. `terminal_app` and
`element_workbench` use it. `scene.renderer().last_frame_stats()` reports resident
layout counts and CPU layout/collect/render/readback timings after successful
frames. Full projection, reconciliation and rendering still visit trees;
retained identities alone do not establish incremental total cost. Character
`CellSurface` remains independent.

`jadix_tui::environment` re-exports the same CPU-only `jadix_core` environment consumed by native hosts. Select one source with `terminal_app --environment preferences.json` (a complete validated snapshot) or `--system-environment` (Windows/Linux). The system option automatically subscribes to OS notifications through a bounded latest snapshot and the existing mailbox; the UI thread applies it without replacing current focus lifecycle. The file option remains explicit; F10 reloads in a bounded worker and rejects stale revisions. Without either flag the example uses a deterministic default. `--dump-environment` prints the actual system snapshot and exits before GPU or terminal initialization. Unknown adapters fail explicitly.

Theme and host font-scale changes render through real Element styles and glyphs, preserving the immutable editor snapshot, directional selection, focus and undo/redo. Reduced motion suppresses the example animation deadlines without forgetting the animation intent; restoring the preference resumes them. Caret blinking remains an independent policy. Background/inactive lifecycle suspends animation and blocks text/activation; FocusLost cancels pending pointer gestures. Focus events also advance the environment revision, so a late refresh cannot restore an obsolete lifecycle. Identical snapshots produce no output. These are example policies over shared state, not automatic global theme/RTL/locale formatting for every Jadix component.

The process acceptance adds 14 environment checks, including actual Windows read-only metadata, same-color glyph-mask comparison, no-output checks, invalid/stale refresh rejection and subsequent redo/undo. Pipe peers remain transport evidence. 配套宿主验收工具 separately exercises a real Kitty window on Linux/WSLg, and optionally an isolated loopback OpenSSH server: actual Jadix pixels, typing, automatic theme/hour/font/motion changes, zlib and stty restoration. It creates a private settings session before D-Bus activation and verifies the user's dconf database is unchanged. The host reads actual TTY pixel dimensions and encodes virtual placement rows/columns so displayed pixels and cell input agree; a TTY reporting no pixel dimensions fails explicitly. Mouse acceptance injects SGR packets through Kitty/SSH, independently of physical OS mouse automation. The current local evidence uses Kitty 0.32.2, OpenSSH 9.6p1 and Mesa software GLES on WSLg; it does not establish macOS runtime, public remote SSH, tmux or OS clipboard behavior. See host validation.

Unix private PTY lifecycle checks and real Kitty/loopback SSH signal-during-save checks: see the latest capability and evidence record. The broad superiority goal stays open while complete application mounting, menus/navigation and the other declared gaps remain; shared CPU auto layout is now available.

Windows character acceptance uses an owned real ConPTY with native Console input, Unicode, undo/redo, resize and exact input/output mode restoration. Jadix saves the entire pre-raw input mode rather than relying on Crossterm’s conventional raw-mode reset. This does not establish Windows signal/backpressure handling or Kitty graphics in ConPTY.
