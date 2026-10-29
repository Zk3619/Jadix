use super::*;

fn label(text: &str) -> Element {
    let mut el = Element::new();
    el.text = Some(text.to_string());
    el
}

#[test]
fn same_glyph_count_keeps_the_same_intrinsic_width() {
    let el = label("10");
    let before = text_intrinsic_width(&el, "10").expect("measurable");
    let after = text_intrinsic_width(&el, "11").expect("measurable");
    assert!(
        (before - after).abs() < 0.5,
        "digit-only counters should not resize: {before} vs {after}"
    );
}

#[test]
fn longer_text_reports_a_wider_intrinsic_width() {
    let el = label("1");
    let before = text_intrinsic_width(&el, "1").expect("measurable");
    let after = text_intrinsic_width(&el, "1000000").expect("measurable");
    assert!(after > before + 0.5, "{before} vs {after}");
}

#[test]
fn non_leaf_nodes_are_not_comparable() {
    let el = Element::new().children(vec![label("x")]);
    assert!(text_intrinsic_width(&el, "x").is_none());
}

#[test]
fn hard_newline_grows_layout_height() {
    let el = label("a");
    let before = text_layout_height(&el, "a", f32::MAX).expect("measurable");
    let after = text_layout_height(&el, "a\nb", f32::MAX).expect("measurable");
    assert!(
        after > before + 0.5,
        "hard newline must grow height for bind_text LayoutChanged: {before} vs {after}"
    );
}

#[test]
fn hard_newline_shrink_requests_layout() {
    let el = label("a\nb\nc");
    assert!(
        bind_text_metric_needs_layout(&el, "a\nb\nc", "a"),
        "deleting hard lines must LayoutChanged so echo Labels collapse"
    );
    let same = label("10");
    assert!(
        !bind_text_metric_needs_layout(&same, "10", "11"),
        "equal-metric digit counters stay paint-only"
    );
}

#[test]
fn padded_mouse_log_coord_tick_layout_status() {
    // Gallery MouseEvent readout: fixed 7 padded rows, stretch width.
    fn log_at(x: f32, y: f32) -> String {
        format!(
            "{:<10} detail={:<2} button={} buttons={:<3} which={}\n\
client=({:>7.1},{:>7.1})  offset=({:>7.1},{:>7.1})\n\
page=({:>7.1},{:>7.1})    screen=({:>7.1},{:>7.1})\n\
movement=({:>7.1},{:>7.1})\n\
Alt={:<5} Ctrl={:<5} Shift={:<5} Meta={:<5} Accel={}\n\
region={}  related={}\n\
phase={:<8}  stop={}  prevent={}",
            "mousemove",
            0,
            0,
            0,
            1,
            x,
            y,
            x,
            y,
            x,
            y,
            x,
            y,
            1.7,
            0.9,
            false,
            false,
            false,
            false,
            false,
            format!("{:<18}", "-"),
            format!("{:<18}", "gal-mouse-pad"),
            "Target",
            false,
            false,
        )
    }
    let mut el = label("").w("100%");
    el.text_style.size = Some(12.0);
    el.text_style.line_height_scale = Some(1.5);
    el.layout_bounds = [12.0, 80.0, 400.0, 126.0];
    let a = log_at(410.0, 82.0);
    let b = log_at(411.7, 82.9);
    assert_eq!(a.len(), b.len(), "padded fields must keep byte length");
    // Font initialization is a first-use cost, not a mousemove cost.
    assert!(!bind_text_metric_needs_layout(&el, &a, &b));
    let mut needs_any = false;
    let mut metric_samples = [0.0_f32; 5];
    for sample in &mut metric_samples {
        let start = std::time::Instant::now();
        for tick in 0..80 {
            let x = 410.0 + tick as f32 * 1.7;
            let y = 82.0 + tick as f32 * 0.9;
            needs_any |=
                bind_text_metric_needs_layout(&el, &log_at(x, y), &log_at(x + 1.7, y + 0.9));
        }
        *sample = start.elapsed().as_secs_f32() * 1000.0;
    }
    metric_samples.sort_by(f32::total_cmp);
    let metric_ms = metric_samples[metric_samples.len() / 2];
    assert!(
        !needs_any,
        "definite-width padded coords must stay paint-only"
    );
    assert!(
            metric_ms < 5.0,
            "pointer path must not cosmic-prepare every mousemove (median {metric_ms:.2}ms / 80 ticks; batches={metric_samples:?})"
        );
    let mut node = label("").w("100%");
    node.text_style.size = Some(12.0);
    node.text_style.line_height_scale = Some(1.5);
    node.layout_bounds = el.layout_bounds;
    node.text = Some(a);
    let status = apply_bound_text_value(&mut node, b);
    assert_eq!(status, LayoutStatus::StyleChanged);
}

#[test]
fn specified_width_play_pause_stays_paint_only() {
    // Gallery play-state: "Pause"/"Play" different byte lengths, but the used
    // box is authored. Must not LayoutChanged (root Taffy hitch sibling CSS).
    let mut el = label("Pause").w(76.0);
    el.layout_bounds = [8.0, 8.0, 76.0, 28.0];
    assert!(
        !bind_text_metric_needs_layout(&el, "Pause", "Play"),
        "definite-width Play/Pause must stay paint-only"
    );
    let status = apply_bound_text_value(&mut el, "Play".into());
    assert_eq!(status, LayoutStatus::StyleChanged);
}

#[test]
fn specified_width_go_reset_stays_paint_only() {
    let mut el = label("Go").w("100%");
    el.layout_bounds = [8.0, 8.0, 68.0, 28.0];
    let t0 = std::time::Instant::now();
    let mut needs = false;
    for _ in 0..80 {
        needs |= bind_text_metric_needs_layout(&el, "Go", "Reset");
        needs |= bind_text_metric_needs_layout(&el, "Reset", "Go");
    }
    let metric_ms = t0.elapsed().as_secs_f32() * 1000.0;
    assert!(!needs, "fixed-width Go/Reset must stay paint-only");
    assert!(
        metric_ms < 5.0,
        "Go/Reset must not cosmic-prepare on click ({metric_ms:.2}ms / 160 ticks)"
    );
    let status = apply_bound_text_value(&mut el, "Reset".into());
    assert_eq!(status, LayoutStatus::StyleChanged);
}

#[test]
fn bind_layout_absorb_min_w_button_is_absorbed() {
    let mut text = Element::new().text("Pause");
    text.layout_bounds = [8.0, 80.0, 48.0, 20.0];
    let mut btn = Element::new().min_w(76.0).child(text);
    btn.layout_bounds = [8.0, 72.0, 76.0, 28.0];
    let mut root = Element::new().w("100%").child(btn);
    root.layout_bounds = [0.0, 0.0, 320.0, 200.0];
    assert!(
        bind_layout_absorbed_by_specified_ancestor(&root, &[0, 0]),
        "min_w Button on the bind_text path must absorb leaf LayoutChanged"
    );
    let mut chip = Element::new().text("Blocked");
    chip.layout_bounds = [8.0, 8.0, 64.0, 20.0];
    let mut page = Element::new().w("100%").child(chip);
    page.layout_bounds = [0.0, 0.0, 320.0, 200.0];
    assert!(
        !bind_layout_absorbed_by_specified_ancestor(&page, &[0]),
        "content-sized badge under % width page must still LayoutChanged"
    );
}

#[test]
fn bind_layout_absorb_wrap_row_without_height_still_layouts() {
    let mut text = Element::new().text("hi");
    text.layout_bounds = [0.0, 0.0, 40.0, 16.0];
    let mut row = Element::new().w(320.0).child(text);
    row.layout_bounds = [0.0, 0.0, 320.0, 0.0];
    let mut page = Element::new().w("100%").child(row);
    page.layout_bounds = [0.0, 0.0, 320.0, 200.0];
    assert!(
        !bind_layout_absorbed_by_specified_ancestor(&page, &[0, 0]),
        "% / wrap row with no laid-out height must not swallow leaf LayoutChanged (footer Y)"
    );
}

#[test]
fn shorter_label_shrink_requests_layout() {
    // Gallery ReactiveStatusBadge: "Blocked" → "Ready" must collapse chip width.
    let el = label("Blocked");
    let before = text_intrinsic_width(&el, "Blocked").expect("before");
    let after = text_intrinsic_width(&el, "Ready").expect("after");
    assert!(
        before > after + 0.5,
        "fixture must shrink: {before} → {after}"
    );
    assert!(
        bind_text_metric_needs_layout(&el, "Blocked", "Ready"),
        "width shrink must LayoutChanged (not paint-only StyleChanged)"
    );
    assert!(
        bind_text_metric_needs_layout(&el, "Ready", "Blocked"),
        "width grow must still LayoutChanged"
    );
}

#[test]
fn switch_reactive_builds_placeholder() {
    use crate::core::reactivity::{create_effect, create_root, create_signal};
    let (_o, tid) = create_root(|| {
        let key = create_signal(0_i32);
        let out = std::rc::Rc::new(std::cell::RefCell::new(None::<String>));
        let out2 = out.clone();
        create_effect(move || {
            let el = switch_reactive(
                "sw-test",
                move || key.get(),
                |k| Element::new().text(format!("arm-{k}")),
            );
            *out2.borrow_mut() = el.template_id.clone();
        });
        let tid = out.borrow().clone().expect("id");
        tid
    });
    assert_eq!(tid, "sw-test");
}

#[test]
fn match_when_reactive_builds_placeholder() {
    use crate::core::reactivity::{create_effect, create_root, create_signal};
    let (_o, tid) = create_root(|| {
        let ok = create_signal(true);
        let out = std::rc::Rc::new(std::cell::RefCell::new(None::<String>));
        let out2 = out.clone();
        create_effect(move || {
            let el = match_when_reactive("match-test", move || {
                if ok.get() {
                    Element::new().text("yes")
                } else {
                    Element::new().text("no")
                }
            });
            *out2.borrow_mut() = el.template_id.clone();
        });
        let tid = out.borrow().clone().expect("id");
        tid
    });
    assert_eq!(tid, "match-test");
}

#[test]
fn element_text_signal_and_closure_bind() {
    use crate::core::reactivity::{create_effect, create_root, use_signal};
    reset_registered_anchors();
    let (_o, (sig_id, clos_id, snap_id)) = create_root(|| {
        let sig = use_signal("__jadix_into_node_text", || "live".to_string());
        sig.set_silent("live".to_string());
        let out = std::rc::Rc::new(std::cell::RefCell::new(
            None::<(Option<String>, Option<String>, Option<String>)>,
        ));
        let out2 = out.clone();
        create_effect(move || {
            let bound = Element::new().text(sig);
            let clos = Element::new().text(move || sig.get());
            let snap = Element::new().text(sig.get());
            *out2.borrow_mut() = Some((
                bound.template_id.clone(),
                clos.template_id.clone(),
                snap.template_id.clone(),
            ));
        });
        let ids = out.borrow().clone().expect("ids");
        ids
    });
    assert!(
        sig_id
            .as_deref()
            .is_some_and(|id| id.contains("__jadix_text_sig")),
        "Element::text(sig) must bind; id={sig_id:?}"
    );
    assert!(
        clos_id.is_some(),
        "Element::text(|| sig.get()) must bind; id={clos_id:?}"
    );
    assert!(
        snap_id.is_none(),
        "Element::text(sig.get()) is a snapshot; id={snap_id:?}"
    );
    reset_registered_anchors();
}

#[test]
fn reactive_anchor_registers_independently_per_window_scope() {
    use crate::app::window_scope::{enter, WindowScope};
    use crate::core::reactivity::{
        bind_current_owner_window_key_scope, create_effect, create_root, create_signal,
    };

    reset_registered_anchors();
    let before = registered_anchor_count();

    let (_o1, _) = create_root(|| {
        let _g = enter(WindowScope::Primary);
        bind_current_owner_window_key_scope(0);
        let open = create_signal(false);
        create_effect(move || {
            let _g = enter(WindowScope::Primary);
            reactive_anchor("shared_dialog_view", move || {
                let _ = open.get();
                Element::new().text("primary")
            });
        });
    });
    assert_eq!(registered_anchor_count(), before + 1);

    let (_o2, _) = create_root(|| {
        bind_current_owner_window_key_scope(7);
        let open = create_signal(false);
        create_effect(move || {
            let _g = enter(WindowScope::Secondary(7));
            reactive_anchor("shared_dialog_view", move || {
                let _ = open.get();
                Element::new().text("secondary")
            });
        });
    });
    assert_eq!(
        registered_anchor_count(),
        before + 2,
        "same anchor id must register once per window scope"
    );
    reset_registered_anchors();
}
