# jadix_shell

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
dismissal back to the session-owned TSX event. Hosts without popover capability
reject the candidate without publishing it.

The Windows `native-tsx` product compiles actual TSX into an IIFE, executes it in
QuickJS and connects it to retained Win32 controls. Its probe verifies native
editing, keyed movement, undo, permission refusal, late-event cancellation,
keyed popover reanchoring and native dismissal returning to controlled TSX state.
Development-signed packages exercise failed-update preservation, replacement
and explicit rollback with versioned business checkpoints. Publisher credential
management, timers/downloads, GPU panels and Android/OHOS script products remain open. This crate targets trusted
first-party scripts and does not provide an OS security sandbox.
