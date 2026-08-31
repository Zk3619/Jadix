//! 弹出菜单共享私有能力（DropdownMenu / ContextMenu）。
//!
//! Same level as [`super::text_edit`]: multi-consumer product chrome, not included in prelude/product list.
//! Menubar 不直接依赖本目录（内容委托 DropdownMenu）。

pub(crate) mod checkable;
pub(crate) mod item_chrome;
pub(crate) mod subtree;
