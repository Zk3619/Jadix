//! WP-OVL.1：`<ViewportModal>` RSX 与 Builder 同一 `*Opts` / content 路径。

use rui::core::reactivity::{create_root, use_signal};
use rui::core::tree::Element;
use rui::ui;
use rui::utils::viewport_modal::{
    viewport_modal, viewport_modal_content, viewport_modal_scope, ShellOverlayOpts,
    ViewportModalContentOpts,
};

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
        let open = use_signal("vm_rsx_open", || false);
        let rsx = ui! {
            <ViewportModal id="vm-rsx" openSignal={open}>
                <ViewportModalContent motion="dialog-fade">
                    { Element::new().text("rsx") }
                </ViewportModalContent>
            </ViewportModal>
        };
        let builder = viewport_modal("vm-builder")
            .open(open)
            .child(|| Element::new().text("b"));
        (rsx, builder)
    });
    let mut rsx_ids = Vec::new();
    collect_tids(&rsx, &mut rsx_ids);
    let mut b_ids = Vec::new();
    collect_tids(&builder, &mut b_ids);
    assert!(
        rsx_ids.iter().any(|t| t.ends_with("__viewport_modal")),
        "RSX scope tid missing: {rsx_ids:?}"
    );
    assert!(
        b_ids.iter().any(|t| t.ends_with("__viewport_modal")),
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
fn rsx_and_opts_content_same_view_id() {
    let (_o, (rsx, opts)) = create_root(|| {
        let open = use_signal("vm_opts_open", || false);
        let rsx = ui! {
            <ViewportModal id="vm-opts" openSignal={open} overlayOpacity={0.3}>
                <ViewportModalContent motion="slide-right">
                    { Element::new().text("sheet") }
                </ViewportModalContent>
            </ViewportModal>
        };
        let opts =
            viewport_modal_scope("vm-opts-b", Some(open), ShellOverlayOpts::default(), || {
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                frag.children(vec![viewport_modal_content(
                    ViewportModalContentOpts::default()
                        .motion(rui::utils::viewport_modal::ModalSurfaceMotion::SlideFromRight),
                    || Element::new().text("sheet"),
                )])
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
