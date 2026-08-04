//! 原子原语层：声明式 UI 的底座。
//!
//! - **[`r#box`] / [`hstack`] / [`vstack`] / [`gstack`]**：布局与视觉容器。
//! - **[`interactive`]**：点击、悬停、键盘与焦点挂载。
//! - **[`text`] / [`icon`]**：纯文本与图标叶子。
//!
//! 拖拽、浮层、虚拟列表、内嵌文本等能力在 [`crate::utils`] / [`crate::app`]；
//! Tabs / Toggle 等在 [`crate::components`]。

pub mod r#box;
pub mod interactive;
pub mod text;

pub use interactive::{interactive, InteractiveBuilder};
pub use r#box::{gstack, hstack, r#box, vstack, BoxBuilder};
pub use text::{icon, text, TextBuilder};
