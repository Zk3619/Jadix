use crate::core::tree::Element;
use crate::primitives::r#box::r#box;
use crate::primitives::text::text;
use crate::tokens;

/// Fixed top-center toast strip (visual only; visibility should be driven by parent `Show` / `reactive!`).
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
    el = el.child(label);
    el
}
