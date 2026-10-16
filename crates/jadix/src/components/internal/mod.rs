//! Component-library private chrome（多消费者共享，非框架原语）。
//!
//! 仅放 **≥2 个已导出组件** 共用的实现。单组件策略/算术留在对应组件目录
//! (such as `components/toast/`, `components/carousel/`), do not insert "fake sinking" here.
//! 作者面原语（`utils/collapse_panel`、`presence`、`motion`）也不进这里。
//! fade/slide/expand 引擎在 [`crate::utils::modal_motion`] / [`crate::utils::floating_motion`]
//! 的 `engine` 子模块；遮罩默认与 z 同步在 [`crate::utils::modal_shell`]。
//!
//! - [`text_edit`]: Input / Textarea shared chrome
//! - [`menu`]：DropdownMenu / ContextMenu 共享的 checkable / item chrome / 子树生命周期

pub(crate) mod menu;
pub(crate) mod text_edit;
