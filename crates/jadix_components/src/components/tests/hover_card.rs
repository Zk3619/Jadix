use super::*;
use crate::components::popover::popover_side_align_to_placement;
use crate::core::reactivity::create_root;
use crate::style::with_class_list;
use crate::utils::popup::PopupPlacement;

#[test]
fn side_align_defaults_via_popover_helper() {
    assert_eq!(
        popover_side_align_to_placement("bottom", "center"),
        PopupPlacement::BottomCenter
    );
}

#[test]
fn content_anchor_is_zero_sized() {
    let (_owner, el) = create_root(|| {
        hover_card_scope("hc-anchor", None, || {
            hover_card_content(HoverCardContentOpts::default(), || {
                Element::new().text("hi")
            })
        })
    });
    fn find_zero(el: &Element) -> bool {
        let is_view = el.template_id.as_deref() == Some("hc-anchor__view");
        let dim0 = crate::layout::dim_length_px(el.style.size.width).is_some_and(|w| w == 0.0)
            && crate::layout::dim_length_px(el.style.size.height).is_some_and(|h| h == 0.0);
        (is_view && dim0) || el.children.iter().any(find_zero)
    }
    assert!(
        find_zero(&el),
        "HoverCardContent must use a 0×0 portal anchor"
    );
}

#[test]
fn scope_exposes_template() {
    let (_owner, el) = create_root(|| {
        hover_card("hc-scope")
            .open_signal(use_signal("hc-scope-open", || false))
            .child(|| Element::new().text("x"))
    });
    assert_eq!(el.template_id.as_deref(), Some("hc-scope__hover_card"));
}

#[test]
fn surface_default_has_no_fixed_width() {
    let el = hover_card_surface(Element::new().text("hi"), None);
    assert!(
        crate::layout::dim_length_px(el.style.size.width).is_none(),
        "base HoverCardContent must not bake w-64; width comes from class/opts"
    );
}

#[test]
fn class_width_seeds_floating_w_attr() {
    let mut panel = with_class_list(
        hover_card_surface(Element::new().text("body"), None),
        "w-64",
    );
    seed_hover_card_floating_w(&mut panel, None);
    let w = panel
        .attrs
        .get(ATTR_FLOATING_W)
        .expect("class w-64 must seed ATTR_FLOATING_W");
    let px = w.trim_end_matches("px").parse::<f32>().unwrap_or(0.0);
    assert!(
        (px - 256.0).abs() < 0.5,
        "w-64 → 256px floating width, got {w}"
    );
}

#[test]
fn zero_delay_open_closes_via_intent() {
    use crate::utils::hover_intent::{arm_close, arm_open, HoverIntent};
    let (_owner, _) = create_root(|| {
        let open = use_signal("hc-zd-open", || false);
        let th = use_signal("hc-zd-th", || false);
        let ch = use_signal("hc-zd-ch", || false);
        let od = use_signal("hc-zd-od", || 0_u64);
        let cd = use_signal("hc-zd-cd", || 0_u64);
        let intent = HoverIntent {
            open,
            trigger_hovered: th,
            content_hovered: ch,
            open_delay_ms: od,
            close_delay_ms: cd,
        };
        arm_open("hc-zd", intent);
        assert!(!open.peek());
        th.set(true);
        arm_open("hc-zd", intent);
        assert!(open.peek());
        th.set(false);
        arm_close("hc-zd", intent);
        assert!(!open.peek());
        Element::new()
    });
}
