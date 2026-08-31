//! # Kbd
//!
//! ShadCN-style keyboard key capsules. Aligns with
//! `Reference/ShadCN-UI/.../bases/base/ui/kbd.tsx`.
//!
//! Chrome (Kbd): `pointer-events-none inline-flex h-5 w-fit min-w-5 items-center
//! justify-center gap-1 rounded-sm bg-muted px-1 text-xs font-medium
//! text-muted-foreground` (+ select-none / display-only).
//! KbdGroup: `inline-flex items-center gap-1`.
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags
//! <Kbd>"⌘"</Kbd>
//! <KbdGroup>
//!     <Kbd>"Ctrl"</Kbd>
//!     <Kbd>"K"</Kbd>
//! </KbdGroup>
//!
//! // B · Builder
//! Element::from(kbd().text("⌘"))
//! Element::from(
//!     kbd_group()
//!         .child(kbd().text("Ctrl"))
//!         .child(kbd().text("K"))
//! )
//! ```
//!
//! ## Notes
//! - **Presentational**: empty `id` omits `template_id` (WP-2.8).
//! - `<Kbd>` / `<KbdGroup>` use the general PascalCase → `*Props` path (same as Badge /
//!   Separator / ButtonGroup; no special `ui!` scope).
//! - Distinct from menu `*Shortcut` slots (those are inline muted captions inside menus).
//! - `ui!` multi-child packs into `rui_fragment` / anonymous shell; both roots use `take_packed` then lay out in a row.

use crate::core::tree::{parse_color, parse_text_color, Element, IntoF32};
use crate::primitives::r#box::{hstack, BoxBuilder};
use crate::tokens;
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use crate::utils::take_packed;
use rui_macros::component;

const ATTR_SLOT: &str = "data-slot";
const SLOT_KBD: &str = "kbd";
const SLOT_KBD_GROUP: &str = "kbd-group";
fn paint_caption(el: &mut Element, color: [u8; 4]) {
    if el.text.is_some() {
        el.text_style.color = color;
    }
    for child in &mut el.children {
        paint_caption(child, color);
    }
}

fn kbd_text_style() -> TextNodeConfig {
    TextNodeConfig {
        size_px: IntoF32::into_f32("0.75rem"),
        weight: 500,
        color: parse_text_color("muted-foreground"),
        underline: false,
        underline_offset_px: None,
        text_align: crate::style::TextAlign::Center,
        line_height_px: None,
        leading_scale: None,
        font_family: None,
    }
}

fn attach_kbd_children(mut el: Element, content: Vec<Element>, text_style: &TextNodeConfig) -> Element {
    let fg = text_style.color;
    for mut child in content {
        if let Some(text_content) = child.text.take() {
            let preserve_binding = child.template_id.is_some() || child.on_unmount.is_some();
            let is_lucide = child.text_style.font_family.as_deref() == Some("lucide");
            let size_px = if is_lucide {
                // ShadCN [&_svg:not([class*='size-'])]:size-3 → 0.75rem default
                if child.text_style.size > 1.0 {
                    child.text_style.size
                } else {
                    IntoF32::into_f32("0.75rem")
                }
            } else {
                text_style.size_px
            };
            let mut text_node = build_text_node(
                text_content,
                TextNodeConfig {
                    color: fg,
                    font_family: child.text_style.font_family.clone(),
                    size_px,
                    weight: if is_lucide { 400 } else { text_style.weight },
                    ..text_style.clone()
                },
            );
            if !is_lucide {
                text_node = text_node.whitespace_nowrap();
            }
            if preserve_binding {
                if let Some(tid) = child.template_id.take() {
                    text_node.template_id = Some(tid);
                }
                if let Some(hook) = child.on_unmount.take() {
                    text_node.on_unmount = Some(hook);
                }
            }
            el = el.child(text_node);
        } else {
            if child.text_style.font_family.as_deref() == Some("lucide")
                && child.text_style.size <= 1.0
            {
                child = child.text_size(IntoF32::into_f32("0.75rem"));
            }
            paint_caption(&mut child, fg);
            el = el.child(child);
        }
    }
    el
}

// ---------------------------------------------------------------------------
// Kbd
// ---------------------------------------------------------------------------

pub struct KbdBuilder {
    id: String,
    container: BoxBuilder,
    content: Vec<Element>,
    text_style: TextNodeConfig,
}

/// Builder entry: presentational keycap (no required user id).
pub fn kbd() -> KbdBuilder {
    KbdBuilder {
        id: String::new(),
        // ShadCN: inline-flex h-5 w-fit min-w-5 items-center justify-center gap-1
        // rounded-sm bg-muted px-1 text-xs font-medium text-muted-foreground
        container: hstack()
            .items_center()
            .justify_center()
            .gap("0.25rem")
            .h("1.25rem")
            .min_h("1.25rem")
            .min_w("1.25rem")
            .w("auto")
            .px("0.25rem")
            .rounded(tokens::RADIUS_SM)
            .flex_shrink(0.0),
        content: Vec::new(),
        text_style: kbd_text_style(),
    }
}

/// RSX `<Kbd>…</Kbd>`.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Kbd(id: String, children: Option<Element>) -> Element {
    let mut builder = kbd();
    if !id.trim().is_empty() {
        builder = builder.id(id);
    }
    if let Some(c) = children {
        for kid in take_packed(c) {
            builder = builder.child(kid);
        }
    }
    builder.into()
}

impl KbdBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.content.push(Element::new().text(content.into()));
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.content.extend(take_packed(child.into()));
        self
    }

    pub fn children(mut self, kids: impl IntoIterator<Item = Element>) -> Self {
        for c in kids {
            self = self.child(c);
        }
        self
    }
}

impl crate::core::tree::IntoElementList for KbdBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<KbdBuilder> for Element {
    fn from(builder: KbdBuilder) -> Self {
        let mut el: Element = builder.container.bg(parse_color("muted")).into();
        el = el.pointer_events_none();
        el.attrs
            .insert(ATTR_SLOT.to_string(), SLOT_KBD.to_string());
        if !builder.id.trim().is_empty() {
            el.template_id = Some(builder.id.clone());
        }
        attach_kbd_children(el, builder.content, &builder.text_style)
    }
}

// ---------------------------------------------------------------------------
// KbdGroup
// ---------------------------------------------------------------------------

pub struct KbdGroupBuilder {
    id: String,
    content: Vec<Element>,
}

/// Builder entry for a horizontal cluster of [`Kbd`] keycaps.
pub fn kbd_group() -> KbdGroupBuilder {
    KbdGroupBuilder {
        id: String::new(),
        content: Vec::new(),
    }
}

/// RSX `<KbdGroup>…</KbdGroup>`.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn KbdGroup(id: String, children: Option<Element>) -> Element {
    let mut builder = kbd_group();
    if !id.trim().is_empty() {
        builder = builder.id(id);
    }
    if let Some(c) = children {
        for kid in take_packed(c) {
            builder = builder.child(kid);
        }
    }
    builder.into()
}

impl KbdGroupBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.content.extend(take_packed(child.into()));
        self
    }

    pub fn children(mut self, kids: impl IntoIterator<Item = Element>) -> Self {
        for c in kids {
            self = self.child(c);
        }
        self
    }

    /// Convenience: append a keycap with plain text.
    pub fn kbd(mut self, label: impl Into<String>) -> Self {
        self.content.push(kbd().text(label).into());
        self
    }
}

impl crate::core::tree::IntoElementList for KbdGroupBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<KbdGroupBuilder> for Element {
    fn from(builder: KbdGroupBuilder) -> Self {
        // ShadCN: inline-flex items-center gap-1
        let mut el: Element = hstack()
            .items_center()
            .gap("0.25rem")
            .w("auto")
            .h("auto")
            .flex_shrink(0.0)
            .into();
        el.attrs
            .insert(ATTR_SLOT.to_string(), SLOT_KBD_GROUP.to_string());
        if !builder.id.trim().is_empty() {
            el.template_id = Some(builder.id.clone());
        }
        for child in builder.content {
            el = el.child(child);
        }
        el
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kbd_sets_slot_and_muted_chrome() {
        let el: Element = kbd().text("⌘").into();
        assert_eq!(
            el.attrs.get(ATTR_SLOT).map(|s| s.as_str()),
            Some(SLOT_KBD)
        );
        assert_eq!(el.style.bg_color, parse_color("muted"));
        assert_eq!(
            el.attrs
                .get(crate::core::tree::ATTR_POINTER_EVENTS)
                .map(|s| s.as_str()),
            Some("none")
        );
        let leaf = el
            .children
            .iter()
            .find(|c| c.text.as_deref() == Some("⌘"))
            .expect("caption leaf");
        assert_eq!(leaf.text_style.color, parse_text_color("muted-foreground"));
        assert_eq!(leaf.text_style.weight, 500);
    }

    #[test]
    fn kbd_empty_id_does_not_write_empty_template() {
        let el: Element = kbd().text("K").into();
        assert!(el.template_id.is_none() || el.template_id.as_deref() != Some(""));
    }

    #[test]
    fn kbd_group_unpacks_and_sets_slot() {
        let el: Element = KbdGroup(KbdGroupProps {
            children: Some(Element::from(
                crate::primitives::r#box::vstack().children(vec![
                    kbd().text("Ctrl").into(),
                    kbd().text("K").into(),
                ]),
            )),
            ..Default::default()
        });
        assert_eq!(
            el.attrs.get(ATTR_SLOT).map(|s| s.as_str()),
            Some(SLOT_KBD_GROUP)
        );
        assert_eq!(el.children.len(), 2);
        assert!(el
            .children
            .iter()
            .all(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_KBD)));
    }

    #[test]
    fn kbd_group_builder_kbd_helper() {
        let el: Element = kbd_group().kbd("Ctrl").kbd("B").into();
        assert_eq!(el.children.len(), 2);
    }
}
