//! Optional Windows PE `/STACK` helpers for **downstream binaries**.
//!
//! The framework does **not** emit linker flags. Layout collect/build/cascade and
//! hit-testing walk the element tree on the heap so typical trees fit the
//! Windows ~1 MiB default. Use these constants only after measuring a worst-case
//! stack, as headroom — never as a substitute for converting recursion to a work queue.

/// Suggested PE `/STACK` reserve (bytes) if an app opts in via its own `build.rs`.
pub const EXE_STACK_RESERVE: usize = crate::platform::UI_STACK_RESERVE;

/// MSVC linker flag, e.g. `/STACK:8388608`.
pub const fn msvc_link_arg() -> &'static str {
    "/STACK:8388608"
}

/// MinGW linker flag.
pub const fn gnu_link_arg() -> &'static str {
    "-Wl,--stack,8388608"
}

#[cfg(test)]
#[path = "tests/stack.rs"]
mod tests;
