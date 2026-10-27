# jadix_hot_reload

Shared template/asset/stylesheet update protocol and TCP receiver, without a UI runtime dependency. The framework applies received commands only at its own safe points.

The watcher/sender remains in `jadix_hot_reload_cli`. Its RSX text-order analysis uses the Lucide catalog, so combining it with this receiver would produce a Cargo package dependency cycle through the optional Lucide UI integration. The receiver does not depend on the CLI or the icon product.

Install the crate, or copy `src` and reproduce the dependencies from `Cargo.toml` in an application-owned module/crate. The transport is intended for trusted local development, not an authenticated production service.
