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
//!   (Kbd / menu Item / Button chrome, …); and any anonymous shell that carries
//!   box-model layout (padding / margin / gap / border) so Taffy still sees insets.
//! - **Recursive**: nested pack shells are flattened; preserved nodes are leaves of the
//!   walk (their children are not hoisted).
//! - **Vacuous children**: [`children_none_if_vacuous`] treats empty fragments / blank
//!   leaves as `None` so slot components (`AlertDialogCancel`, `SheetClose`, …) can apply
//!   their default chrome for `<Foo />` and `<Foo></Foo>`.

use crate::core::tree::Element;
use crate::layout::{lp_length_px, lpa_kind, DimKind};
use crate::style::Style;

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

/// Anonymous multi-child packs must not be peeled when they carry box-model layout
/// (padding / margin / gap / border). Otherwise `take_packed` drops the wrapper and
/// Taffy never sees those insets — BoxBuilder `p/px/py` on the shell is lost.
fn carries_box_model_shell(el: &Element) -> bool {
    carries_box_model_style(&el.style)
}

fn carries_box_model_style(style: &Style) -> bool {
    fn nonzero_lp(lp: taffy::style::LengthPercentage) -> bool {
        lp_length_px(lp).map(|v| v.abs() > 1e-4).unwrap_or(false)
    }
    fn nonzero_lpa(lpa: taffy::style::LengthPercentageAuto) -> bool {
        match lpa_kind(lpa) {
            DimKind::Auto => false,
            DimKind::Length(l) => l.abs() > 1e-4,
            DimKind::Percent(p) => p.abs() > 1e-4,
        }
    }
    if nonzero_lp(style.padding.left)
        || nonzero_lp(style.padding.right)
        || nonzero_lp(style.padding.top)
        || nonzero_lp(style.padding.bottom)
    {
        return true;
    }
    if nonzero_lpa(style.margin.left)
        || nonzero_lpa(style.margin.right)
        || nonzero_lpa(style.margin.top)
        || nonzero_lpa(style.margin.bottom)
    {
        return true;
    }
    if nonzero_lp(style.gap.width) || nonzero_lp(style.gap.height) {
        return true;
    }
    style.resolved_border_widths().iter().any(|w| *w > 1e-4)
}

fn is_anonymous_multi_pack(el: &Element) -> bool {
    !is_preserved_shell(el)
        && el.text.as_ref().is_none_or(|t| t.is_empty())
        && el.children.len() > 1
}

fn is_pack_shell(el: &Element) -> bool {
    if is_preserved_shell(el) || carries_box_model_shell(el) {
        return false;
    }
    is_fragment(el) || is_anonymous_multi_pack(el)
}

/// Plain-text RSX child (`"Cancel"`) — whitespace-only strings are treated as absent.
pub fn is_text_content_leaf(el: &Element) -> bool {
    el.children.is_empty() && el.text.as_ref().is_some_and(|t| !t.trim().is_empty())
}

/// `true` when the subtree carries no renderable or interactive content.
pub fn is_vacuous_element(el: &Element) -> bool {
    if is_preserved_shell(el) || el.focusable {
        return false;
    }
    // Reconcile identity: `reactive_view` placeholders are empty shells whose only job is
    // to keep `template_id` in the tree so `try_reconcile_subtree` can graft. `ui!`
    // runs `children_none_if_vacuous` on component children; dropping those placeholders
    // blanks nested islands (Gallery MessageScroller rows-only split).
    if el.template_id.as_deref().is_some_and(|id| !id.is_empty()) {
        return false;
    }
    if is_text_content_leaf(el) {
        return false;
    }
    if el.children.is_empty() {
        return true;
    }
    if is_pack_shell(el) {
        return el.children.iter().all(is_vacuous_element);
    }
    false
}

/// Normalize RSX `children` before slot components: empty / fragment-only → `None`.
pub fn children_none_if_vacuous(children: Option<Element>) -> Option<Element> {
    match children {
        None => None,
        Some(el) if is_vacuous_element(&el) => None,
        Some(el) => Some(el),
    }
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
        let pack = Element::from(
            vstack().children(vec![Element::new().text("a"), Element::new().text("b")]),
        );
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
        let mut kbd = Element::from(
            vstack().children(vec![Element::new().text("⌘"), Element::new().text("K")]),
        );
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
    fn padded_anonymous_vstack_is_not_dismantled() {
        let pack = Element::from(
            vstack()
                .w("100%")
                .p("0.25rem")
                .children(vec![Element::new().text("a"), Element::new().text("b")]),
        );
        let kids = take_packed(pack);
        assert_eq!(kids.len(), 1, "padded shell must stay intact");
        assert!(
            lp_length_px(kids[0].style.padding.left).unwrap_or(0.0) > 0.0,
            "padding must survive take_packed"
        );
    }

    #[test]
    fn interactive_shell_is_preserved() {
        let mut btn = Element::from(
            vstack().children(vec![Element::new().text("A"), Element::new().text("B")]),
        );
        btn = btn.on_click(|| {});
        let kids = take_packed(btn);
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].children.len(), 2);
    }

    #[test]
    fn vacuous_leaf_and_fragment_normalize_to_none() {
        assert!(is_vacuous_element(&Element::new()));
        let mut frag = Element::new();
        frag.set_attr(ATTR_FRAGMENT, "1");
        assert!(is_vacuous_element(&frag));
        assert!(children_none_if_vacuous(None).is_none());
        assert!(children_none_if_vacuous(Some(Element::new())).is_none());
        assert!(children_none_if_vacuous(Some(frag)).is_none());
    }

    #[test]
    fn vacuous_whitespace_text_is_absent() {
        assert!(is_text_content_leaf(&Element::new().text("Cancel")));
        assert!(!is_text_content_leaf(&Element::new().text("   ")));
        assert!(is_vacuous_element(&Element::new().text("   ")));
    }

    #[test]
    fn non_vacuous_children_survive_normalization() {
        let el = Element::new().text("OK");
        let out = children_none_if_vacuous(Some(el));
        assert_eq!(out.unwrap().text.as_deref(), Some("OK"));
    }

    #[test]
    fn reactive_placeholder_is_not_vacuous() {
        let ph = Element::new()
            .template_id("examples/gallery/MessageScrollerShowcase:rows")
            .template_hash("__rui_reactive_placeholder_v1")
            .w("100%")
            .h("max-content");
        assert!(
            !is_vacuous_element(&ph),
            "empty reactive placeholder must survive ui! children_none_if_vacuous"
        );
        let kept = children_none_if_vacuous(Some(ph)).expect("placeholder dropped");
        assert_eq!(
            kept.template_id.as_deref(),
            Some("examples/gallery/MessageScrollerShowcase:rows")
        );
    }
}
