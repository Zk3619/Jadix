# Jadix patch to Taffy 0.15.0

Source: published crates.io `taffy` 0.15.0, commit
`eb6450694f33e2b7413e40677f230ff63f0fc194` (MIT).

Only the out-of-flow invalidation patch remains. `mark_dirty` invalidates final
layout along the full ancestor chain while retaining intrinsic measurement slots
after crossing an absolute/fixed boundary. Moving out-of-flow descendants changes
placement and overflow, not their ancestors' intrinsic contributions. Position-mode
changes explicitly invalidate the parent intrinsic chain; mixed marks always walk
to the root. `dirty` requires a final layout, not merely a retained measurement.
No cache entries, capacities, lookup keys, or upstream algorithms are changed.

The old 0.13 intrinsic cache-key normalization is removed: on 0.15 it provided
-0.7% to +1.3% changes in five rotated release microbenchmark rounds. The retained
invalidation patch reduced the four absolute-marker update workloads by 62–65%
(about 38us to 13–14us/update); this is a layout-only result, not whole-frame speed.
See `development-docs/engine/taffy_015_vendor_fast_20261019.md` and raw evidence in
`.tmp/taffy-015-20261019/`.

Changes are confined to `src/tree/taffy_tree.rs` and `src/tree/cache.rs`;
`Cargo.toml` adds an empty workspace for independent tests. Regression coverage
moved to `crates/jadix/tests/taffy_cache.rs` so it survives a future fork removal.

Validation on 0.15: 169 upstream unit tests and all four migrated Jadix
integration tests pass, including absolute/fixed/normal-flow transitions.
Randomized comparison of 32,768 geometry records against unpatched 0.15 found
no differences above 0.001 logical pixels (fixed deterministic corpus).
