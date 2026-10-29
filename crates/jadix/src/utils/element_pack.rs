//! `jadix_fragment` / anonymous pack unpack (WP-SI.8).
//!
//! `ui!` product components pack multiple children into a single-root
//! [`jadix_fragment`](take_fragment) (historically a VStack). Row/list builders must
//! unpack before laying out — call [`take_packed`] (or [`take_fragment`] when only
//! the fragment marker is expected).
//!
//! ## Contract ([`take_packed`])
//! - **Expand**: `jadix_fragment=1`, or an anonymous multi-child shell (`children.len() > 1`,
//!   no text).
//! - **Preserve**: any `data-slot`, `on_click`, `focus_id`, or authored paint policy
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

const ATTR_FRAGMENT: &str = "jadix_fragment";
const ATTR_SLOT: &str = "data-slot";

/// Expand only `jadix_fragment=1` (do not peel anonymous VStack / Box wrappers).
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
        || el.on_wheel.is_some()
        || el.focus_id.is_some()
        || !el.authoring_state.is_empty()
        || el.paint_edge.is_some()
        || el.paint_opaque_fill
        || el.paint_region_replay
        || el.paint_children_unculled
        || el.attrs.contains_key(crate::core::tree::ATTR_OPACITY_GROUP)
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
#[path = "tests/element_pack.rs"]
mod tests;
