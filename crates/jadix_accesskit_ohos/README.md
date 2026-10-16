# Jadix OpenHarmony AccessKit adapter

Backport of richerfu AccessKit PR #828, pinned to
b50ecd7ef7c2da9e911a004808cc13e3832ca7f4 (2026-10-16).
Source: https://github.com/AccessKit/accesskit/pull/828
Original paths: adapters/ohos/src/{adapter,node,lib}.rs.
Licenses and copyright headers are retained.

The PR is open, not merged. Its head targets AccessKit 0.25 / consumer 0.39.
This backport uses the existing 0.24.1 / 0.38.0 graph (TreeInfo -> Tree,
NodeRef -> Node, FullNodeId -> NodeId), keeping tree/action types compatible.
API 15 multi-instance registration isolates windows and replacement surfaces.

The winit #4684 backend is already integrated through the separately pinned
winit-ohos dependency. Its accessibility plugin reports screen-reader state;
this crate supplies the separate native semantic tree and action provider.

Native OHOS compilation and host model tests do not establish screen-reader
acceptance on a device. See the HarmonyOS example README for the device gate.

Local changes beyond the backport:
- Reject node ID exhaustion instead of saturating/reusing an ID; prune removed IDs.
- Disable retained callbacks under the tree lock before unregistering.
- Host-guarded event sender holds the current Ability owner during native sends.
- Jadix rejects queued actions from destroyed/replaced surfaces, requests the first
  real tree on its UI loop, and scales logical bounds once at the root.

Jadix currently advertises Click and Focus actions. The adapter retains the PR's
text/selection/scroll mappings, but application text editing and scroll action
support are not established by this integration. macOS/Linux adapters remain stubs.

Validation, 2026-10-16:
- Jadix a11y host tests: 26 passed (24 existing semantic/action tests plus DPI
  and stale-surface action regression tests).
- OHOS ID model test: 1 passed; host and OHOS all-feature adapter Clippy clean
  with warnings denied.
- harmonyos_host a11y feature: aarch64-unknown-linux-ohos cargo check passed.
  Existing Jadix OHOS warnings remain outside the adapter.
- One AccessKit/consumer identity, one winit-core/DPI identity retained;
  cargo-deny licenses, bans and sources passed.

These tests do not execute ArkUI callbacks on a device. Native node queries,
reading order, action delivery, DPI placement and lifecycle timing still require
system screen-reader acceptance. The native callback implementation is a pinned
unmerged backport; keep its source revision and local differences when updating.
