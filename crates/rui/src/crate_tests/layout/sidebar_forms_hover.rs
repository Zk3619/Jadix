//! Exercise the actual Gallery switch and pointer path, including Forms portals.
use crate::app::{reactive, runtime, tree_hit};
use crate::core::reactivity::{
    create_effect, create_root, mark_next_effect_as_ui_root, use_signal,
};
use crate::core::tree::{find_by_template_id, Element, LayoutTree};
use std::{cell::RefCell, rc::Rc};

#[test]
fn gallery_sidebar_hover_survives_switch_to_forms() {
    crate::app::with_ui_build_stack(|| {
        reactive::reset_registered_anchors();
        tree_hit::clear_hover_tracking();
        let mut fonts = crate::renderer::font_loader::create_font_system();
        crate::renderer::font_loader::apply_default_families(fonts.db_mut());
        crate::text::install_shared_font_system(&mut fonts);
        let live = Rc::new(RefCell::new(Element::new()));
        runtime::install_test_reconcile_root(live.clone());
        let mounted = live.clone();
        mark_next_effect_as_ui_root();
        let (owner, _) = create_root(move || {
            use_signal("gal_nav_section", || "events".to_string()).set("events".into());
            create_effect(move || {
                crate::style::track_style_changes();
                let mut next = crate::gallery_app::build_gallery_app();
                let mut old = mounted.borrow_mut();
                crate::app::test_support::reconcile_elements(&mut next, &mut old);
                *old = next;
            });
        });
        reactive::drain_deferred_view_remounts();
        let mut layout = LayoutTree::new();
        const W: f32 = 1280.0;
        const H: f32 = 800.0;
        {
            let mut root = live.borrow_mut();
            layout.build_and_layout(&mut root, W, H, [-1.0; 2]);
            layout.set_collect_skip_allowed(true);
            for _ in 0..3 {
                layout.build_and_layout_fast(&mut root, W, H, [-1.0; 2], true, false);
            }
        }
        let click = find_by_template_id(&live.borrow(), "gal-nav-forms")
            .unwrap()
            .on_click
            .clone()
            .unwrap();
        (click.borrow_mut())();
        reactive::drain_deferred_view_remounts();
        assert_eq!(use_signal("gal_nav_section", String::new).peek(), "forms");
        {
            let mut root = live.borrow_mut();
            layout.build_and_layout_fast(&mut root, W, H, [-1.0; 2], true, true);
            assert!(
                crate::core::tree::find_in_tree(&root, |el| el.focus_id.as_deref()
                    == Some("scroll:scroll-forms"))
                .is_some()
            );
            for id in [
                "gal-nav-events",
                "gal-nav-basics",
                "gal-nav-data",
                "gal-nav-events",
            ] {
                let node = find_by_template_id(&root, id).unwrap();
                let [x, y, w, h] = node.layout_bounds;
                let mouse = [x + w * 0.5, y + h * 0.5];
                let path = crate::app::pointer_events::hit_path(&root, mouse[0], mouse[1]);
                assert!(
                    path.iter()
                        .any(|entry| entry.focus_id.as_deref() == Some(id)),
                    "{id} must be hit after Forms mounts; path={:?}",
                    path.iter()
                        .map(|e| (&e.focus_id, &e.floating_id, e.bounds))
                        .collect::<Vec<_>>()
                );
                tree_hit::sync_pointer_component_chrome_after_layout(
                    &mut root,
                    &mut layout,
                    mouse,
                    W,
                    H,
                );
                assert!(
                    use_signal(&format!("{id}_hover"), || false).peek(),
                    "{id} must receive hover"
                );
                let node = find_by_template_id(&root, id).unwrap();
                let accent = crate::style::parse_color("sidebar-accent");
                assert_eq!(node.style.bg_color, accent, "{id} must apply hover chrome");
                let frame = layout.build_and_layout_fast(&mut root, W, H, mouse, true, false);
                assert!(
                    frame.instances.iter().any(|i| {
                        (i.rect_pos[0] - x).abs() < 1.0
                            && (i.rect_pos[1] - y).abs() < 1.0
                            && i.rect_size[0] > w - 2.0
                            && i.color == accent
                    }),
                    "{id} must collect hovered paint after Forms mounts"
                );
            }
        }
        tree_hit::clear_hover_tracking();
        drop(owner);
        runtime::clear_test_reconcile_root();
        reactive::reset_registered_anchors();
    });
}
