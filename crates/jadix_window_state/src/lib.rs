//! Cross-platform window state persistence (TOML) for jadix / winit apps.
//!
//! Optional capability crate: the Jadix engine does not embed product persistence.
//! Register via [`Builder::install`] (feature `jadix`) or drive [`WindowStateManager`]
//! yourself.
//!
//! Default storage is the **process working directory** (`window-state.toml`).
//! Override with [`Builder::with_path`], [`Builder::with_storage`], or
//! `JADIX_WINDOW_STATE_PATH`.

#![cfg(not(any(target_os = "android", target_os = "ios")))]

mod error;
mod flags;
mod manager;
mod path;
mod state;
mod store;
mod view;

#[cfg(feature = "jadix")]
mod jadix_hook;

pub use error::{Error, Result};
pub use flags::StateFlags;
pub use manager::{Builder, WindowStateManager, DEFAULT_LABEL};
pub use path::{executable_dir, resolve_storage_path, StorageLocation, DEFAULT_FILENAME, ENV_PATH};
pub use state::WindowState;
pub use store::WindowStateFile;
pub use view::{MonitorRect, WindowView};
