# jadix_animation

Framework-independent animation algorithms and clocks. This crate does not depend on `jadix` or product crates.

- `animation-base`: shared CSS/virtual clocks, reduced-motion policy, easing, timing, spring/inertia physics, scalar animation, and timelines.
- `animation-ex`: stagger scheduling helpers.
- `animation-path`: prepared-path measurement/follow/reveal, topology-preserving PathMorph, stroke-path normalization, matching, resampling, and morph interpolation. It does not resolve icon names or render SVG.

The framework retains adapters that apply animation samples to `Element` and `Style`, including CSS transition/keyframe scheduling. Icon-name lookup and SVG rasterization also remain framework adapters. Source-distribution consumers can copy `src` and reproduce the required dependencies/features from `Cargo.toml`; installing the crate is preferred when updates should flow automatically.
