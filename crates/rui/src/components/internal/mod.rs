//! Component-library private chrome（多消费者共享，非框架原语）。
//!
//! 仅放 **≥2 个已导出组件** 共用的实现。单组件策略/算术留在对应组件目录
//!（如 `components/toast/`、`components/carousel/`），勿再以「假下沉」塞进此处。
//!
//! Dialog 模态壳已并入 [`crate::components::dialog`]；此处保留：
//! - [`text_edit`]：Input / Textarea 共享 chrome
//! - [`menu`]：DropdownMenu / ContextMenu 共享的 checkable / item chrome / 子树生命周期

pub(crate) mod menu;
pub(crate) mod text_edit;
