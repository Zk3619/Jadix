//! Crate-local scenario / regression tests (need `pub(crate)` APIs).
//!
//! Organized by domain; unit tests for single modules stay in `src/**` `#[cfg(test)]`.
//! Public-API / `ui!` macro smoke lives under `crates/rui/tests/`.

mod components;
mod interaction;
mod layout;
mod reactive;
mod text;
