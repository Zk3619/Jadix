#[cfg(feature = "gpu")]
pub use jadix::authoring::widgets::navigation_chevron as chrome_nav_chevron;
pub use jadix::*;
pub mod schema;
#[cfg(feature = "gpu")]
pub use jadix::{app, authoring, core, layout, media, motion, primitives, renderer, style, text};

#[cfg(feature = "gpu")]
pub mod components;
#[cfg(not(feature = "gpu"))]
#[path = "portable.rs"]
pub mod components;
pub use components::*;

pub mod utils {
    pub use jadix::utils::*;
    #[cfg(feature = "dnd")]
    pub use jadix_dnd::utils::drag;
}

pub mod prelude {
    pub use crate::components::*;
    #[cfg(feature = "gpu")]
    pub use crate::components::{image, marker};
    #[cfg(feature = "macros")]
    pub use crate::ui;
    pub use jadix::prelude::*;
    pub use jadix::primitives::text;
}

#[cfg(feature = "macros")]
#[macro_export]
macro_rules! ui {
    ($($tokens:tt)*) => {
        $crate::__ui_with_runtime!($crate; components = $crate::components; dnd = $crate::utils::drag; $($tokens)*)
    };
}
