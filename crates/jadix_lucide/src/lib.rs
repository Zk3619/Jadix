pub mod catalog;
pub use catalog::*;
#[cfg(feature = "lucide-animated")]
pub mod recipes;
#[cfg(feature = "macros")]
pub use jadix as __runtime;
#[cfg(feature = "ui")]
pub use jadix::{app, authoring, core, layout, media, primitives, style};
#[cfg(feature = "lucide-animated")]
pub use recipes::*;

#[cfg(feature = "ui")]
pub mod utils {
    pub use jadix::utils::*;
    pub mod lucide;
    #[cfg(feature = "lucide-animated")]
    pub mod lucide_animated;
}
#[cfg(feature = "ui")]
pub use utils::lucide::lucide_glyph;
#[cfg(feature = "lucide-animated")]
pub use utils::lucide_animated::*;

#[cfg(feature = "ui")]
pub fn install() -> Result<(), String> {
    jadix::utils::font::register_font_data(FONT_DATA.to_vec())?;
    jadix::utils::icon_provider::set_icon_provider(std::sync::Arc::new(
        utils::lucide::DefaultProvider,
    ));
    Ok(())
}

#[cfg(feature = "macros")]
#[macro_export]
macro_rules! ui {
    ($($tokens:tt)*) => {
        $crate::__runtime::__ui_with_runtime!($crate::__runtime; icons = $crate::utils::lucide_animated; $($tokens)*)
    };
}
