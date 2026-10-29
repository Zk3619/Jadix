use super::*;
use crate::core::reactivity::{create_root, use_signal};
use crate::core::scroll_viewport::{activate_scroll_viewport, ScrollViewportSignalKeys};

#[test]
fn secondary_scroll_chases_targets_and_honors_explicit_jump_in_its_scope() {
    create_root(|| {
        let primary_keys = ScrollViewportSignalKeys::new("primary-scroll-id");
        let primary = use_signal(&primary_keys.offset_y, || 17.0_f32);
        let keys = ScrollViewportSignalKeys::new("secondary-scroll-id");
        let _scope =
            crate::app::window_scope::enter(crate::app::window_scope::WindowScope::Secondary(9018));
        let (_owner, ()) = create_root(|| {
            crate::core::reactivity::bind_current_owner_window_key_scope(9018);
            let metrics = vec![(
                "secondary-scroll-id".into(),
                [0.0, 0.0, 200.0, 100.0],
                [400.0, 900.0],
                [600.0, 1000.0],
            )];
            let mut motion = HashMap::new();
            let offset = use_signal(&keys.offset_y, || 0.0_f32);
            let target = use_signal(&keys.target_y, || 0.0_f32);
            target.set_silent(300.0);
            assert!(advance_secondary_scroll(
                &metrics,
                &mut motion,
                1.0 / 60.0,
                1.0
            ));
            assert!(offset.peek() > 0.0 && offset.peek() < target.peek());
            for _ in 0..180 {
                advance_secondary_scroll(&metrics, &mut motion, 1.0 / 60.0, 1.0);
            }
            assert_eq!(offset.peek(), 300.0);
            use_signal(&keys.behavior, || String::new()).set_silent("jump".into());
            target.set_silent(500.0);
            assert!(!advance_secondary_scroll(
                &metrics,
                &mut motion,
                1.0 / 60.0,
                1.0
            ));
            assert_eq!(offset.peek(), 500.0);
            target.set_silent(1200.0);
            advance_secondary_scroll(&metrics, &mut motion, 1.0 / 60.0, 1.0);
            assert_eq!(offset.peek(), 900.0);
            assert_eq!(target.peek(), 900.0);
        });
        assert_eq!(primary.peek(), 17.0);
    });
}

#[test]
fn secondary_metrics_enable_scrollbars_and_apply_alignment_in_the_owning_window() {
    create_root(|| {
        let rest = "secondary-metrics-test";
        let keys = ScrollViewportSignalKeys::new(rest);
        let primary = use_signal("__jadix_scroll_y_primary-app-content", || 17.0_f32);
        let _scope =
            crate::app::window_scope::enter(crate::app::window_scope::WindowScope::Secondary(9017));
        let mut viewport = activate_scroll_viewport(
            Element::new().w(200.0).h(100.0).flex_col().child(
                Element::new()
                    .w(600.0)
                    .h(1000.0)
                    .min_h(1000.0)
                    .flex_shrink(0.0)
                    .child(
                        Element::new()
                            .template_id("secondary-target")
                            .absolute()
                            .top(600.0)
                            .left(400.0)
                            .w(20.0)
                            .h(20.0),
                    ),
            ),
            rest,
            "auto",
            "auto",
            "jump",
            "scrollbar",
        );
        let mut layout = LayoutTree::new();
        let result = layout.build_and_layout(&mut viewport, 200.0, 100.0, [-1.0; 2]);
        super::super::scroll_support::request_scroll_descendant_into_view_when(
            rest,
            "secondary-target",
            super::super::scroll_support::ScrollItemAlign::Center,
            super::super::scroll_support::ScrollAlignWhen::Before,
        );
        assert!(sync_secondary_scroll_metrics(
            &viewport,
            &result.scroll_metrics
        ));
        assert_eq!(use_signal(&keys.viewport_h, || 0.0_f32).peek(), 100.0);
        assert_eq!(use_signal(&keys.content_h, || 0.0_f32).peek(), 1000.0);
        assert_eq!(use_signal(&keys.offset_y, || 0.0_f32).peek(), 560.0);
        assert_eq!(use_signal(&keys.offset_x, || 0.0_f32).peek(), 310.0);
        assert_eq!(primary.peek(), 17.0, "must not scroll the primary window");
        let mut activity = HashMap::new();
        sync_secondary_scrollbars(&result.scroll_metrics, [199.0, 50.0], &mut activity);
        for _ in 0..12 {
            crate::style::tick_animations(0.05);
        }
        assert!(crate::style::use_ui_transition_cubic(&keys.spring_y, 0.0).peek() > 0.9);
        assert!(crate::style::use_ui_transition_cubic(&keys.spring_x, 0.0).peek() < 0.01);
        sync_secondary_scrollbars(&result.scroll_metrics, [-1.0, -1.0], &mut activity);
        for _ in 0..12 {
            crate::style::tick_animations(0.05);
        }
        assert!(crate::style::use_ui_transition_cubic(&keys.spring_y, 0.0).peek() < 0.01);
    });
}
