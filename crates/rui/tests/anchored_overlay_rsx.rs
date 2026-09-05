//! WP-OVL.2：`<AnchoredOverlay>` RSX 与 Builder 同一 `*Opts` / content 路径。

use rui::core::reactivity::{create_root, use_signal};
use rui::core::tree::Element;
use rui::ui;
use rui::utils::anchored_overlay::{anchored_overlay, AnchoredOverlayContentOpts, Placement};

fn collect_tids(el: &Element, out: &mut Vec<String>) {
    if let Some(id) = &el.template_id {
        out.push(id.clone());
    }
    for c in &el.children {
        collect_tids(c, out);
    }
}

#[test]
fn rsx_and_builder_share_scope_suffix() {
    let (_o, (rsx, builder)) = create_root(|| {
        let open = use_signal("ao_rsx_open", || false);
        let rsx = ui! {
            <AnchoredOverlay id="ao-rsx" openSignal={open}>
                <AnchoredOverlayContent placement="unanchored">
                    { Element::new().text("rsx") }
                </AnchoredOverlayContent>
            </AnchoredOverlay>
        };
        let builder = anchored_overlay("ao-builder")
            .open(open)
            .placement(Placement::Unanchored)
            .child(|| Element::new().text("b"));
        (rsx, builder)
    });
    let mut rsx_ids = Vec::new();
    collect_tids(&rsx, &mut rsx_ids);
    let mut b_ids = Vec::new();
    collect_tids(&builder, &mut b_ids);
    assert!(
        rsx_ids.iter().any(|t| t.ends_with("__anchored_overlay")),
        "RSX scope tid missing: {rsx_ids:?}"
    );
    assert!(
        b_ids.iter().any(|t| t.ends_with("__anchored_overlay")),
        "Builder scope tid missing: {b_ids:?}"
    );
    assert!(
        rsx_ids.iter().any(|t| t.ends_with("__view")),
        "RSX content view missing: {rsx_ids:?}"
    );
    assert!(
        b_ids.iter().any(|t| t.ends_with("__view")),
        "Builder content view missing: {b_ids:?}"
    );
}

#[test]
fn rsx_point_and_opts_content_same_view_id() {
    let (_o, (rsx, opts)) = create_root(|| {
        let open = use_signal("ao_pt_open", || false);
        let pos = use_signal("ao_pt_pos", || [8.0_f32, 9.0]);
        let rsx = ui! {
            <AnchoredOverlay id="ao-pt" openSignal={open}>
                <AnchoredOverlayContent placement="point" pos={pos} kind="popover">
                    { Element::new().text("pt") }
                </AnchoredOverlayContent>
            </AnchoredOverlay>
        };
        let opts =
            rui::utils::anchored_overlay::anchored_overlay_scope("ao-pt-opts", Some(open), || {
                rui::utils::anchored_overlay::anchored_overlay_content(
                    AnchoredOverlayContentOpts::default()
                        .placement(Placement::Point { pos, gap: 0.0 }),
                    || Element::new().text("pt"),
                )
            });
        (rsx, opts)
    });
    fn has_view(el: &Element) -> bool {
        el.template_id
            .as_deref()
            .is_some_and(|t| t.ends_with("__view"))
            || el.children.iter().any(has_view)
    }
    assert!(has_view(&rsx));
    assert!(has_view(&opts));
}
