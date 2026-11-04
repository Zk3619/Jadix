//! Startup coordination and optional native/component splash presentations.
//!
//! The runtime owns windows and GPU resources; this crate owns startup policy,
//! preparation, cancellation, retry epochs, presentation and controlled splash UI.
mod startup;
mod system;
pub use startup::{SplashMode, StartupConfig, StartupController, StartupPhase, StartupSnapshot};

#[cfg(feature = "components")]
mod component;
#[cfg(feature = "components")]
pub use component::{splash, Splash, SplashBuilder, SplashProps};

#[cfg(feature = "components")]
#[doc(hidden)]
pub mod components {
    pub use crate::{splash, Splash, SplashBuilder, SplashProps};
    pub use jadix_components::components::*;
}

#[doc(hidden)]
pub use jadix as __jadix;

/// Build an RSX tree including the optional Splash component.
#[cfg(all(feature = "macros", feature = "components"))]
#[macro_export]
macro_rules! ui {
    ($($tokens:tt)*) => {
        $crate::__jadix::__ui_with_runtime!($crate::__jadix; components = $crate::components; $($tokens)*)
    };
}

/// Startup types alongside the selected Jadix authoring vocabulary.
pub mod prelude {
    #[cfg(all(feature = "macros", feature = "components"))]
    pub use crate::ui;
    #[cfg(feature = "components")]
    pub use crate::{splash, Splash, SplashBuilder, SplashProps};
    pub use crate::{SplashMode, StartupConfig, StartupController, StartupPhase, StartupSnapshot};
    #[cfg(not(feature = "components"))]
    pub use jadix::prelude::*;
    #[cfg(feature = "components")]
    pub use jadix_components::prelude::*;
}
