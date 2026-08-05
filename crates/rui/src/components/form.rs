//! # Form
//!
//! 轻量表单上下文：errors map + field 错误展示；依赖 Context API。

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, Context, Signal,
};
use crate::core::tree::Element;
use crate::primitives::r#box::vstack;
use crate::primitives::text::text;

#[derive(Clone)]
pub struct FormState {
    pub errors: Signal<HashMap<String, String>>,
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

/// Provide a form error map under `id` for descendant fields.
pub fn form_provider(id: impl Into<String>, body: impl FnOnce() -> Element) -> Element {
    let id = id.into();
    let errors = form_errors_signal(&id);
    provide_context(form_context(), FormState { errors });
    let mut root: Element = vstack().w("100%").child(body()).into();
    root.template_id = Some(format!("{id}__form"));
    root
}

pub fn set_field_error(name: &str, message: impl Into<String>) {
    if let Some(st) = use_context(form_context()) {
        let mut map = st.errors.get();
        map.insert(name.to_string(), message.into());
        st.errors.set(map);
    }
}

pub fn clear_field_error(name: &str) {
    if let Some(st) = use_context(form_context()) {
        let mut map = st.errors.get();
        map.remove(name);
        st.errors.set(map);
    }
}

pub fn clear_all_errors() {
    if let Some(st) = use_context(form_context()) {
        st.errors.set(HashMap::new());
    }
}

/// Validate with a closure; returns true when all checks pass (no errors left).
pub fn form_validate(validate: impl FnOnce()) -> bool {
    clear_all_errors();
    validate();
    use_context(form_context())
        .map(|st| st.errors.peek().is_empty())
        .unwrap_or(true)
}

pub fn form_field(name: impl Into<String>, label: impl Into<String>, child: Element) -> Element {
    let name = name.into();
    let label_s = label.into();
    let err = use_context(form_context())
        .and_then(|st| st.errors.get().get(&name).cloned())
        .unwrap_or_default();

    let mut col = vstack().w("100%").gap(4.0);
    col = col.child(text(label_s).size(13.0));
    col = col.child(child);
    if !err.is_empty() {
        let err_el: Element = text(err).size(12.0).color([180, 40, 40, 255]).into();
        col = col.child(err_el);
    }
    col.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::create_root;

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
}
