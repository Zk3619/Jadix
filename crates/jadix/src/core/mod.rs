//! Framework capabilities and runtime contracts, independent of themed UI assembly.
//!
//! Keep scroll signals, viewport activation, transform/portal scoping and shared
//! paint/input geometry here. Product components, their builders, RSX tags and
//! theme-dependent defaults belong in `components`; atomic layout/rendering
//! building blocks belong in `primitives`.

pub mod geometry;
#[cfg(debug_assertions)]
pub mod hot_reload;
pub mod reactivity;
pub(crate) mod registry_stats;
pub mod resource;
pub mod scaled_viewport;
pub mod scroll_viewport;
pub mod store;
pub(crate) mod texture_stats;
pub mod tree;
pub(crate) mod tree_stats;

pub mod scrollbar;
