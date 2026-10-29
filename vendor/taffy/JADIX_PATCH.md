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

Changes are confined to `src/tree/taffy_tree.rs` and `src/tree/cache.rs`.
Jadix regression coverage is maintained in `crates/jadix/tests/taffy_cache.rs`.
The patch changes layout invalidation; it makes no whole-frame performance guarantee.
