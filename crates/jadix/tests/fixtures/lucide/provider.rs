pub use ui_runtime::{app, authoring, core, media, primitives, style};

pub mod utils {
    pub use ui_runtime::utils::*;
    pub mod lucide;
    pub mod lucide_animated;
    pub use lucide::{lucide_glyph, lucide_svg_inner, ATTR_ICON_NAME};
    pub use lucide_animated::*;
}

pub fn install() {
    ui_runtime::utils::font::register_font_data(include_bytes!("../assets/lucide.ttf").to_vec()).unwrap();
    ui_runtime::utils::set_icon_provider(std::sync::Arc::new(utils::lucide::DefaultProvider));
}

#[doc(hidden)]
pub use ui_runtime as __runtime;
#[macro_export]
macro_rules! ui {
    ($($tokens:tt)*) => {
        $crate::__runtime::__ui_with_runtime!($crate::__runtime; icons = $crate::utils; $($tokens)*)
    };
}
