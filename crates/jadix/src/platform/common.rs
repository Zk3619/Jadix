//! Cross-platform constants. Not OS chrome — keep separate from `desktop/` window logic.

/// Optional stack when an app **explicitly** spawns a worker that builds a huge
/// UI tree (e.g. gallery's all-sections test). Not applied to the process main
/// thread; not a PE `/STACK` reserve.
pub const UI_STACK_RESERVE: usize = 8 * 1024 * 1024;
