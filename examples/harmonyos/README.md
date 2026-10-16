# OpenHarmony Ability host

The Rust entry uses `jadix::run_ohos` and the workspace's `jadix_winit` facade.
`EventLoop::builder().with_openharmony_app(app)` selects the pinned `winit-ohos`
backend, sharing the same core/DPI types as desktop. `run_app` registers Ability
callbacks and returns to the platform.

The Rust manifest includes the direct N-API dependencies required by `#[ability]`;
`build.rs` configures N-API linking only for the OHOS **target** (the script itself
runs on the host). Ability and its derive/plugin crates use `=1.0.0-beta.3`.

`ohos/entry` contains integration snippets for an existing DevEco/ohrs application,
not a complete signed HAP project. Copy the module dependencies and `EntryAbility`
into that project, install the matching HARs and build/deploy it there. The entry
registers the six plugins enabled by the default Winit backend. The clipboard
factory explicitly sets `application/x-jadix-text-metadata-v1`, as required by
the beta.3 constructor. Extended plugins
require additional HAR factories when `jadix_winit/ohos-extended-plugins` is enabled.

The version and factory names were checked against the published beta.3 HARs,
including the [WindowPlugin assembly example](https://repo.harmonyos.com/ohpm/@ohos-rs/ability-plugin-window/-/ability-plugin-window-1.0.0-beta.3.har).
This is a source check; ArkTS/HAP compilation and device execution remain unverified.
The selected backend enables API-22 window bindings; Rust `cargo check` with a
locally installed SDK does not prove those APIs exist on the deployment device.

With Rust's `aarch64-unknown-linux-ohos` target installed, set `OHOS_NDK_HOME` to
the SDK and configure its Clang/AR for the target, then run:

```text
cargo check -p harmonyos_host --lib --target aarch64-unknown-linux-ohos
cargo check -p jadix_winit --lib --all-features --target aarch64-unknown-linux-ohos
```

The cross-target Rust checks passed on 2026-10-16. These commands do not link a
cdylib or certify GPU presentation, IME, focus, surface recreation, permissions,
plugin deployment or exit on a device. Fork provenance and update requirements are
in [jadix_winit](../../crates/jadix_winit/README.md); detailed acceptance records are
in [the window manual](../../development-docs/ops/native_window_and_menu.md).

## Native accessibility (2026-10-16)

Enable the a11y feature for the native ArkUI provider:

    cargo check -p harmonyos_host --lib --features a11y --target aarch64-unknown-linux-ohos
    cargo clippy -p jadix_accesskit_ohos --all-features --target aarch64-unknown-linux-ohos -- -D warnings
    cargo test -p jadix_accesskit_ohos --lib
    cargo test -p jadix --lib --features a11y a11y:: -- --test-threads=1

Winit PR #4684 (head c9ee2bff) and AccessKit PR #828 (head b50ecd7e)
were both OPEN, not merged, when checked on 2026-10-16. The pinned winit-ohos
already supplies #4684's backend and the Ability accessibility state plugin.
It does not supply the AccessKit semantic tree. The local jadix_accesskit_ohos
crate backports #828 to AccessKit 0.24.1 / consumer 0.38.0. It uses API-15
multi-instance provider callbacks; the existing window backend requires API 22.

The native semantic provider uses the XComponent API and does not require an
additional HAR. Screen-reader state/font-scale APIs are separate extended
plugins and require their matching HARs if enabled.

Device acceptance remains PENDING: enable/disable the system screen reader;
verify role/name/value and reading order; activate buttons; move focus; change
DPI; background/foreground the Ability; destroy/recreate the XComponent; ensure
old callbacks/actions cannot affect a replacement; repeat with supported
secondary windows. Record device, OS/API level and screen-reader version.
Text replacement, selection editing and AT scrolling also need application-side
actions before they can pass; current Jadix trees advertise Click and Focus.
No window/platform or accessibility score increase follows from cargo check alone.
