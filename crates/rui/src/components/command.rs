//! # Command
//!
//! 命令面板：过滤输入 + 动作列表。对照 ShadCN Command / cmdk：**类型即搜**，
//! ↑↓ 移动高亮，Enter 执行，Esc 关闭（portal 栈顶）。
//!
//! ## 用法
//! ```ignore
//! // A · RSX（↑↓ 高亮 · Enter 执行 · Esc 关）
//! ui! {
//!     <Command id="pal" open={open_sig} placeholder="Type a command…" actions={actions} />
//! }
//! // B · Builder
//! command("pal")
//!     .open_signal(open_sig)
//!     .action(CommandAction::new("ping", "Ping", || log::info!("ping")))
//! ```

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::reactive_view;
use crate::components::button::button;
use crate::components::input::{input, input_value_signal_key};
use crate::components::scroll_area::scroll_area;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::r#box::vstack;
use crate::tokens;
use crate::utils::popup::{panel_block_clicks, portal_panel, underlay_close_on_click};
use crate::utils::roving::{roving_edge, roving_move};
use rui_macros::component;
use winit::keyboard::{Key, NamedKey};

/// 一条可过滤命令。
#[derive(Clone)]
pub struct CommandAction {
    pub id: String,
    pub label: String,
    pub run: std::rc::Rc<dyn Fn()>,
}

impl CommandAction {
    pub fn new(id: impl Into<String>, label: impl Into<String>, run: impl Fn() + 'static) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            run: std::rc::Rc::new(run),
        }
    }
}

/// RSX `<Command>`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Command(
    id: String,
    open: Option<Signal<bool>>,
    placeholder: Option<String>,
    actions: Option<Vec<CommandAction>>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = command(&id);
    if let Some(s) = open {
        b = b.open_signal(s);
    }
    if let Some(p) = placeholder {
        b = b.placeholder(p);
    }
    if let Some(a) = actions {
        b = b.actions(a);
    }
    b.into()
}

pub struct CommandBuilder {
    id: String,
    open: Option<Signal<bool>>,
    actions: Vec<CommandAction>,
    placeholder: String,
}

/// 命令面板；`open` 为 true 时居中浮层显示。
pub fn command(id: impl Into<String>) -> CommandBuilder {
    CommandBuilder {
        id: id.into(),
        open: None,
        actions: Vec::new(),
        placeholder: "Type a command…".into(),
    }
}

impl CommandBuilder {
    pub fn open_signal(mut self, s: Signal<bool>) -> Self {
        self.open = Some(s);
        self
    }
    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = s.into();
        self
    }
    pub fn actions(mut self, actions: Vec<CommandAction>) -> Self {
        self.actions = actions;
        self
    }
    pub fn action(mut self, action: CommandAction) -> Self {
        self.actions.push(action);
        self
    }
}

impl From<CommandBuilder> for Element {
    fn from(builder: CommandBuilder) -> Self {
        let CommandBuilder {
            id,
            open,
            actions,
            placeholder,
        } = builder;
        let id = crate::utils::resolve_instance_id(id, "command");
        let open = open.unwrap_or_else(|| use_signal(format!("{id}__cmd_open").as_str(), || false));
        let input_id = format!("{id}__filter");
        let active = use_signal(format!("{id}__cmd_active").as_str(), || 0usize);
        let view_id = format!("{id}__cmd_view");

        reactive_view(view_id, move || {
            let _ = open.get();
            let Some(oz) = portal_sync(&id, open, OverlayKind::Modal, OverlayMode::Stacked) else {
                return Element::new();
            };
            if !open.peek() {
                return Element::new();
            }

            let query_sig = use_signal(input_value_signal_key(&input_id).as_str(), String::new);
            let query = query_sig.get().to_ascii_lowercase();
            let filtered: Vec<_> = actions
                .iter()
                .filter(|a| query.is_empty() || a.label.to_ascii_lowercase().contains(&query))
                .cloned()
                .collect();
            let n = filtered.len();
            if n == 0 {
                active.set(0);
            } else if active.peek() >= n {
                active.set(n - 1);
            }
            let hi = active.get().min(n.saturating_sub(1));

            let open_esc = open;
            let active_k = active;
            let filtered_k = filtered.clone();
            let mut filter: Element = input(&input_id).placeholder(placeholder.as_str()).into();
            filter = filter.on_key_down(move |ev| {
                match &ev.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        open_esc.set(false);
                        crate::core::reactivity::use_signal("__rui_button_focus_id", String::new)
                            .set(String::new());
                        return true;
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        if let Some(next) = roving_move(filtered_k.len(), active_k.peek(), 1) {
                            active_k.set(next);
                        }
                        return true;
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        if let Some(next) = roving_move(filtered_k.len(), active_k.peek(), -1) {
                            active_k.set(next);
                        }
                        return true;
                    }
                    Key::Named(NamedKey::Home) => {
                        if let Some(i) = roving_edge(filtered_k.len(), false) {
                            active_k.set(i);
                        }
                        return true;
                    }
                    Key::Named(NamedKey::End) => {
                        if let Some(i) = roving_edge(filtered_k.len(), true) {
                            active_k.set(i);
                        }
                        return true;
                    }
                    Key::Named(NamedKey::Enter) => {
                        if let Some(a) = filtered_k.get(active_k.peek()) {
                            (a.run)();
                            open_esc.set(false);
                            crate::core::reactivity::use_signal(
                                "__rui_button_focus_id",
                                String::new,
                            )
                            .set(String::new());
                        }
                        return true;
                    }
                    _ => {}
                }
                false
            });

            let mut rows = vstack().w("100%").gap("0.125rem");
            for (i, action) in filtered.iter().enumerate() {
                let run = action.run.clone();
                let o_sig = open;
                let a_sig = active;
                let bid = format!("{id}:act{i}");
                let row: Element = button(&bid)
                    .variant(if i == hi { "secondary" } else { "ghost" })
                    .size("sm")
                    .w("100%")
                    .on_click(move || {
                        a_sig.set(i);
                        run();
                        o_sig.set(false);
                        crate::core::reactivity::use_signal("__rui_button_focus_id", String::new)
                            .set(String::new());
                    })
                    .text(action.label.clone())
                    .into();
                rows = rows.child(row);
            }

            let scroll_id = format!("{id}:list");
            let list: Element = scroll_area(&scroll_id)
                .overflow_y("auto")
                .w("100%")
                .max_h("16rem")
                .child(rows)
                .into();

            let mut panel: Element = vstack()
                .gap("0.5rem")
                .p("0.75rem")
                .w("24rem")
                .child(filter)
                .child(list)
                .into();
            panel = panel
                .bg(tokens::background())
                .rounded("0.5rem")
                .border_style("solid")
                .border_width(1.0)
                .border_color(tokens::border());

            let mut root = vstack().child(underlay_close_on_click(open, oz.underlay));
            root = root.child(portal_panel(panel_block_clicks(panel, oz.panel), oz.panel));
            root.into()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_action_holds_label() {
        let a = CommandAction::new("ping", "Ping host", || {});
        assert_eq!(a.id, "ping");
        assert_eq!(a.label, "Ping host");
    }
}
