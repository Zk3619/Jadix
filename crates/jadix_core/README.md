# jadix_core

Reactive ownership, signals and stores without a UI or renderer

See [state management and scopes](../../docs/state-management.md) for read-only
Store views, checked `store_field!` paths, Context and owner lifetimes.

The framework still owns the Element tree and its host runtime; this crate owns the extracted reactive and geometry subsystems. This crate never depends on jadix or product crates. For source distribution, copy src and the dependencies and features from Cargo.toml into a local crate. Crate installation is recommended for updates.

Shared `path` commands, SVG path decoding/encoding and adaptive flattening are also owned here, so animation and rendering use identical geometry without reverse dependencies.
