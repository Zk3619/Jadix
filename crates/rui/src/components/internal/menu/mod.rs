//! 弹出菜单共享私有能力（DropdownMenu / ContextMenu）。
//!
//! 与 [`super::text_edit`] 同级：多消费者产品 chrome，不进 prelude / 产品清单。
//! Menubar 不直接依赖本目录（内容委托 DropdownMenu）。

pub(crate) mod checkable;
pub(crate) mod item_chrome;
pub(crate) mod subtree;
