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
//! - `ui!` multi-child packs into `jadix_fragment` / anonymous shell; both roots use `take_packed` then lay out in a row.

use crate::components::base::SlotChrome;
use crate::core::tree::{parse_color, parse_text_color, Element, IntoF32};
use crate::primitives::r#box::{hstack, BoxBuilder};
use crate::tokens;
use crate::utils::take_packed;
use crate::utils::text_node::{build_text_node, TextNodeConfig};

const ATTR_SLOT: &str = "data-slot";
const SLOT_KBD: &str = "kbd";
const SLOT_KBD_GROUP: &str = "kbd-group";
fn paint_caption(el: &mut Element, color: [u8; 4]) {
    crate::core::tree::walk_elements_mut(el, |el| {
        if el.text.is_some() {
            el.text_style.color = Some(color);
        }
    });
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

fn attach_kbd_children(
    mut el: Element,
    content: Vec<Element>,
    text_style: &TextNodeConfig,
) -> Element {
    let fg = text_style.color;
    for mut child in content {
        if let Some(text_content) = child.text.take() {
            let preserve_binding = child.template_id.is_some() || child.on_unmount.is_some();
            let is_lucide = child.text_style.font_family.as_deref() == Some("lucide");
            let size_px = if is_lucide {
                // ShadCN [&_svg:not([class*='size-'])]:size-3 → 0.75rem default
                if child.text_style.size_resolved() > 1.0 {
                    child.text_style.size_resolved()
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
                && child.text_style.size_resolved() <= 1.0
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
    chrome: SlotChrome,
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
        chrome: SlotChrome::default(),
    }
}

crate::__component_contract! {
KbdProps [];
/// RSX `<Kbd>…</Kbd>`.
#[allow(non_snake_case)]
pub fn Kbd(
identity [] id: String,
plain [] children: Option<Element>,
) -> Element {
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
}

impl KbdBuilder {
    crate::impl_slot_chrome_methods!();

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
        let chrome = builder.chrome;
        let mut el: Element = builder.container.bg(parse_color("muted")).into();
        el = el.pointer_events_none();
        el.attrs.insert(ATTR_SLOT.to_string(), SLOT_KBD.to_string());
        if !builder.id.trim().is_empty() {
            el.template_id = Some(builder.id.clone());
        }
        chrome.apply(attach_kbd_children(
            el,
            builder.content,
            &builder.text_style,
        ))
    }
}

// ---------------------------------------------------------------------------
// KbdGroup
// ---------------------------------------------------------------------------

pub struct KbdGroupBuilder {
    id: String,
    content: Vec<Element>,
    chrome: SlotChrome,
}

/// Builder entry for a horizontal cluster of [`Kbd`] keycaps.
pub fn kbd_group() -> KbdGroupBuilder {
    KbdGroupBuilder {
        id: String::new(),
        content: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

crate::__component_contract! {
KbdGroupProps [];
/// RSX `<KbdGroup>…</KbdGroup>`.
#[allow(non_snake_case)]
pub fn KbdGroup(
identity [] id: String,
plain [] children: Option<Element>,
) -> Element {
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
}

impl KbdGroupBuilder {
    crate::impl_slot_chrome_methods!();

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
        builder.chrome.apply(el)
    }
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/kbd.rs"]
mod tests;
