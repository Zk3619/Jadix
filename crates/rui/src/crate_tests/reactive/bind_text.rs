use crate::app::reactive::{bind_class, bind_prop, bind_text, bind_with, reset_registered_anchors};
use crate::app::test_support::reconcile_elements;
use crate::core::reactivity::{create_effect, dispose_effect, use_signal};
use crate::core::tree::{Element, LayoutStatus};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[test]
fn bind_text_sets_initial_value_without_tracking() {
    reset_registered_anchors();
    let el = bind_text("bind:init", || "hello".to_string());
    assert_eq!(el.template_id.as_deref(), Some("bind:init"));
    assert_eq!(el.text.as_deref(), Some("hello"));
    reset_registered_anchors();
}

#[test]
fn bind_text_effect_reruns_on_signal_without_reactive_view() {
    reset_registered_anchors();
    let sig = use_signal("__rui_test_bind_text", || "a".to_string());
    sig.set_silent("a".to_string());
    let runs = Rc::new(Cell::new(0_u32));
    let mounted = Rc::new(RefCell::new(None::<Element>));

    let runs_a = Rc::clone(&runs);
    let mounted_a = Rc::clone(&mounted);
    let root = create_effect(move || {
        let runs_a = Rc::clone(&runs_a);
        let el = bind_text("bind:signal", move || {
            runs_a.set(runs_a.get() + 1);
            sig.get()
        });
        *mounted_a.borrow_mut() = Some(el);
    });

    // untrack(initial) + first binding effect run
    assert_eq!(runs.get(), 2);
    assert_eq!(
        mounted
            .borrow()
            .as_ref()
            .and_then(|e| e.text.clone())
            .as_deref(),
        Some("a")
    );

    sig.set("b".to_string());
    assert_eq!(runs.get(), 3, "binding effect should re-run on signal set");

    drop(mounted.borrow_mut().take());
    dispose_effect(root);
    reset_registered_anchors();
}

#[test]
fn bind_text_hook_survives_reconcile_same_template_id() {
    reset_registered_anchors();
    let sig = use_signal("__rui_test_bind_reconcile", || "one".to_string());
    let runs = Rc::new(Cell::new(0_u32));
    let mounted = Rc::new(RefCell::new(None::<Element>));

    let runs_a = Rc::clone(&runs);
    let mounted_a = Rc::clone(&mounted);
    let root = create_effect(move || {
        let runs_a = Rc::clone(&runs_a);
        let leaf = bind_text("bind:reconcile", move || {
            runs_a.set(runs_a.get() + 1);
            sig.get()
        });
        let mut wrap = Element::new().template_id("wrap");
        wrap = wrap.child(leaf);
        *mounted_a.borrow_mut() = Some(wrap);
    });
    assert_eq!(runs.get(), 2);

    // Simulate root rebuild recreating the same bind_text leaf.
    let runs_b = Rc::clone(&runs);
    let mut next = {
        let leaf = bind_text("bind:reconcile", move || {
            runs_b.set(runs_b.get() + 1);
            sig.get()
        });
        Element::new().template_id("wrap").child(leaf)
    };
    // already-registered path only reattaches hook; compute above is unused (untrack still runs once)
    let after_rebuild_untrack = runs.get();
    assert!(after_rebuild_untrack >= 2);

    let mut old = mounted.borrow_mut().take().unwrap();
    let status = reconcile_elements(&mut next, &mut old);
    assert_ne!(status, LayoutStatus::StructureChanged);
    assert!(next.children[0].on_unmount.is_some());

    *mounted.borrow_mut() = Some(next);
    let before = runs.get();
    sig.set("two".to_string());
    assert_eq!(
        runs.get(),
        before + 1,
        "binding must survive reconcile and keep listening"
    );

    drop(mounted.borrow_mut().take());
    dispose_effect(root);
    reset_registered_anchors();
}

#[test]
fn bind_with_can_patch_text_color_style() {
    reset_registered_anchors();
    let color = use_signal("__rui_test_bind_color", || [1_u8, 2, 3, 255]);
    let mounted = Rc::new(RefCell::new(None::<Element>));
    let mounted_a = Rc::clone(&mounted);
    let root = create_effect(move || {
        let el = bind_with(
            "bind:color",
            move || color.get(),
            |el, rgba| {
                if el.text_style.color == Some(rgba) {
                    LayoutStatus::Clean
                } else {
                    el.text_style.color = Some(rgba);
                    LayoutStatus::StyleChanged
                }
            },
        );
        *mounted_a.borrow_mut() = Some(el);
    });

    assert_eq!(
        mounted
            .borrow()
            .as_ref()
            .map(|e| e.text_style.color_resolved()),
        Some([1, 2, 3, 255])
    );

    drop(mounted.borrow_mut().take());
    dispose_effect(root);
    reset_registered_anchors();
}

#[test]
fn bind_prop_applies_initial_style_and_reruns() {
    reset_registered_anchors();
    let color = use_signal("__rui_test_bind_prop_bg", || [10_u8, 20, 30, 255]);
    color.set_silent([10, 20, 30, 255]);
    let runs = Rc::new(Cell::new(0_u32));
    let mounted = Rc::new(RefCell::new(None::<Element>));

    let runs_a = Rc::clone(&runs);
    let mounted_a = Rc::clone(&mounted);
    let root = create_effect(move || {
        let runs_a = Rc::clone(&runs_a);
        let el = bind_prop(
            Element::new(),
            "bind:prop:bg",
            move || {
                runs_a.set(runs_a.get() + 1);
                color.get()
            },
            |el, next| {
                let rgba = [
                    next[0] as f32 / 255.0,
                    next[1] as f32 / 255.0,
                    next[2] as f32 / 255.0,
                    next[3] as f32 / 255.0,
                ];
                if el.style.bg_color == rgba {
                    LayoutStatus::Clean
                } else {
                    el.style.bg_color = rgba;
                    LayoutStatus::StyleChanged
                }
            },
        );
        *mounted_a.borrow_mut() = Some(el);
    });

    assert_eq!(runs.get(), 2);
    assert_eq!(
        mounted.borrow().as_ref().map(|e| e.style.bg_color),
        Some([10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0, 1.0])
    );

    let before = runs.get();
    color.set([40, 50, 60, 255]);
    assert_eq!(runs.get(), before + 1);

    drop(mounted.borrow_mut().take());
    dispose_effect(root);
    reset_registered_anchors();
}

#[test]
fn bind_class_applies_utility_tokens() {
    reset_registered_anchors();
    let classes = use_signal("__rui_test_bind_class", || "bg-red-500".to_string());
    classes.set_silent("bg-red-500".to_string());
    let runs = Rc::new(Cell::new(0_u32));
    let mounted = Rc::new(RefCell::new(None::<Element>));

    let runs_a = Rc::clone(&runs);
    let mounted_a = Rc::clone(&mounted);
    let root = create_effect(move || {
        let runs_a = Rc::clone(&runs_a);
        let el = bind_class(Element::new(), "bind:class", move || {
            runs_a.set(runs_a.get() + 1);
            classes.get()
        });
        *mounted_a.borrow_mut() = Some(el);
    });

    assert_eq!(runs.get(), 2);
    assert_eq!(
        mounted
            .borrow()
            .as_ref()
            .and_then(|e| e.attrs.get("rui_utility_class").cloned())
            .as_deref(),
        Some("bg-red-500")
    );

    let before = runs.get();
    classes.set("text-lg".to_string());
    assert_eq!(runs.get(), before + 1);

    drop(mounted.borrow_mut().take());
    dispose_effect(root);
    reset_registered_anchors();
}

#[test]
fn bind_text_reconcile_preserves_sibling_layout_node() {
    reset_registered_anchors();
    let mut old_root = Element::new().template_id("root");
    let mut sibling = Element::new().template_id("sib");
    sibling.text = Some("stable".to_string());
    sibling.layout_node = Some(taffy::prelude::NodeId::new(11));
    let mut target = Element::new().template_id("bind:target");
    target.text = Some("one".to_string());
    target.layout_node = Some(taffy::prelude::NodeId::new(22));
    old_root.children = vec![sibling, target];

    let mut new_root = Element::new().template_id("root");
    let mut sibling2 = Element::new().template_id("sib");
    sibling2.text = Some("stable".to_string());
    let mut target2 = Element::new().template_id("bind:target");
    target2.text = Some("two".to_string());
    new_root.children = vec![sibling2, target2];

    let status = reconcile_elements(&mut new_root, &mut old_root);
    assert_ne!(status, LayoutStatus::StructureChanged);
    assert_eq!(
        new_root.children[0].layout_node,
        Some(taffy::prelude::NodeId::new(11)),
        "sibling layout identity must survive text-only patch"
    );
    assert_eq!(
        new_root.children[1].layout_node,
        Some(taffy::prelude::NodeId::new(22)),
        "target layout identity must survive text-only patch"
    );
    assert_eq!(new_root.children[1].text.as_deref(), Some("two"));
    reset_registered_anchors();
}

#[test]
fn apply_bound_button_variant_updates_chrome_in_place() {
    use crate::components::{apply_bound_button_variant, button};
    let mut el: Element = button("bind-btn-variant")
        .variant("default")
        .text("Go")
        .into();
    assert_eq!(
        el.button_hover_paint.as_ref().map(|d| d.variant.as_str()),
        Some("default")
    );
    let status = apply_bound_button_variant(&mut el, "outline");
    assert_eq!(status, LayoutStatus::StyleChanged);
    assert_eq!(
        el.button_hover_paint.as_ref().map(|d| d.variant.as_str()),
        Some("outline")
    );
    let again = apply_bound_button_variant(&mut el, "outline");
    assert_eq!(again, LayoutStatus::Clean);
}

#[test]
fn apply_bound_unknown_button_variant_falls_back_to_default() {
    use crate::components::{apply_bound_button_variant, button};
    let mut el: Element = button("bind-btn-unknown")
        .variant("ghost")
        .text("Go")
        .into();
    let status = apply_bound_button_variant(&mut el, "not-a-variant");
    assert_eq!(status, LayoutStatus::StyleChanged);
    assert_eq!(
        el.button_hover_paint.as_ref().map(|d| d.variant.as_str()),
        Some("default")
    );
}

#[test]
fn apply_bound_icon_name_swaps_lucide_glyph() {
    use crate::utils::{apply_bound_icon_name, build_named_icon_text_node, lucide_glyph};
    let check = lucide_glyph("check").expect("lucide check");
    let ban = lucide_glyph("ban").expect("lucide ban");
    let mut el = build_named_icon_text_node("check", check.clone());
    assert_eq!(apply_bound_icon_name(&mut el, "check"), LayoutStatus::Clean);
    assert_eq!(
        apply_bound_icon_name(&mut el, "ban"),
        LayoutStatus::LayoutChanged
    );
    assert_eq!(el.text.as_deref(), Some(ban.as_str()));
}

#[test]
fn bind_style_applies_bg_and_class_patch() {
    use crate::app::reactive::{bind_style, StylePatch};
    reset_registered_anchors();
    let tone = use_signal("__rui_test_bind_style", || "primary".to_string());
    tone.set_silent("primary".to_string());
    let mounted = Rc::new(RefCell::new(None::<Element>));
    let mounted_a = Rc::clone(&mounted);
    let root = create_effect(move || {
        let el = bind_style(Element::new(), "bind:style:patch", move || StylePatch {
            bg: Some(crate::style::parse_color(tone.get().as_str())),
            class: Some("rounded".to_string()),
            ..Default::default()
        });
        *mounted_a.borrow_mut() = Some(el);
    });
    assert!(mounted.borrow().as_ref().is_some());
    drop(mounted.borrow_mut().take());
    dispose_effect(root);
    reset_registered_anchors();
}

#[test]
fn apply_bound_size_marks_layout_changed_on_width() {
    use crate::app::reactive::apply_bound_size;
    use crate::core::tree::IntoDimension;
    let mut el = Element::new().w("3.5rem");
    let narrow = IntoDimension::into_dimension("3.5rem");
    let wide = IntoDimension::into_dimension("11rem");
    assert_eq!(apply_bound_size(&mut el, "w", narrow), LayoutStatus::Clean);
    assert_eq!(
        apply_bound_size(&mut el, "w", wide),
        LayoutStatus::LayoutChanged
    );
    assert_eq!(el.style.size.width, wide);
}

#[test]
fn bind_prop_applies_initial_width() {
    use crate::app::reactive::{apply_bound_size, bind_prop};
    use crate::core::tree::IntoDimension;
    reset_registered_anchors();
    let wide = use_signal("__rui_test_bind_size_w", || false);
    wide.set_silent(false);
    let el = bind_prop(
        Element::new().template_id("bind:size:w"),
        "bind:size:w",
        move || IntoDimension::into_dimension(if wide.get() { "11rem" } else { "3.5rem" }),
        |el, dim| apply_bound_size(el, "w", dim),
    );
    assert_eq!(el.style.size.width, IntoDimension::into_dimension("3.5rem"));
    reset_registered_anchors();
}

#[test]
fn bind_prop_rebuild_does_not_seed_from_paint_when_effect_alive() {
    use taffy::style::Dimension;
    reset_registered_anchors();
    let open = use_signal("__rui_test_bind_prop_skip_paint", || false);
    open.set_silent(false);
    let applies = Rc::new(Cell::new(0_u32));
    let mounted = Rc::new(RefCell::new(None::<Element>));

    let applies_a = Rc::clone(&applies);
    let mounted_a = Rc::clone(&mounted);
    let root = create_effect(move || {
        let applies_a = Rc::clone(&applies_a);
        let el = bind_prop(
            Element::new().w("16rem"),
            "bind:skip-paint-width",
            move || open.get(),
            move |el, is_open| {
                applies_a.set(applies_a.get() + 1);
                el.style.size.width = Dimension::length(if is_open { 256.0 } else { 48.0 });
                LayoutStatus::LayoutChanged
            },
        );
        *mounted_a.borrow_mut() = Some(el);
    });

    let after_first = applies.get();
    assert!(after_first >= 1, "first mount must apply");
    assert_eq!(
        crate::layout::dim_length_px(mounted.borrow().as_ref().unwrap().style.size.width),
        Some(48.0)
    );

    // Sibling tracking scope: the original bind effect stays alive (parent is not
    // re-run). Rebuild must not apply paint 16rem onto the fragment.
    let applies_b = Rc::clone(&applies);
    let mounted_b = Rc::clone(&mounted);
    let rebuild = create_effect(move || {
        let applies_b = Rc::clone(&applies_b);
        let el = bind_prop(
            Element::new().w("16rem"),
            "bind:skip-paint-width",
            move || open.get(),
            move |el, is_open| {
                applies_b.set(applies_b.get() + 1);
                el.style.size.width = Dimension::length(if is_open { 256.0 } else { 48.0 });
                LayoutStatus::LayoutChanged
            },
        );
        *mounted_b.borrow_mut() = Some(el);
    });
    assert_eq!(
        applies.get(),
        after_first,
        "rebuild fragment must not re-apply (paint would seed the tween)"
    );
    assert_eq!(
        crate::layout::dim_length_px(mounted.borrow().as_ref().unwrap().style.size.width),
        Some(256.0),
        "skipped apply leaves paint 16rem on the fragment"
    );

    drop(mounted.borrow_mut().take());
    dispose_effect(rebuild);
    dispose_effect(root);
    reset_registered_anchors();
}

#[test]
fn bind_miss_warns_when_unmounted() {
    use crate::app::reactive::{bind_miss_count, reset_bind_miss_count};
    use crate::app::test_support::resolve_bind_template_id;

    reset_bind_miss_count();
    let root = Element::new();
    assert!(
        !resolve_bind_template_id(&root, "unmounted"),
        "absent template_id is a miss"
    );
    assert!(
        bind_miss_count() >= 1,
        "miss must increment bind_miss_count (was silent Clean)"
    );
}

#[test]
fn bind_miss_hit_does_not_count() {
    use crate::app::reactive::{bind_miss_count, reset_bind_miss_count};
    use crate::app::test_support::resolve_bind_template_id;

    reset_bind_miss_count();
    let root = Element::new().child(Element::new().template_id("alive"));
    assert!(resolve_bind_template_id(&root, "alive"));
    assert_eq!(bind_miss_count(), 0);
}

#[test]
fn leaf_bind_structure_changed_coerces_to_layout() {
    use crate::app::reactive::sanitize_leaf_bind_status;

    assert_eq!(
        sanitize_leaf_bind_status(LayoutStatus::StructureChanged),
        LayoutStatus::LayoutChanged
    );
    assert_eq!(
        sanitize_leaf_bind_status(LayoutStatus::StyleChanged),
        LayoutStatus::StyleChanged
    );
}

#[test]
fn bind_prop_list_items_with_same_site_key_get_own_effects() {
    reset_registered_anchors();
    let applied = Rc::new(RefCell::new(Vec::<String>::new()));
    let root = create_effect({
        let applied = Rc::clone(&applied);
        move || {
            let a = Rc::clone(&applied);
            let _ = bind_prop(
                Element::new().template_id("list-a"),
                "bind:list:shared-site",
                || 0_i32,
                move |el, _| {
                    a.borrow_mut()
                        .push(el.template_id.clone().unwrap_or_default());
                    LayoutStatus::Clean
                },
            );
            let b = Rc::clone(&applied);
            let _ = bind_prop(
                Element::new().template_id("list-b"),
                "bind:list:shared-site",
                || 0_i32,
                move |el, _| {
                    b.borrow_mut()
                        .push(el.template_id.clone().unwrap_or_default());
                    LayoutStatus::Clean
                },
            );
        }
    });
    let got = applied.borrow().clone();
    assert_eq!(
        got,
        vec!["list-a".to_string(), "list-b".to_string()],
        "same ui! bind_key must not skip later list items"
    );
    dispose_effect(root);
    reset_registered_anchors();
}
