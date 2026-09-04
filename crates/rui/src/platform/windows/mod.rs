//! Windows-specific window chrome and process metrics.

mod process;
mod scroll;
mod stack;
mod window;

pub(crate) use process::process_private_mb;
pub(crate) use scroll::system_scroll_lines;
pub use stack::{gnu_link_arg, msvc_link_arg, EXE_STACK_RESERVE};
pub(crate) use window::{apply_created_window, decorate_window_attributes};
