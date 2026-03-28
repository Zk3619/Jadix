use crate::components::button::button;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::popup::{register_last_popup_for_escape, underlay_close_on_click};
use crate::primitives::r#box::vstack;
use crate::tokens;
use rui_macros::component;

pub struct SelectBuilder {
    id: String,
    value: Option<Signal<String>>,
    default_value: String,
    options: Vec<(String, String)>,
    placeholder: String,
    children: Vec<Element>,
}

/// ShadCN-style select: outline trigger + optional overlay menu. The root builder must read `open` /
/// `value` (via `.get()` here) so the menu toggles when those signals change; use `reactive!` for a
/// stable subtree if this block sits under minimal parent subscriptions.
/// Select component with overlay menu.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Select(
    id: String,
    value: Option<Signal<String>>,
    default_value: String,
    options: Vec<(String, String)>,
    placeholder: String,
    children: Option<Element>,
) -> Element {
    let mut builder = select(&id)
        .default_value(default_value)
        .options(options)
        .placeholder(placeholder);
    
    if let Some(sig) = value {
        builder = builder.value_signal(sig);
    }

    if let Some(c) = children {
        builder = builder.child(c);
    }

    builder.into()
}

pub fn select(id: impl Into<String>) -> SelectBuilder {
    SelectBuilder {
        id: id.into(),
        value: None,
        default_value: String::new(),
        options: Vec::new(),
        placeholder: "Select…".into(),
        children: Vec::new(),
    }
}

impl SelectBuilder {
    pub fn value_signal(mut self, s: Signal<String>) -> Self {
        self.value = Some(s);
        self
    }

    pub fn default_value(mut self, v: impl Into<String>) -> Self {
        self.default_value = v.into();
        self
    }

    pub fn options(mut self, pairs: Vec<(String, String)>) -> Self {
        self.options = pairs;
        self
    }

    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }

    pub fn child(mut self, c: Element) -> Self {
        self.children.push(c);
        self
    }

    pub fn children(mut self, c: Vec<Element>) -> Self {
        self.children.extend(c);
        self
    }
}

impl From<SelectBuilder> for Element {
    fn from(builder: SelectBuilder) -> Self {
        let open_key = format!("{}__select_open", builder.id);
        let val_key = format!("{}__select_value", builder.id);
        let open = use_signal(open_key.as_str(), || false);
        let default_initial = if !builder.default_value.is_empty() {
            builder.default_value.clone()
        } else {
            builder
                .options
                .first()
                .map(|(v, _)| v.clone())
                .unwrap_or_default()
        };
        let val = builder
            .value
            .unwrap_or_else(|| use_signal(val_key.as_str(), move || default_initial.clone()));

        register_last_popup_for_escape(open);

        let current = val.get();
        let label = builder
            .options
            .iter()
            .find(|(v, _)| v == &current)
            .map(|(_, l)| l.as_str())
            .filter(|_| !current.is_empty())
            .unwrap_or(builder.placeholder.as_str());

        let open_t = open;
        let trigger_id = format!("{}:trigger", builder.id);
        let trigger: Element = button(&trigger_id)
            .variant("outline")
            .w("12rem")
            .on_click(move || open_t.set(!open_t.peek()))
            .text(label.to_string())
            .into();

        let mut root: Element = vstack().gap("0.25rem").into();
        root = root.relative();

        root = root.child(trigger);

        if open.get() {
            let u = underlay_close_on_click(open);
            root = root.child(u);

            let mut menu: Element = vstack()
                .gap("2px")
                .p("4px")
                .rounded("0.375rem")
                .min_w("12rem")
                .into();
            menu = menu.bg(tokens::background());
            menu.style.z_index = 960;
            menu.style.border_width = 1.0;
            menu.style.border_color = tokens::border();

            for (idx, (opt_v, opt_l)) in builder.options.into_iter().enumerate() {
                let v_sig = val;
                let o_sig = open;
                let ov = opt_v.clone();
                let bid = format!("{}:opt:{idx}", builder.id);
                let row: Element = button(&bid)
                    .variant("ghost")
                    .size("sm")
                    .on_click(move || {
                        v_sig.set(ov.clone());
                        o_sig.set(false);
                    })
                    .text(opt_l)
                    .into();
                menu = menu.child(row);
            }
            root = root.child(menu);
        }

        root.template_id = Some(builder.id);
        root
    }
}
