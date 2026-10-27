//! # Form
//!
//! 轻量表单 Context helpers（**experimental**，非独立产品组件；WP-3.9）：
//! 错误 map + [`form_field`] 错误文案与 Input/Textarea **invalid 环同源**。
//!
//! - [`form_provider`]：实例 [`component_root`](crate::core::reactivity::component_root) + 错误 map。
//! - 点击回调请 [`use_form`] 抓住 [`FormState`] 再 `set_field_error`（不要在事件里指望 `current_owner`）。
//! - 字段 chrome 走 [`bind_prop`](crate::app::reactive::bind_prop)，禁止根裸 `get()` 整表重建。

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::app::reactive::bind_prop;
use crate::components::input::apply_bound_input_invalid;
use crate::core::reactivity::{
    component_root, create_context, provide_context, use_context, use_signal, Context, Signal,
};
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::r#box::vstack;
use crate::primitives::text::text;
use crate::style::parse_text_color;

#[derive(Clone)]
pub struct FormState {
    pub errors: Signal<HashMap<String, String>>,
}

impl FormState {
    pub fn set_field_error(&self, name: &str, message: impl Into<String>) {
        let mut map = self.errors.peek();
        map.insert(name.to_string(), message.into());
        self.errors.set(map);
    }

    pub fn clear_field_error(&self, name: &str) {
        let mut map = self.errors.peek();
        if map.remove(name).is_some() {
            self.errors.set(map);
        }
    }

    pub fn clear_all(&self) {
        if !self.errors.peek().is_empty() {
            self.errors.set(HashMap::new());
        }
    }

    /// 先清空，再跑 `validate`；无错误返回 `true`。
    pub fn validate(&self, validate: impl FnOnce()) -> bool {
        self.clear_all();
        validate();
        self.errors.peek().is_empty()
    }

    pub fn message(&self, name: &str) -> Option<String> {
        self.errors.peek().get(name).cloned()
    }

    pub fn is_invalid(&self, name: &str) -> bool {
        self.errors.peek().contains_key(name)
    }
}

fn form_context() -> Context<FormState> {
    static CTX: OnceLock<Context<FormState>> = OnceLock::new();
    *CTX.get_or_init(create_context::<FormState>)
}

pub fn form_errors_signal(id: &str) -> Signal<HashMap<String, String>> {
    use_signal(
        &format!("{id}__form_errors"),
        HashMap::<String, String>::new,
    )
}

/// 当前 [`form_provider`] 的 [`FormState`]（渲染期 / 同 Owner）。事件回调请在构建时 `clone`。
pub fn use_form() -> Option<FormState> {
    use_context(form_context())
}

/// Provide a form error map under `id` for descendant fields.
pub fn form_provider(id: impl Into<String>, body: impl FnOnce() -> Element) -> Element {
    component_root(|| {
        let id = id.into();
        let errors = form_errors_signal(&id);
        provide_context(form_context(), FormState { errors });
        let mut root: Element = vstack().w("100%").child(body()).into();
        root.template_id = Some(format!("{id}__form"));
        root
    })
}

pub fn set_field_error(name: &str, message: impl Into<String>) {
    if let Some(st) = use_form() {
        st.set_field_error(name, message);
    }
}

pub fn clear_field_error(name: &str) {
    if let Some(st) = use_form() {
        st.clear_field_error(name);
    }
}

pub fn clear_all_errors() {
    if let Some(st) = use_form() {
        st.clear_all();
    }
}

/// Validate with a closure; returns true when all checks pass (no errors left).
pub fn form_validate(validate: impl FnOnce()) -> bool {
    use_form().map(|st| st.validate(validate)).unwrap_or(true)
}

/// 跟踪读：错误文案（供 `reactive_view` / bind）。无表单或无该字段 → `None`。
pub fn field_error(name: &str) -> Option<String> {
    use_form().and_then(|st| st.errors.get().get(name).cloned())
}

/// 跟踪读：该字段是否有错误（与 Input invalid 同源）。
pub fn field_invalid(name: &str) -> bool {
    field_error(name).is_some()
}

fn error_message_snapshot(name: &str) -> String {
    use_form()
        .and_then(|st| st.message(name))
        .unwrap_or_default()
}

fn sync_error_text(el: &mut Element, msg: &str) -> LayoutStatus {
    let next = if msg.is_empty() {
        None
    } else {
        Some(msg.to_string())
    };
    if el.text == next {
        return LayoutStatus::Clean;
    }
    el.text = next;
    LayoutStatus::LayoutChanged
}

fn error_text_el(name: &str, msg: &str) -> Element {
    let mut el: Element = text(msg)
        .size(12.0)
        .color(parse_text_color("destructive"))
        .into();
    el.template_id = Some(format!("{name}__form_err"));
    if msg.is_empty() {
        el.text = None;
    }
    let field = name.to_string();
    el.reactive_sync(move |el, _tree: &mut LayoutTree| {
        let live = use_form()
            .and_then(|st| st.message(&field))
            .unwrap_or_default();
        sync_error_text(el, &live)
    })
}

fn apply_control_invalid(el: &mut Element, invalid: bool) -> LayoutStatus {
    let mut status = LayoutStatus::Clean;
    if el.input_paint_sync.is_some() {
        status = status.merge(apply_bound_input_invalid(el, invalid));
    }
    for child in &mut el.children {
        status = status.merge(apply_control_invalid(child, invalid));
    }
    status
}

fn bind_field_invalid(child: Element, name: &str) -> Element {
    let bind_key = format!("{name}__form_invalid");
    let field = name.to_string();
    bind_prop(
        child,
        bind_key,
        move || field_invalid(&field),
        |el, invalid| apply_control_invalid(el, invalid),
    )
}

pub fn form_field(name: impl Into<String>, label: impl Into<String>, child: Element) -> Element {
    let name = name.into();
    let label_s = label.into();
    let child = bind_field_invalid(child, &name);

    let err_el = error_text_el(&name, &error_message_snapshot(&name));

    let mut col = vstack().w("100%").gap(4.0);
    col = col.child(text(label_s).size(13.0));
    col = col.child(child);
    col = col.child(err_el);
    col.into()
}

#[cfg(all(test, feature = "product-tests"))]
mod tests {
    use super::*;
    use crate::components::input::input;
    use crate::core::reactivity::{create_effect, create_root};

    fn tree_has_text(el: &Element, s: &str) -> bool {
        el.text.as_deref() == Some(s) || el.children.iter().any(|c| tree_has_text(c, s))
    }

    fn find_input_invalid(el: &Element) -> Option<bool> {
        if let Some(sync) = &el.input_paint_sync {
            return Some(sync.invalid);
        }
        el.children.iter().find_map(find_input_invalid)
    }

    #[test]
    fn form_validate_sets_and_clears_errors() {
        let (_owner, _) = create_root(|| {
            let errors = form_errors_signal("f1");
            provide_context(form_context(), FormState { errors });
            assert!(form_validate(|| {}));
            assert!(!form_validate(|| set_field_error("email", "bad")));
            assert!(form_validate(|| {}));
        });
    }

    #[test]
    fn form_field_error_text_matches_input_invalid() {
        let (_owner, _) = create_root(|| {
            form_provider("wp39-a", || {
                set_field_error("email", "必填");
                let field = form_field("email", "邮箱", Element::from(input("wp39-email")));
                assert!(
                    tree_has_text(&field, "必填"),
                    "error copy must render next to the field"
                );
                assert_eq!(
                    find_input_invalid(&field),
                    Some(true),
                    "Input invalid chrome must follow the error map"
                );
                Element::new()
            });
            form_provider("wp39-b", || {
                let field = form_field("email", "邮箱", Element::from(input("wp39-email-ok")));
                assert!(
                    !tree_has_text(&field, "必填"),
                    "sibling form must not inherit the other map"
                );
                assert_eq!(find_input_invalid(&field), Some(false));
                Element::new()
            });
        });
    }

    #[test]
    fn form_field_under_effect_does_not_overflow() {
        let (_owner, _) = create_root(|| {
            create_effect(|| {
                crate::core::reactivity::isolate_reads(|| {
                    let _ = crate::core::reactivity::component_root(|| {
                        form_provider("wp39-fx", || {
                            form_field("email", "邮箱", Element::from(input("wp39-fx-email")))
                        })
                    });
                });
            });
        });
    }

    #[test]
    fn form_state_clear_field_drops_invalid() {
        let (_owner, _) = create_root(|| {
            form_provider("wp39-c", || {
                set_field_error("email", "bad");
                clear_field_error("email");
                let field = form_field("email", "邮箱", Element::from(input("wp39-email-clr")));
                assert!(!tree_has_text(&field, "bad"));
                assert_eq!(find_input_invalid(&field), Some(false));
                Element::new()
            });
        });
    }
}
