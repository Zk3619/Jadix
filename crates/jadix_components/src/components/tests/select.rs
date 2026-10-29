use super::*;
use crate::core::reactivity::create_root;
use crate::utils::popup::{
    mark_floating, ATTR_FLOATING, ATTR_FLOATING_MATCH_TRIGGER, ATTR_FLOATING_TRIGGER,
};

#[test]
fn select_builder_keeps_template_id() {
    let (_owner, el) = create_root(|| {
        Element::from(
            select("calc:mode")
                .default_value("a")
                .options(vec![("a".into(), "A".into())])
                .trigger_width("7rem"),
        )
    });
    assert_eq!(el.template_id.as_deref(), Some("calc:mode__select"));
}

#[test]
fn select_trigger_keeps_value_ellipsis_after_button_rebuild() {
    let (_owner, el) = create_root(|| {
        Element::from(
            select("sel_ell")
                .default_value("september")
                .options(vec![("september".into(), "September".into())])
                .trigger_width("100%"),
        )
    });
    fn find_tid<'a>(el: &'a Element, want: &str) -> Option<&'a Element> {
        if el.template_id.as_deref() == Some(want) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_tid(c, want))
    }
    let value = find_tid(&el, "sel_ell:value").expect("SelectValue leaf");
    assert!(
        matches!(
            value.text_style.overflow,
            crate::style::TextOverflow::Ellipsis
        ),
        "trigger caption must ellipsis, got {:?}",
        value.text_style.overflow
    );
    assert!(
        value.style.flex_shrink > 0.5,
        "ellipsis caption must shrink inside the trigger"
    );
}

#[test]
fn select_builder_wires_trigger_and_content_anchor() {
    let (_owner, el) = create_root(|| {
        Element::from(
            select("sel_c")
                .default_value("a")
                .options(vec![("a".into(), "A".into())])
                .trigger_width("100%")
                .max_height("12rem"),
        )
    });

    fn has_focus(el: &Element, id: &str) -> bool {
        if el.focus_id.as_deref() == Some(id) {
            return true;
        }
        el.children.iter().any(|c| has_focus(c, id))
    }
    fn has_template(el: &Element, id: &str) -> bool {
        if el.template_id.as_deref() == Some(id) {
            return true;
        }
        el.children.iter().any(|c| has_template(c, id))
    }

    assert!(
        has_focus(&el, "sel_c:trigger"),
        "SelectTrigger must use {{id}}:trigger focus id"
    );
    assert!(
        has_template(&el, "sel_c__view"),
        "SelectContent anchored overlay anchor must be present"
    );
    assert_eq!(el.template_id.as_deref(), Some("sel_c__select"));
}

#[test]
fn select_value_uses_bind_text_not_reactive_view() {
    let (_owner, el) = create_root(|| {
        let open = use_signal("sv_bt__select_open", || false);
        let value = use_signal("sv_bt__select_value", || "a".to_string());
        let labels = use_signal("sv_bt__select_labels", || {
            let mut m = HashMap::new();
            m.insert("a".into(), "Alpha".into());
            m
        });
        provide_context(
            select_context(),
            SelectCtx {
                id: "sv_bt".into(),
                open,
                trigger_fid: use_signal("sv_bt__select_trigger_fid", || "sv_bt:trigger".into()),
                value,
                placeholder: "Pick".into(),
                labels,
                match_trigger: true,
                menu_width: None,
                menu_height: None,
                min_height: None,
                max_height: None,
                placement: PopupPlacement::BottomStart,
                trigger_class: String::new(),
                content_class: String::new(),
                scroll_selected: SelectScrollSelected::None,
            },
        );
        SelectValue(SelectValueProps {
            placeholder: None,
            children: None,
            base: None,
        })
    });
    assert_eq!(el.template_id.as_deref(), Some("sv_bt:value"));
    assert_eq!(el.text.as_deref(), Some("Alpha"));
}

#[test]
fn select_builder_without_default_keeps_empty_value() {
    let (_owner, _) = create_root(|| {
        let sig = use_signal("sel_empty_val", String::new);
        let _el: Element = select("sel_empty")
            .value_signal(sig)
            .item(SelectItemData::new("a", "Alpha"))
            .item(SelectItemData::new("b", "Beta"))
            .into();
        assert!(
            sig.peek().is_empty(),
            "no default_value must leave selection empty for placeholder"
        );
    });
}

#[test]
fn select_menu_chrome_marks_floating() {
    let mut menu = Element::new().absolute();
    mark_floating(
        &mut menu,
        "sel_c",
        "sel_c:trigger",
        PopupPlacement::BottomStart,
    );
    menu.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "1");
    assert_eq!(
        menu.attrs.get(ATTR_FLOATING).map(|s| s.as_str()),
        Some("sel_c")
    );
    assert_eq!(
        menu.attrs.get(ATTR_FLOATING_TRIGGER).map(|s| s.as_str()),
        Some("sel_c:trigger")
    );
    assert_eq!(
        menu.attrs
            .get(ATTR_FLOATING_MATCH_TRIGGER)
            .map(|s| s.as_str()),
        Some("1")
    );
}

#[test]
fn scroll_offset_centers_selected_item() {
    use crate::app::scroll_support::scroll_offset_to_center_item;
    // 12 months, August (index 7), 16rem menu − pad ≈ 248px viewport.
    let sy = scroll_offset_to_center_item(7, 32.0, 248.0, 12);
    assert!(
        (sy - 116.0).abs() < 0.5,
        "August should sit near mid, got {sy}"
    );
    assert_eq!(scroll_offset_to_center_item(0, 32.0, 248.0, 12), 0.0);
    let last = scroll_offset_to_center_item(11, 32.0, 248.0, 12);
    assert!(
        (last - 136.0).abs() < 0.5,
        "last month max scroll, got {last}"
    );
}

#[test]
fn find_select_item_index_reads_attr() {
    let mut a = Element::new();
    a.set_attr("jadix_select_item", "jan");
    let mut b = Element::new();
    b.set_attr("jadix_select_item", "aug");
    let col = Element::new().child(a).child(b);
    assert_eq!(find_select_item_index(&col, "aug"), Some(1));
    assert_eq!(count_select_items(&col), 2);
}

#[test]
fn scroll_selected_parse() {
    assert_eq!(
        SelectScrollSelected::parse("center"),
        SelectScrollSelected::Center
    );
    assert_eq!(
        SelectScrollSelected::parse("start"),
        SelectScrollSelected::Start
    );
    assert_eq!(
        SelectScrollSelected::parse("none"),
        SelectScrollSelected::None
    );
    assert_eq!(SelectScrollSelected::parse(""), SelectScrollSelected::None);
}

#[test]
#[cfg(feature = "animation-base")]
fn content_keeps_panel_tid_while_exiting() {
    use crate::app::portal_overlay::{clear_overlay_stack, OverlayKind, OverlayMode};
    use crate::core::reactivity::Signal;
    use crate::utils::anchored_overlay::{
        anchored_overlay_is_mounted, anchored_overlay_phase, apply_anchored_overlay_panel_motion,
        AnchoredOverlayContentOpts,
    };
    use crate::utils::floating_motion::FloatingMotionKind;
    use crate::utils::presence::PresencePhase;

    clear_overlay_stack();
    create_root(|| {
        let open = Signal::new(true);
        let id = "sel-exit-smoke";
        let opts = AnchoredOverlayContentOpts {
            kind: OverlayKind::Menu,
            lock_page_scroll: Some(true),
            motion: FloatingMotionKind::Expand,
            mode: OverlayMode::Stacked,
            ..Default::default()
        };
        assert_eq!(
            anchored_overlay_phase(id, open, &opts),
            Some(PresencePhase::Present)
        );

        open.set(false);
        assert_eq!(
            anchored_overlay_phase(id, open, &opts),
            Some(PresencePhase::Exiting),
            "must stay mounted while exiting"
        );
        assert!(anchored_overlay_is_mounted(id, &opts));

        let mut panel = Element::new();
        apply_anchored_overlay_panel_motion(id, &mut panel, &opts, open);
        assert_eq!(
            panel.template_id.as_deref(),
            Some("sel-exit-smoke__floating_panel")
        );
    });
    clear_overlay_stack();
}
