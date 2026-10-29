# jadix_hot_reload

Shared template/asset/stylesheet update protocol and TCP receiver, without a UI runtime dependency. The framework applies received commands only at its own safe points.

The watcher/sender remains in `jadix_hot_reload_cli`. Its RSX text-order analysis uses the Lucide catalog, so combining it with this receiver would produce a Cargo package dependency cycle through the optional Lucide UI integration. The receiver does not depend on the CLI or the icon product.

Install the crate, or copy `src` and reproduce the dependencies from `Cargo.toml` in an application-owned module/crate. The transport is intended for trusted local development, not an authenticated production service.

Protocol/schema v1 uses a per-process build session, capability handshake, ordered revisions, bounded frames/queues and UI completion ACKs. Unversioned network commands and unsigned templates are rejected. `Applied` confirms a UI commit; `Deferred` stores an unmounted template for later capability validation. `NodeChange` is the shared runtime-identity/path map used by Inspector. Duplicate delivery replays its ACK without repeating effects; incompatible code requires rebuilding.

The optional `syntax` feature supplies the shared Rust/RSX classifier and finite template parser for authoring tools. Ordinary Builder applications do not need this parser or UI macros. Standard Release framework integrations compile out receiver startup, template registration and Inspector. See `docs/devtools.md` in the repository for supported operations and rebuild boundaries.
