//! Window / event-loop **host**: compiled-in kind + winit runner + extension slot.
//!
//! winit 0.31-beta covers Windows / macOS / iOS / Android / Web. HarmonyOS uses
//! the same `ApplicationHandler` via crates.io `winit-ohos` (same `winit-core`).
//! [`HostBackend`] remains for hosts that are not winit. Do not add `jadix_winit`.

mod backend;
mod event_loop;

pub use backend::{external_host, external_host_takes_over, set_external_host, HostBackend};
pub use event_loop::{build_event_loop, ensure_winit_host};

#[cfg(target_os = "android")]
pub(crate) use event_loop::android_app;
#[cfg(target_os = "android")]
pub use event_loop::install_android_app;
#[cfg(target_env = "ohos")]
pub use event_loop::install_ohos_app;
#[cfg(not(target_arch = "wasm32"))]
pub use event_loop::run_winit_app;
#[cfg(target_arch = "wasm32")]
pub use event_loop::spawn_winit_app;

/// Which window host this binary was compiled for (or an installed [`HostBackend`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HostKind {
    Windows,
    Macos,
    Linux,
    Ios,
    Android,
    Web,
    /// OpenHarmony / HarmonyOS (`winit-ohos` + Ability).
    HarmonyOs,
    Other,
}

impl HostKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Macos => "macos",
            Self::Linux => "linux",
            Self::Ios => "ios",
            Self::Android => "android",
            Self::Web => "web",
            Self::HarmonyOs => "harmonyos",
            Self::Other => "other",
        }
    }
}

impl std::fmt::Display for HostKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What the current host can do. Used to gate multi-window / tray / blocking `run`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostCapabilities {
    pub multi_window: bool,
    pub decorations: bool,
    pub system_tray: bool,
    /// `run_app` blocks the calling thread (false on wasm `spawn_app`).
    pub blocking_run: bool,
    pub suspend_resume: bool,
    /// Primary `Destroyed` should drop GPU and wait for `resumed` (Android).
    pub recreate_window_after_destroy: bool,
}

impl HostCapabilities {
    pub const fn for_kind(kind: HostKind) -> Self {
        match kind {
            HostKind::Windows | HostKind::Macos | HostKind::Linux => Self {
                multi_window: true,
                decorations: true,
                system_tray: true,
                blocking_run: true,
                suspend_resume: false,
                recreate_window_after_destroy: false,
            },
            HostKind::Ios => Self {
                multi_window: false,
                decorations: false,
                system_tray: false,
                blocking_run: true,
                suspend_resume: true,
                recreate_window_after_destroy: false,
            },
            HostKind::Android => Self {
                multi_window: false,
                decorations: false,
                system_tray: false,
                blocking_run: true,
                suspend_resume: true,
                recreate_window_after_destroy: true,
            },
            HostKind::Web => Self {
                multi_window: false,
                decorations: false,
                system_tray: false,
                blocking_run: false,
                suspend_resume: false,
                recreate_window_after_destroy: false,
            },
            HostKind::HarmonyOs => Self {
                multi_window: false,
                decorations: true,
                system_tray: false,
                blocking_run: false,
                suspend_resume: true,
                recreate_window_after_destroy: true,
            },
            HostKind::Other => Self {
                multi_window: false,
                decorations: true,
                system_tray: false,
                blocking_run: true,
                suspend_resume: false,
                recreate_window_after_destroy: false,
            },
        }
    }
}

/// Compile-time host (ignores [`set_external_host`]). `ohos` wins over `linux`.
pub const fn compiled_host_kind() -> HostKind {
    if cfg!(target_env = "ohos") {
        HostKind::HarmonyOs
    } else if cfg!(target_os = "windows") {
        HostKind::Windows
    } else if cfg!(target_os = "macos") {
        HostKind::Macos
    } else if cfg!(target_os = "ios") {
        HostKind::Ios
    } else if cfg!(target_os = "android") {
        HostKind::Android
    } else if cfg!(target_arch = "wasm32") {
        HostKind::Web
    } else if cfg!(target_os = "linux") {
        HostKind::Linux
    } else {
        HostKind::Other
    }
}

/// Effective host: external backend if installed, otherwise [`compiled_host_kind`].
pub fn host_kind() -> HostKind {
    external_host()
        .map(|h| h.kind())
        .unwrap_or_else(compiled_host_kind)
}

pub fn host_capabilities() -> HostCapabilities {
    external_host()
        .map(|h| h.capabilities())
        .unwrap_or_else(|| HostCapabilities::for_kind(compiled_host_kind()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostError {
    ExternalTookOver,
    HarmonyOsNeedsBackend,
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExternalTookOver => {
                f.write_str("external HostBackend took over the event loop; winit will not start")
            }
            Self::HarmonyOsNeedsBackend => {
                f.write_str("HarmonyOS has no winit backend; call set_external_host before run")
            }
        }
    }
}

impl std::error::Error for HostError {}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
