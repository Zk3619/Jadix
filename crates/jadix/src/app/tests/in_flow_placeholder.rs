use super::*;
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::vstack;
use taffy::style::Display;

fn neighbor_gap_in_column(middle: Element) -> f32 {
    fn find_tid<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
        if el.template_id.as_deref() == Some(id) {
            Some(el)
        } else {
            el.children.iter().find_map(|c| find_tid(c, id))
        }
    }

    let mut a = Element::new().w(20.0).h(20.0);
    a.template_id = Some("gap-a".into());
    let mut b = Element::new().w(20.0).h(20.0);
    b.template_id = Some("gap-b".into());
    let mut col: Element = vstack().gap(8.0).child(a).child(middle).child(b).into();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut col, 80.0, 200.0, [0.0, 0.0]);
    let aa = find_tid(&col, "gap-a")
        .unwrap_or_else(|| panic!("missing gap-a"))
        .layout_bounds;
    let bb = find_tid(&col, "gap-b")
        .unwrap_or_else(|| panic!("missing gap-b"))
        .layout_bounds;
    bb[1] - (aa[1] + aa[3])
}

#[test]
fn display_none_placeholder_does_not_occupy_flex_gap() {
    let in_flow = neighbor_gap_in_column(Element::new().w(0.0).h(0.0));
    let hidden = neighbor_gap_in_column(reactive_placeholder_in_flow("hide-gap", false));
    let visible = neighbor_gap_in_column(reactive_placeholder_in_flow("show-gap", true));
    assert!(
        (in_flow - 16.0).abs() < 1.0,
        "static 0×0 still takes a gap slot (expected ~16, got {in_flow})"
    );
    assert!(
        (hidden - 8.0).abs() < 1.0,
        "display:none must not occupy gap (expected ~8, got {hidden})"
    );
    assert!(
        (visible - 16.0).abs() < 1.0,
        "visible 0×0 still takes a gap slot (expected ~16, got {visible})"
    );
    assert_eq!(
        reactive_placeholder_in_flow("closed", false).style.display,
        Display::None
    );
    assert_ne!(
        reactive_placeholder_in_flow("open", true).style.display,
        Display::None
    );
}
