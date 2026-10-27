//! Crate-local scenario / regression tests (need `pub(crate)` APIs).
//!
//! Organized by domain; unit tests for single modules stay in `src/**` `#[cfg(test)]`.
//! Public-API / `ui!` macro smoke lives under `crates/jadix/tests/`.

#[cfg(feature = "product-tests")]
mod components;
mod golden;
mod interaction;
mod layout;
mod reactive;
mod runtime_regressions;
mod style;
mod text;
