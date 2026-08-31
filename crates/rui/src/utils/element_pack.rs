//! `rui_fragment` / anonymous pack unpack (WP-SI.8).
//!
//! `ui!` product components pack multiple children into a single-root
//! [`rui_fragment`](take_fragment) (historically a VStack). Row/list builders must
//! unpack before laying out — call [`take_packed`] (or [`take_fragment`] when only
//! the fragment marker is expected).
//!
//! ## Contract ([`take_packed`])
//! - **Expand**: `rui_fragment=1`, or an anonymous multi-child shell (`children.len() > 1`,
//!   no text).
//! - **Preserve**: any `data-slot`, `on_click`, `focus_id`, or `button_hover_paint`
//!   (Kbd / menu Item / Button chrome, …).
//! - **Recursive**: nested pack shells are flattened; preserved nodes are leaves of the
//!   walk (their children are not hoisted).

use crate::core::tree::Element;

const ATTR_FRAGMENT: &str = "rui_fragment";
const ATTR_SLOT: &str = "data-slot";

/// Expand only `rui_fragment=1` (do not peel anonymous VStack / Box wrappers).
pub fn take_fragment(mut el: Element) -> Vec<Element> {
    if is_fragment(&el) {
        std::mem::take(&mut el.children)
    } else {
        vec![el]
    }
}

/// Expand fragment / anonymous multi-child packs without dismantling slotted or
/// interactive shells. Prefer this for row containers (Button, Badge, ButtonGroup, …).
pub fn take_packed(el: Element) -> Vec<Element> {
    take_packed_inner(el)
}

fn is_fragment(el: &Element) -> bool {
    el.attrs.get(ATTR_FRAGMENT).map(|s| s.as_str()) == Some("1")
}

fn has_slot(el: &Element) -> bool {
    el.attrs.get(ATTR_SLOT).is_some_and(|s| !s.is_empty())
}

/// Nodes that must stay intact (product chrome / identity).
fn is_preserved_shell(el: &Element) -> bool {
    has_slot(el)
        || el.on_click.is_some()
        || el.focus_id.is_some()
        || el.button_hover_paint.is_some()
}

fn is_anonymous_multi_pack(el: &Element) -> bool {
    !is_preserved_shell(el)
        && el.text.as_ref().is_none_or(|t| t.is_empty())
        && el.children.len() > 1
}

fn is_pack_shell(el: &Element) -> bool {
    if is_preserved_shell(el) {
        return false;
    }
    is_fragment(el) || is_anonymous_multi_pack(el)
}

fn take_packed_inner(mut el: Element) -> Vec<Element> {
    if is_pack_shell(&el) {
        std::mem::take(&mut el.children)
            .into_iter()
            .flat_map(take_packed_inner)
            .collect()
    } else {
        vec![el]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::r#box::vstack;

    #[test]
    fn fragment_unpacks_children() {
        let mut frag = Element::new();
        frag.set_attr(ATTR_FRAGMENT, "1");
        frag = frag.children(vec![Element::new().text("a"), Element::new().text("b")]);
        let kids = take_packed(frag);
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].text.as_deref(), Some("a"));
        assert_eq!(kids[1].text.as_deref(), Some("b"));
    }

    #[test]
    fn take_fragment_ignores_anonymous_vstack() {
        let pack = Element::from(vstack().children(vec![
            Element::new().text("a"),
            Element::new().text("b"),
        ]));
        let kids = take_fragment(pack);
        assert_eq!(kids.len(), 1, "take_fragment must not peel VStack");
    }

    #[test]
    fn anonymous_vstack_unpacks() {
        let pack = Element::from(vstack().children(vec![
            Element::new().text("Search"),
            Element::new().text("⌘"),
        ]));
        let kids = take_packed(pack);
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].text.as_deref(), Some("Search"));
    }

    #[test]
    fn plain_leaf_stays_singleton() {
        let el = Element::new().text("x");
        let kids = take_packed(el);
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].text.as_deref(), Some("x"));
    }

    #[test]
    fn data_slot_shell_is_not_dismantled() {
        let mut kbd = Element::from(vstack().children(vec![
            Element::new().text("⌘"),
            Element::new().text("K"),
        ]));
        kbd.set_attr(ATTR_SLOT, "kbd-group");
        let kids = take_packed(kbd);
        assert_eq!(kids.len(), 1);
        assert_eq!(
            kids[0].attrs.get(ATTR_SLOT).map(|s| s.as_str()),
            Some("kbd-group")
        );
        assert_eq!(kids[0].children.len(), 2);
    }

    #[test]
    fn nested_fragment_flattens_but_keeps_slot() {
        let mut kbd = Element::new().text("⌘");
        kbd.set_attr(ATTR_SLOT, "kbd");

        let mut inner = Element::new();
        inner.set_attr(ATTR_FRAGMENT, "1");
        inner = inner.children(vec![Element::new().text("Search"), kbd]);

        let mut outer = Element::new();
        outer.set_attr(ATTR_FRAGMENT, "1");
        outer = outer.children(vec![inner]);

        let kids = take_packed(outer);
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].text.as_deref(), Some("Search"));
        assert_eq!(
            kids[1].attrs.get(ATTR_SLOT).map(|s| s.as_str()),
            Some("kbd")
        );
    }

    #[test]
    fn interactive_shell_is_preserved() {
        let mut btn = Element::from(vstack().children(vec![
            Element::new().text("A"),
            Element::new().text("B"),
        ]));
        btn = btn.on_click(|| {});
        let kids = take_packed(btn);
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].children.len(), 2);
    }
}
