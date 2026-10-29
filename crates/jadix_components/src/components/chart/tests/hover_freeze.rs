use super::*;
use crate::core::reactivity::{create_root, use_signal};

#[test]
fn tooltip_regression_decimal_stays_on_one_line() {
    let item = ChartTooltipItem {
        label: "Tuesday".into(),
        value: 0.52,
        series_key: None,
        header: None,
    };
    let body = chart_tooltip_content(&item, [0.0, 0.0, 1.0, 1.0], TooltipIndicator::Dot);
    let mut chart = chart_tooltip_bubble(body, &SlotChrome::default())
        .w(96.0)
        .max_w(96.0);
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let result = tree.build_and_layout(&mut chart, 1360.0, 200.0, [-1.0, -1.0]);
    for text in ["0.52"] {
        let value = result.text_nodes.iter().find(|t| t.text == text).unwrap();
        assert_eq!(
            value.style.white_space,
            crate::style::WhiteSpace::Nowrap,
            "tooltip {text} must stay on one visual row"
        );
        let prepared = value.prepared.as_ref().expect("measured tooltip text");
        let metrics = crate::text::layout(prepared, value.width);
        let intrinsic = crate::text::layout(prepared, f32::MAX).width;
        assert!(
            value.width + 0.01 >= intrinsic,
            "tooltip {text} is squeezed: {} < {intrinsic}",
            value.width
        );
        assert_eq!(
            metrics.line_count, 1,
            "tooltip {text} wraps at width {}",
            value.width
        );
    }
}

#[test]
fn tooltip_regression_pointer_moves_do_not_rebuild() {
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, take_flushed_effects, EffectPhase};

    reset_registered_anchors();
    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(live.clone());
    let (owner, hov) = create_root({
        let live = live.clone();
        move || {
            let hov = use_signal("tooltip-regression-move", || ChartHover {
                x: 30.0,
                y: 20.0,
                index: Some(0),
            });
            create_effect(move || {
                let mut builder = define_chart()
                    .id("tooltip-regression")
                    .mark(super::super::mark::bar_y([("Tue", 0.52), ("Wed", 0.8)]))
                    .tooltip(true);
                builder.hover_signal = Some(hov);
                *live.borrow_mut() = builder.into();
            });
            hov
        }
    });
    drain_deferred_view_remounts();
    let _ = take_flushed_effects();
    assign_chart_hover(
        hov,
        ChartHover {
            x: 50.0,
            y: 30.0,
            index: Some(0),
        },
    );
    let rebuilds = take_flushed_effects()
        .into_iter()
        .filter(|(_, phase)| *phase == EffectPhase::Render)
        .count();
    assign_chart_hover(
        hov,
        ChartHover {
            x: 70.0,
            y: 30.0,
            index: Some(1),
        },
    );
    let datum_rebuilds = take_flushed_effects()
        .into_iter()
        .filter(|(_, phase)| *phase == EffectPhase::Render)
        .count();
    let mut updated_value = false;
    crate::core::tree::walk_elements(&live.borrow(), |el| {
        updated_value |= el.text.as_deref() == Some("0.80");
    });
    assign_chart_hover(hov, ChartHover::default());
    let mut tooltip_visible = false;
    crate::core::tree::walk_elements(&live.borrow(), |el| {
        tooltip_visible |= el
            .get_attr("data-slot")
            .is_some_and(|s| s == "chart-tooltip");
    });
    clear_test_reconcile_root();
    owner.dispose();
    reset_registered_anchors();
    assert_eq!(
        rebuilds, 0,
        "moving within a datum must only update overlay position"
    );
    assert!(
        datum_rebuilds > 0 && updated_value,
        "changing datum must refresh the content"
    );
    assert!(
        !tooltip_visible,
        "leaving must remove an unselected tooltip"
    );
}

#[test]
fn tooltip_regression_pointer_sync_follows_and_flips_without_layout() {
    create_root(|| {
        let scene = Rc::new(RefCell::new(None));
        let mut builder = define_chart()
            .mark(super::super::mark::bar_y([("Tue", 0.52)]))
            .width_px(200.0)
            .plot_h(80.0)
            .tooltip(true)
            .crosshair(true);
        builder.tooltip_scene = Some(scene.clone());
        let _ = compile_cartesian(builder);
        let scene = scene.borrow();
        let sc = scene.as_ref().unwrap();
        let sig = use_signal("tooltip-sync-regression", || ChartHover {
            x: 30.0,
            y: 20.0,
            index: Some(0),
        });
        let layer =
            attach_overlay_pointer_sync(overlay_from_pointer(sig.peek(), None, sc), sig, None, sc);
        let mut root: Element = r#box().w(200.0).h(80.0).relative(true).child(layer).into();
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        tree.build_and_layout(&mut root, 200.0, 80.0, [-1.0, -1.0]);
        let original = root.children[0].children[2].layout_bounds;
        for (x, y, flip) in [
            (50.0, 25.0, false),
            (190.0, 75.0, true),
            (40.0, 15.0, false),
        ] {
            assign_chart_hover(
                sig,
                ChartHover {
                    x,
                    y,
                    index: Some(0),
                },
            );
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                200.0,
                80.0,
                [0.0, 0.0],
            );
            assert_eq!(impact, LayoutStatus::StyleChanged);
            let layer = &root.children[0];
            let tip = &layer.children[2];
            assert_eq!(
                tip.layout_bounds, original,
                "movement must retain the layout box"
            );
            let expected_x = x + if flip {
                -original[2] - TOOLTIP_GAP_PX
            } else {
                TOOLTIP_GAP_PX
            };
            let expected_y = y + if flip {
                -original[3] - TOOLTIP_GAP_PX
            } else {
                TOOLTIP_GAP_PX
            };
            assert!((tip.layout_bounds[0] + tip.style.translate[0] - expected_x).abs() < 0.01);
            assert!((tip.layout_bounds[1] + tip.style.translate[1] - expected_y).abs() < 0.01);
            assert!(
                (layer.children[0].layout_bounds[0] + layer.children[0].style.translate[0] - x)
                    .abs()
                    < 0.01
            );
            assert!(
                (layer.children[1].layout_bounds[1] + layer.children[1].style.translate[1] - y)
                    .abs()
                    < 0.01
            );
        }
    });
}

#[test]
fn polar_marks_keep_fill_stroke_and_dot_z_order() {
    let rows = || {
        vec![
            super::super::datum::ChartDatum::new("N", 4.0),
            super::super::datum::ChartDatum::new("E", 2.0),
        ]
    };
    let marks = vec![
        ChartMark::from(super::super::polar::radial_dot(rows())),
        ChartMark::from(super::super::polar::radial_line(rows())),
        ChartMark::from(super::super::polar::radial_bar(rows())),
    ];
    let kinds: Vec<_> = polar_mark_order(&marks)
        .into_iter()
        .map(|i| marks[i].kind())
        .collect();
    assert_eq!(kinds, vec!["radialBar", "radialLine", "radialDot"]);
}

#[test]
fn frozen_hover_does_not_write_until_unfreeze() {
    create_root(|| {
        set_chart_hover_frozen(false);
        let sig = use_signal("t-chart-hov-freeze", ChartHover::default);
        set_chart_hover_frozen(true);
        assign_chart_hover(
            sig,
            ChartHover {
                x: 12.0,
                y: 8.0,
                index: Some(1),
            },
        );
        assert_eq!(
            sig.peek(),
            ChartHover::default(),
            "scroll-frozen hover must not remount the tooltip overlay"
        );
        set_chart_hover_frozen(false);
        assert_eq!(
            sig.peek().index,
            Some(1),
            "pending hover applies after scroll settles"
        );
    });
}

#[test]
fn frozen_leave_clears_after_unfreeze() {
    create_root(|| {
        set_chart_hover_frozen(false);
        let sig = use_signal("t-chart-hov-leave", || ChartHover {
            x: 4.0,
            y: 4.0,
            index: Some(0),
        });
        set_chart_hover_frozen(true);
        assign_chart_hover(sig, ChartHover::default());
        assert_eq!(sig.peek().index, Some(0));
        assert!(
            overlay_scroll_hidden(sig.id()),
            "leave during scroll must hide the live overlay immediately"
        );
        set_chart_hover_frozen(false);
        assert!(!overlay_scroll_hidden(sig.id()));
        assert_eq!(sig.peek(), ChartHover::default());
    });
}

#[test]
fn frozen_leave_hides_overlay_via_opacity_not_structure() {
    use crate::core::tree::{sync_reactive_elements_subtree, LayoutTree};
    create_root(|| {
        set_chart_hover_frozen(false);
        let sig = use_signal("t-chart-hov-hide-style", || ChartHover {
            x: 4.0,
            y: 4.0,
            index: Some(0),
        });
        let mut layer = attach_overlay_scroll_hide(r#box().w(40.0).h(40.0).into(), sig);
        layer.layout_bounds = [0.0, 0.0, 40.0, 40.0];
        set_chart_hover_frozen(true);
        assign_chart_hover(sig, ChartHover::default());
        let mut tree = LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        sync_reactive_elements_subtree(
            &mut layer,
            &mut tree,
            &mut impact,
            200.0,
            200.0,
            [0.0, 0.0],
        );
        assert!(
            (layer.style.opacity).abs() < 0.001,
            "leave during scroll must hide tooltip without remount"
        );
        assert_eq!(impact, LayoutStatus::StyleChanged);
        assert_eq!(sig.peek().index, Some(0));
        set_chart_hover_frozen(false);
    });
}

#[test]
fn hover_enter_samples_datum_without_mousemove() {
    use super::super::mark::bar_y;
    use super::super::scale::{resolve_scales, AxisSpec};
    create_root(|| {
        set_chart_hover_frozen(false);
        let sig = use_signal("t-chart-hov-enter", ChartHover::default);
        let mark = ChartMark::from(bar_y([0.2f32, 0.9]));
        let scales = resolve_scales(
            200.0,
            80.0,
            &AxisSpec::band(),
            &AxisSpec::linear(),
            vec!["Mon".into(), "Tue".into()],
            &[],
            &[],
            &[0.2, 0.9],
            true,
        );
        let theme = ChartTheme::tokens();
        crate::app::pointer_events::note_pointer_client(150.0, 40.0);
        crate::app::pointer_events::note_hover_target_bounds([0.0, 0.0, 200.0, 80.0]);
        apply_chart_hover_from_client(
            &[mark],
            &scales,
            &theme,
            None,
            80.0,
            0.0,
            0.0,
            0.0,
            200.0,
            80.0,
            true,
            sig,
            150.0,
            40.0,
            [0.0, 0.0, 200.0, 80.0],
        );
        assert!(
            sig.peek().index.is_some(),
            "scroll-induced hover (no mousemove) must still pin a datum"
        );
    });
}
