//! # Tooltip
//!
//! 锚点浮层提示：hover 触发器时显示；Pinned portal + 统一 floating sync。

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::reactive_view;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::r#box::{hstack, r#box};
use crate::primitives::text::text;
use crate::tokens;
use crate::utils::popup::{mark_floating, panel_block_clicks, PopupPlacement};

pub struct TooltipBuilder {
    id: String,
    content: String,
    open: Option<Signal<bool>>,
    placement: PopupPlacement,
}

pub fn tooltip(id: impl Into<String>, content: impl Into<String>) -> TooltipBuilder {
    TooltipBuilder {
        id: id.into(),
        content: content.into(),
        open: None,
        placement: PopupPlacement::BottomStart,
    }
}

impl TooltipBuilder {
    pub fn open_signal(mut self, s: Signal<bool>) -> Self {
        self.open = Some(s);
        self
    }

    pub fn placement(mut self, p: PopupPlacement) -> Self {
        self.placement = p;
        self
    }

    pub fn trigger(self, build: impl FnOnce(Signal<bool>) -> Element) -> Element {
        let open_key = format!("{}__tooltip_open", self.id);
        let open = self
            .open
            .unwrap_or_else(|| use_signal(open_key.as_str(), || false));
        let trigger_fid = format!("{}__tooltip_trigger", self.id);
        let mut trigger = build(open);
        if trigger.focus_id.is_none() {
            trigger = trigger.focus_id(trigger_fid.as_str());
        }
        let trigger_fid = trigger.focus_id.clone().unwrap_or(trigger_fid);
        let open_h = open;
        trigger = trigger.on_hover(move |hovered| open_h.set(hovered));

        let id = self.id.clone();
        let content = self.content.clone();
        let placement = self.placement;
        let view_id = format!("{id}__tooltip_view");
        let tip_open = open;

        let tip = reactive_view(view_id, move || {
            let is_open = tip_open.get();
            let Some(oz) = portal_sync(&id, tip_open, OverlayKind::Tooltip, OverlayMode::Pinned)
            else {
                return Element::new();
            };
            if !is_open {
                return Element::new();
            }
            let mut panel: Element = r#box().px("0.5rem").py("0.25rem").rounded("0.25rem").into();
            panel = panel.bg(tokens::primary());
            panel = panel.child(
                text(content.as_str())
                    .color(tokens::primary_foreground_text())
                    .size(12.0),
            );
            mark_floating(&mut panel, &id, &trigger_fid, placement);
            panel = panel.absolute();
            panel_block_clicks(panel, oz.panel)
        });

        hstack().child(trigger).child(tip).into()
    }
}

impl From<TooltipBuilder> for Element {
    fn from(builder: TooltipBuilder) -> Self {
        let label = builder.content.clone();
        builder.trigger(move |_open| {
            let mut el: Element = r#box().px("0.5rem").py("0.25rem").into();
            el = el.child(text(label.as_str()));
            el
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_builds_tree() {
        let _el: Element = tooltip("tip1", "Hello").into();
    }
}
