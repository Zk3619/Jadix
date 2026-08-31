//! Component-library private chrome（多消费者共享，非框架原语）。
//!
//! 仅放 **≥2 个已导出组件** 共用的实现。单组件策略/算术留在对应组件目录
//! (such as `components/toast/`, `components/carousel/`), do not insert "fake sinking" here.
//!
//! - [`modal_shell`]：Dialog / Sheet 共享 Portal 遮罩默认与 z 同步
//! - [`modal_motion`]: Dialog / Sheet / Drawer share fade in and slide out
//! - [`floating_motion`]: Popover / Menu / Tooltip / Select and other anchored floating layers fade in and expand
//! - [`text_edit`]: Input / Textarea shared chrome
//! - [`menu`]：DropdownMenu / ContextMenu 共享的 checkable / item chrome / 子树生命周期

pub(crate) mod floating_motion;
pub(crate) mod menu;
pub(crate) mod modal_motion;
pub(crate) mod modal_shell;
pub(crate) mod text_edit;
