//! # Toast
//!
//! 顶栏感视觉提示条（primary 底、圆角）。**仅绘制**，无队列、无定时消失、无全局 API。
//!
//! ## 用法
//! ```ignore
//! <Show when={show.get()}>{ toast("已保存") }</Show>
//! ```
//!
//! ## 注意
//! 可见性、位置、自动关闭均由父级 `Show` / `reactive!` 控制；默认 `z_index=1100`。

use crate::core::tree::Element;
use crate::primitives::r#box::r#box;
use crate::primitives::text::text;
use crate::tokens;

/// 顶栏感 toast 条（仅视觉）。见模块文档。
pub fn toast(message: impl Into<String>) -> Element {
    let msg: String = message.into();
    let mut el: Element = r#box()
        .px("1rem")
        .py("0.5rem")
        .rounded("0.375rem")
        .mt("0.75rem")
        .into();
    el.style.z_index = 1100;
    el = el.bg(tokens::primary());
    let label: Element = text(msg.as_str())
        .color(tokens::primary_foreground_text())
        // shadcn Toast 描述区常用 `text-sm`
        .size(0.875 * 16.0)
        .into();
    el.child(label)
}
