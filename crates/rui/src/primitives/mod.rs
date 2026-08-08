//! 原子原语层：声明式 UI 的底座。
//!
//! - **[`r#box`] / [`hstack`] / [`vstack`] / [`gstack`]**：布局与视觉容器。
//! - **[`interactive`]**：点击、悬停、键盘与焦点挂载。
//! - **[`overlay`]**：全屏模态遮罩（色 / 透明度 / backdrop 模糊 / 点击关闭）。
//! - **[`text`] / [`icon`]**：纯文本与图标叶子。
//! - **[`Portal`]**：树外挂载（Solid `<Portal>`）；定位 / Esc 栈仍在 [`crate::utils::popup`] /
//!   [`crate::app::portal_overlay`]。
//!
//! 拖拽、浮层几何、虚拟列表、内嵌文本等能力在 [`crate::utils`] / [`crate::app`]；
//! Tabs / Toggle 等在 [`crate::components`]。

pub mod r#box;
pub mod interactive;
pub mod overlay;
pub mod path;
pub mod portal;
pub mod text;

pub use interactive::{interactive, InteractiveBuilder};
pub use overlay::{overlay, OverlayBuilder};
pub use path::{
    attach_polyline, draw_polyline_batch_stub, encode_points, pie_sector, polygon, polyline,
    PathCommand, Point2,
};
pub use portal::{Portal, PortalProps};
pub use r#box::{gstack, hstack, r#box, vstack, BoxBuilder};
pub use text::{icon, text, TextBuilder};
