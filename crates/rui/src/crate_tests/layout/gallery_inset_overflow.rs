//! Gallery inset shell: flex children must not spill past SidebarInset border-box.

use crate::components::scroll_area::scroll_area;
use crate::components::sidebar::{
    sidebar, sidebar_provider_scope, SidebarInset, SidebarInsetProps,
};
use crate::core::reactivity::{create_effect, create_root};
use crate::core::tree::{Element, LayoutTree};
use crate::primitives::r#box::{hstack, vstack};

fn card_like() -> Element {
    let mut row = hstack().w("100%").min_w(0.0).gap(8.0).wrap("wrap");
    for _ in 0..9 {
        row = row.child(Element::new().min_w(80.0).h(32.0).bg([0.1, 0.1, 0.1, 1.0]));
    }
    vstack()
        .w("100%")
        .min_w(0.0)
        .p(16.0)
        .border([0.8, 0.8, 0.8, 1.0], 1.0)
        .child(Element::new().text("1) Button Variants + State"))
        .child(row)
        .into()
}

fn max_right_overflow(inset: &Element) -> f32 {
    let ib = inset.layout_bounds;
    let inset_r = ib[0] + ib[2];
    let mut worst = 0.0_f32;
    fn walk(el: &Element, inset_r: f32, worst: &mut f32) {
        let b = el.layout_bounds;
        *worst = worst.max((b[0] + b[2]) - inset_r);
        for c in &el.children {
            walk(c, inset_r, worst);
        }
    }
    walk(inset, inset_r, &mut worst);
    worst
}

fn gallery_inset_shell(scroll: Element) -> Element {
    sidebar_provider_scope("gal", None, true, Some("inset"), None, move || {
        let trigger = Element::from(
            hstack()
                .w("100%")
                .p("0.5rem")
                .flex_shrink(0.0)
                .child(Element::new().w(28.0).h(28.0).bg([0.9, 0.9, 0.9, 1.0])),
        );
        let body = Element::from(vstack().min_w(0.0).min_h(0.0).flex_grow(1.0).child(scroll));
        let inset_children = {
            let mut pack = Element::new();
            pack.set_attr("rui_fragment", "1");
            pack.children(vec![trigger, body])
        };
        let mut frag = Element::new();
        frag.set_attr("rui_fragment", "1");
        frag.children(vec![
            sidebar("gal-sidebar").variant("inset").into(),
            SidebarInset(SidebarInsetProps {
                variant: Some("inset".into()),
                class: None,
                children: Some(inset_children),
                base: None,
            }),
        ])
    })
}

fn find_inset<'a>(root: &'a Element) -> &'a Element {
    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_slot(c, slot))
    }
    find_slot(root, "sidebar-inset").expect("inset")
}

#[test]
fn inset_scroll_children_do_not_exceed_inset_width() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let scroll_body = vstack()
                .w("100%")
                .p("1.5rem")
                .child(Element::new().text("基础"))
                .child(card_like());
            let scroll = Element::from(
                scroll_area("test-scroll")
                    .flex_grow(1.0)
                    .min_h(0.0)
                    .overflow_y("auto")
                    .overflow_x("auto")
                    .child(scroll_body),
            );
            let mut root = gallery_inset_shell(scroll);
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 400.0, 600.0, [-1.0, -1.0]);
            let inset = find_inset(&root);
            let overflow = max_right_overflow(inset);
            assert!(
                overflow < 1.0,
                "descendants must not exceed SidebarInset width; overflow={overflow}px bounds={:?}",
                inset.layout_bounds
            );
        });
    });
}

#[test]
fn inset_fills_provider_height_without_descendant_spill() {
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let scroll_body = vstack()
                .w("100%")
                .p("1.5rem")
                .child(Element::new().text("基础"))
                .child(card_like());
            let scroll = Element::from(
                scroll_area("test-scroll")
                    .flex_grow(1.0)
                    .min_h(0.0)
                    .overflow_y("auto")
                    .overflow_x("auto")
                    .child(scroll_body),
            );
            let mut root = gallery_inset_shell(scroll);
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 400.0, 600.0, [-1.0, -1.0]);
            let inset = find_inset(&root);
            let ib = inset.layout_bounds;
            fn find_tid<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
                if el.template_id.as_deref() == Some(id) {
                    return Some(el);
                }
                el.children.iter().find_map(|c| find_tid(c, id))
            }
            let scroll_shell = find_tid(&root, "test-scroll").expect("scroll shell");
            let sb = scroll_shell.layout_bounds;
            let inset_b = ib[1] + ib[3];
            let shell_b = sb[1] + sb[3];
            assert!(
                shell_b <= inset_b + 1.0,
                "ScrollArea chrome must fit inside SidebarInset; shell={sb:?} inset={ib:?}"
            );
            assert!(
                sb[0] >= ib[0] - 1.0 && sb[0] + sb[2] <= ib[0] + ib[2] + 1.0,
                "ScrollArea chrome must not spill horizontally; shell={sb:?} inset={ib:?}"
            );
        });
    });
}
