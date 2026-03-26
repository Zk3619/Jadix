//! 样式子系统：
//! - [`Style`]：盒模型 + 与 Taffy 对齐的字段 + 矩形绘制（背景/边框/阴影）；
//! - [`TextStyle`] / [`TextDecorationStyle`]：文本排版与装饰线（见 [`text_style`] 模块）；
//! - 语义色解析。
//!
//! 布局遍历见 [`crate::layout::layout_tree`]。

mod theme;
mod color;
mod element_style;
mod animation;
mod text_style;

pub use animation::{tick_animations, use_spring, Spring, SpringState};
pub use color::{parse_color, parse_text_color};
pub use theme::{get_theme, DesignTokens, Theme};
pub use element_style::Style;
pub use text_style::{InputCaretDraw, TextDecorationStyle, TextStyle};
