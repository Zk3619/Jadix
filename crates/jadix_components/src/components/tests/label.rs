use super::*;
use crate::core::reactivity::use_signal;
use crate::core::tree::Element;

#[test]
fn label_selection_sync_does_not_wipe_input_caret() {
    let mut text = Element::new().text("hello");
    text.template_id = Some("__jadix_input:field:text".into());
    text.text_style.input_caret = Some(InputCaretDraw {
        caret_byte_index: 5,
        selection: Some((0, 5)),
        selection_bg: [80, 120, 200, 90],
        overwrite_highlight: None,
        preedit_underline: None,
        extra_carets: Vec::new(),
        extra_selections: Vec::new(),
        extra_preedit_underlines: Vec::new(),
        preedit_ul: Vec::new(),
        drop_caret: false,
        caret_color: None,
        selection_foreground: None,
    });
    let mut root = Element::new().child(text);
    let mut impact = LayoutStatus::Clean;
    sync_label_selection_paint_subtree(&mut root, &mut impact);
    assert!(
        root.children[0].text_style.input_caret.is_some(),
        "Input-owned input_caret must survive label selection sync"
    );
}

#[test]
fn selectable_label_records_selection_range() {
    let key = "lbl1";
    let sk = label_selection_signal_key(key);
    use_signal(&sk, || None::<(usize, usize)>).set(Some((1, 4)));
    assert_eq!(
        use_signal(&sk, || None::<(usize, usize)>).get(),
        Some((1, 4))
    );
}

#[test]
fn non_selectable_label_does_not_participate_in_selection() {
    let el = Element::new().text("hello").selectable(false);
    assert!(selectable_text_key(&el).is_none());

    let mut el2 = Element::new().text("hello").selectable(true);
    el2.template_id = Some("t2".into());
    assert_eq!(selectable_text_key(&el2).as_deref(), Some("t2"));

    let mut el3 = Element::new().text("hello").selectable(true);
    enable_label_selection(&mut el3, "focus-me");
    assert_eq!(selectable_text_key(&el3).as_deref(), Some("focus-me"));
}

#[test]
fn copy_label_selection_updates_global_signal() {
    let mut root = Element::new();
    let mut lbl = Element::new().text("hello world").selectable(true);
    enable_label_selection(&mut lbl, "copy-lbl");
    root.children.push(lbl);

    let sk = label_selection_signal_key("copy-lbl");
    use_signal(&sk, || None::<(usize, usize)>).set(Some((0, 5)));
    assert!(try_copy_label_selection(&root));
    assert_eq!(
        use_signal(LABEL_SELECTION_COPY_SIGNAL, String::new).get(),
        "hello"
    );
}

#[test]
fn label_child_preserves_bind_text_identity() {
    use crate::app::reactive::{bind_text, reset_registered_anchors};
    use crate::core::reactivity::{create_effect, dispose_effect};
    use std::cell::RefCell;
    use std::rc::Rc;

    reset_registered_anchors();
    let mounted = Rc::new(RefCell::new(None::<Element>));
    let mounted_a = Rc::clone(&mounted);
    let root = create_effect(move || {
        let bound = bind_text("bind:label-adopt", || {
            "Selected variant: default".to_string()
        });
        assert!(bound.on_unmount.is_some());
        let hook = Rc::downgrade(bound.on_unmount.as_ref().unwrap());
        let el: Element = label("lbl-auto").child(bound).into();
        assert_eq!(el.text.as_deref(), Some("Selected variant: default"));
        assert_eq!(el.template_id.as_deref(), Some("bind:label-adopt"));
        assert!(
            el.on_unmount.is_some(),
            "adopting bind_text must keep unmount hook alive"
        );
        assert!(
            hook.upgrade().is_some(),
            "bind hook must not fire during Label adoption"
        );
        *mounted_a.borrow_mut() = Some(el);
    });
    assert!(mounted.borrow().is_some());
    drop(mounted.borrow_mut().take());
    dispose_effect(root);
    reset_registered_anchors();
}

#[test]
fn label_for_target_sets_accesskit_id() {
    let el: Element = label("").text("Email").for_target("input:email").into();
    assert_eq!(
        el.attrs.get(ATTR_LABEL_FOR).map(|s| s.as_str()),
        Some("input:email")
    );
    assert_eq!(
        el.attrs.get("accesskit_id").map(|s| s.as_str()),
        Some("lbl:for:input:email")
    );
}

#[test]
fn linked_label_explicit_width_and_container_keep_their_click_box() {
    for mut el in [
        Element::from(label("").text("Title").w(240.0).for_target("terms")),
        Element::from(
            label("")
                .child(
                    Element::new()
                        .flex_col()
                        .child(Element::new().text("Title"))
                        .child(Element::new().text("Description")),
                )
                .for_target("terms"),
        ),
    ] {
        let mut root = Element::new()
            .flex_col()
            .w(240.0)
            .h(100.0)
            .child(std::mem::take(&mut el));
        crate::layout::layout_tree::LayoutTree::new().build_and_layout(
            &mut root,
            240.0,
            100.0,
            [-1.0, -1.0],
        );
        assert_eq!(root.children[0].layout_bounds[2], 240.0);
    }
}

#[test]
fn label_r_for_alias_sets_accesskit_id() {
    let el: Element = label("").text("Email").r#for("input:email").into();
    assert_eq!(
        el.attrs.get(ATTR_LABEL_FOR).map(|s| s.as_str()),
        Some("input:email")
    );
}

#[test]
fn label_empty_id_does_not_write_empty_template_id() {
    let el: Element = label("").text("x").into();
    assert_ne!(el.template_id.as_deref(), Some(""));
    assert!(
        el.template_id.is_none(),
        "display Label empty id → no template_id"
    );
}
