# RUI patch to Taffy 0.13.0

Source: published `taffy` 0.13.0 from crates.io, commit `45a56299d366ddb383e593a1f0372158d00e8530` (MIT), unchanged version and dependencies.

`TaffyView::cache_input` removes irrelevant parent dimensions from existing cache
keys only for leaves and flex containers whose size/max-size are auto and min-size is auto/zero.
Parent width is retained if any padding, border or margin is percentage/calc.
Grid/block containers and explicit/percentage/min/max sizing retain upstream keys.
Known dimensions, available space and requested axis are unchanged. No additional
cache entries or prewarming are introduced. The raw layout algorithm receives the
original input; only cache lookup/storage use the canonical input.

Regression: `scroll_viewport_relayout_does_not_measure_clean_text` in RUI.
Upstream source changes are limited to `src/tree/taffy_tree.rs` and `src/tree/cache.rs`; `Cargo.toml` adds
an empty workspace declaration so this vendored crate can be tested independently.

Validation: 117 upstream unit tests and 46 Flex/Grid/Block constraint-resize
cases pass. An offline old/new differential checks 512 deterministic trees,
four mutations/constraints each, 16 nodes per tree (32,768 layout records,
16 geometry fields each), with no differences above 0.001 logical pixels.
The initial container-wide normalization experiment was rejected after finding
120 geometry mismatches. The accepted patch is restricted to intrinsic flex nodes and leaves.

## Out-of-flow invalidation

`mark_dirty` clears final layout along the complete ancestor chain. After crossing
an absolute-position boundary, it retains the ancestors' intrinsic measure slots:
these exclude out-of-flow descendants in Flex/Grid/Block. Placement and overflow
are still recomputed from the app root. A position-mode change explicitly dirties
the parent intrinsic chain, so entering/leaving normal flow remains correct.
`dirty` checks final-layout validity; a retained size is not a completed layout.
Mixed in-flow/out-of-flow marks always walk the full chain to permit escalation.
No new caches or cache slots are added.

Regression: constrained flow + absolute marker callback count 801 -> 1. Flex,
Grid and Block (rounded/unrounded), overflow, visibility, percentage descendants,
flow transitions and mixed dirty marks are compared with fresh layout.
