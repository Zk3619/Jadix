use super::*;
use crate::app::*;
use crate::imports::*;
use crate::pages::*;
use crate::shared::*;
use rui::core::reactivity::{create_effect, create_root};

/// Headless tests build one page at a time (same trees as the live `<When>` arms).
fn gallery_section_body(section: &str) -> rui::core::tree::Element {
    match section {
        "forms" => GallerySectionForms(GallerySectionFormsProps::default()),
        "events" => GallerySectionEvents(GallerySectionEventsProps::default()),
        "overlays" => GallerySectionOverlays(GallerySectionOverlaysProps::default()),
        "draw" => GallerySectionDraw(GallerySectionDrawProps::default()),
        "data" => GallerySectionData(GallerySectionDataProps::default()),
        "navigation" => GallerySectionNavigation(GallerySectionNavigationProps::default()),
        "layout" => GallerySectionLayout(GallerySectionLayoutProps::default()),
        "text" => GallerySectionText(GallerySectionTextProps::default()),
        "theme" => GallerySectionTheme(GallerySectionThemeProps::default()),
        "about" => GallerySectionAbout(GallerySectionAboutProps::default()),
        _ => GallerySectionBasics(GallerySectionBasicsProps::default()),
    }
}

fn gallery_find_tid(el: &rui::core::tree::Element, want: &str) -> bool {
    gallery_find_by_tid(el, want).is_some()
}

fn gallery_find_by_tid<'a>(
    el: &'a rui::core::tree::Element,
    want: &str,
) -> Option<&'a rui::core::tree::Element> {
    let mut stack = vec![el];
    while let Some(el) = stack.pop() {
        if el.template_id.as_deref() == Some(want) {
            return Some(el);
        }
        stack.extend(el.children.iter().rev());
    }
    None
}

fn gallery_find_slot<'a>(
    el: &'a rui::core::tree::Element,
    slot: &str,
) -> Option<&'a rui::core::tree::Element> {
    let mut stack = vec![el];
    while let Some(el) = stack.pop() {
        if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
            return Some(el);
        }
        stack.extend(el.children.iter().rev());
    }
    None
}

fn gallery_tid_contains(el: &rui::core::tree::Element, needle: &str) -> bool {
    let mut stack = vec![el];
    while let Some(el) = stack.pop() {
        if el
            .template_id
            .as_deref()
            .is_some_and(|id| id.contains(needle))
        {
            return true;
        }
        stack.extend(el.children.iter().rev());
    }
    false
}

#[test]
fn mouse_event_log_line_count_stable_from_idle_to_move() {
    assert_eq!(mouse_log_placeholder().lines().count(), MOUSE_LOG_LINES);
    let live = format_mouse_log_fields(
        "mousemove",
        0,
        0,
        0,
        1,
        [410.0, 82.0],
        [104.0, 116.0],
        [410.0, 82.0],
        [615.0, 123.0],
        [0.7, -0.7],
        false,
        false,
        false,
        false,
        false,
        None,
        Some("gal-mouse-pad"),
        "Target",
        false,
        false,
    );
    assert_eq!(
        live.lines().count(),
        MOUSE_LOG_LINES,
        "first mousemove must not add wrap rows vs idle placeholder"
    );
}

#[test]
fn mouse_event_log_panel_reserves_height_before_first_move() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree = rui::core::reactivity::isolate_reads(|| {
                    MouseEventShowcase(MouseEventShowcaseProps::default())
                });
                let mut layout = rui::core::tree::LayoutTree::new();
                let _ = layout.build_and_layout(&mut tree, 720.0, 900.0, [0.0, 0.0]);
                let panel =
                    gallery_find_by_tid(&tree, "gal-mouse-log-panel").expect("gal-mouse-log-panel");
                assert!(
                    (panel.layout_bounds[3] - MOUSE_LOG_PANEL_H).abs() < 2.0,
                    "idle coordinate panel must already be {}px tall (got {}), or first mousemove collapses the page",
                    MOUSE_LOG_PANEL_H,
                    panel.layout_bounds[3]
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn forms_section_under_isolate_reads_keeps_mark_invalid_caption() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                // Match App: `<When>` arm builds the section component (isolate_reads inside).
                let mut tree =
                    rui::core::reactivity::isolate_reads(|| gallery_section_body("forms"));
                fn find_btn<'a>(
                    el: &'a rui::core::tree::Element,
                ) -> Option<&'a rui::core::tree::Element> {
                    if el.template_id.as_deref() == Some("gal-s2-mark-invalid") {
                        return Some(el);
                    }
                    el.children.iter().find_map(find_btn)
                }
                let btn = find_btn(&tree).expect("gal-s2-mark-invalid button");
                assert!(
                    btn.children
                        .iter()
                        .any(|c| c.text.as_deref() == Some("Mark invalid")),
                    "button children={:?}",
                    btn.children
                        .iter()
                        .map(|c| (c.template_id.clone(), c.text.clone(), c.text_style.size))
                        .collect::<Vec<_>>()
                );
                let mut layout = rui::core::tree::LayoutTree::new();
                let result = layout.build_and_layout(&mut tree, 900.0, 800.0, [0.0, 0.0]);
                let node = result
                    .text_nodes
                    .iter()
                    .find(|n| n.text == "Mark invalid")
                    .expect("forms section must layout Mark invalid");
                assert!(
                    node.width > 40.0,
                    "caption width too small: {} (likely empty outline button)",
                    node.width
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn forms_color_picker_variants_tabs_mount_only_active() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let tree = rui::core::reactivity::isolate_reads(|| gallery_section_body("forms"));
                assert!(
                    gallery_tid_contains(&tree, "gal-cp-variants"),
                    "forms pickers must host ColorPicker variants under Tabs"
                );
                assert!(
                    gallery_tid_contains(&tree, "gal-cp-variants:content:sketch"),
                    "TabsContent placeholder for default sketch tab"
                );
                // Before: nine ColorPicker reactive placeholders were siblings in the
                // declarative forms tree. TabsContent keeps them out of that tree
                // (mounted only via show_reactive into the live reconcile slot).
                assert!(
                    !gallery_tid_contains(&tree, "gal-cp-chrome__cp_view"),
                    "inactive variant must not be an eager declarative sibling"
                );
                assert!(
                    !gallery_tid_contains(&tree, "gal-cp-compact__cp_view"),
                    "inactive variant must not be an eager declarative sibling"
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn picker_showcase_tabs_chips_layout_alone() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree = rui::core::reactivity::isolate_reads(|| {
                    PickerPlaceholdersShowcase(PickerPlaceholdersShowcaseProps::default())
                });
                let mut layout = rui::core::tree::LayoutTree::new();
                let result = layout.build_and_layout(&mut tree, 900.0, 800.0, [0.0, 0.0]);
                eprintln!(
                    "picker alone: root={:?} texts={} instances={}",
                    tree.layout_bounds,
                    result.text_nodes.len(),
                    result.instances.len()
                );
                for label in [
                    "Date", "DateTime", "Time", "Month", "Week", "Sketch", "Chrome",
                ] {
                    match result.text_nodes.iter().find(|n| n.text == label) {
                        Some(n) => eprintln!(
                            "  '{label}' @ ({:.1},{:.1}) {:.1}x{:.1}",
                            n.x, n.y, n.width, n.height
                        ),
                        None => eprintln!("  '{label}' MISSING"),
                    }
                }
                fn collect_triggers<'a>(
                    el: &'a rui::core::tree::Element,
                    out: &mut Vec<&'a rui::core::tree::Element>,
                ) {
                    if el.attrs.get("data-slot").map(|s| s.as_str()) == Some("tabs-trigger") {
                        out.push(el);
                    }
                    for c in &el.children {
                        collect_triggers(c, out);
                    }
                }
                let mut triggers = Vec::new();
                collect_triggers(&tree, &mut triggers);
                eprintln!("triggers={}", triggers.len());
                for t in &triggers {
                    eprintln!("  {:?} bounds={:?}", t.text, t.layout_bounds);
                }
                assert!(
                    triggers.len() >= 5,
                    "expected date/time triggers, got {}",
                    triggers.len()
                );
                let date = triggers
                    .iter()
                    .find(|t| t.text.as_deref() == Some("Date"))
                    .expect("Date");
                let dt = triggers
                    .iter()
                    .find(|t| t.text.as_deref() == Some("DateTime"))
                    .expect("DateTime");
                assert!(
                    date.layout_bounds[2] > 20.0,
                    "Date chip width crushed: {:?}",
                    date.layout_bounds
                );
                assert!(
                    (dt.layout_bounds[0] - date.layout_bounds[0]).abs() > 4.0
                        || (dt.layout_bounds[1] - date.layout_bounds[1]).abs() > 4.0,
                    "Date/DateTime piled: {:?} vs {:?}",
                    date.layout_bounds,
                    dt.layout_bounds
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn forms_tabs_chips_after_scroll_into_view_do_not_overlap() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree =
                    rui::core::reactivity::isolate_reads(|| gallery_section_body("forms"));
                let mut layout = rui::core::tree::LayoutTree::new();
                let _ = layout.build_and_layout(&mut tree, 900.0, 5000.0, [0.0, 0.0]);

                fn find_scroll<'a>(
                    el: &'a mut rui::core::tree::Element,
                ) -> Option<&'a mut rui::core::tree::Element> {
                    if el
                        .focus_id
                        .as_deref()
                        .is_some_and(|f| f.starts_with("scroll:"))
                    {
                        return Some(el);
                    }
                    for c in el.children.iter_mut() {
                        if let Some(hit) = find_scroll(c) {
                            return Some(hit);
                        }
                    }
                    None
                }
                let tabs_y = gallery_find_by_tid(&tree, "gal-dp-kinds")
                    .map(|el| el.layout_bounds[1])
                    .unwrap_or(0.0);
                let scroll = find_scroll(&mut tree).expect("forms ScrollArea viewport");
                // Bring the date/time TabsList into the 800px viewport regardless of
                // how many showcases sit above it.
                scroll.scroll_offset = [0.0, (tabs_y - 48.0).max(0.0)];

                let result =
                    layout.build_and_layout_fast(&mut tree, 900.0, 800.0, [0.0, 0.0], true, false);

                for label in ["Date", "DateTime", "Time", "Month", "Week"] {
                    match result.text_nodes.iter().find(|n| n.text == label) {
                        Some(n) => eprintln!(
                            "scrolled '{label}' @ ({:.1},{:.1}) {:.1}x{:.1}",
                            n.x, n.y, n.width, n.height
                        ),
                        None => eprintln!("scrolled '{label}' MISSING"),
                    }
                }
                let mut xs: Vec<(String, f32, f32)> = ["Date", "DateTime", "Time", "Month", "Week"]
                    .iter()
                    .filter_map(|lab| {
                        let n = result.text_nodes.iter().find(|n| n.text == *lab)?;
                        Some((lab.to_string(), n.x, n.width))
                    })
                    .collect();
                assert!(
                    xs.len() >= 3,
                    "after scroll, date/time chip labels must paint (got {xs:?})"
                );
                xs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                for w in xs.windows(2) {
                    let (la, xa, wa) = (&w[0].0, w[0].1, w[0].2);
                    let (lb, xb, _) = (&w[1].0, w[1].1, w[1].2);
                    assert!(
                        (xb - xa).abs() > 4.0,
                        "after scroll chips share x — '{la}'@x={xa}w={wa} vs '{lb}'@x={xb} (gallery pile-up)"
                    );
                    assert!(
                        xb + 0.5 >= xa + wa.min(8.0),
                        "after scroll chips overlap — '{la}'@x={xa}w={wa} vs '{lb}'@x={xb}"
                    );
                }
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn forms_date_time_tabs_chips_do_not_overlap() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree =
                    rui::core::reactivity::isolate_reads(|| gallery_section_body("forms"));
                let mut layout = rui::core::tree::LayoutTree::new();
                let result = layout.build_and_layout(&mut tree, 900.0, 5000.0, [0.0, 0.0]);
                eprintln!(
                    "forms layout: root_bounds={:?} text_nodes={} instances={}",
                    tree.layout_bounds,
                    result.text_nodes.len(),
                    result.instances.len()
                );
                for label in ["Date", "DateTime", "Sketch", "表单", "Mark invalid"] {
                    match result.text_nodes.iter().find(|n| n.text == label) {
                        Some(n) => eprintln!(
                            "  paint '{label}' @ ({:.1},{:.1}) size {:.1}x{:.1}",
                            n.x, n.y, n.width, n.height
                        ),
                        None => eprintln!("  paint '{label}' MISSING"),
                    }
                }

                fn collect_triggers<'a>(
                    el: &'a rui::core::tree::Element,
                    out: &mut Vec<&'a rui::core::tree::Element>,
                ) {
                    if el.attrs.get("data-slot").map(|s| s.as_str()) == Some("tabs-trigger") {
                        out.push(el);
                    }
                    for c in &el.children {
                        collect_triggers(c, out);
                    }
                }
                fn under_tabs<'a>(
                    el: &'a rui::core::tree::Element,
                    tabs_id: &str,
                ) -> Option<&'a rui::core::tree::Element> {
                    if el.template_id.as_deref() == Some(tabs_id) {
                        return Some(el);
                    }
                    el.children.iter().find_map(|c| under_tabs(c, tabs_id))
                }

                for tabs_id in ["gal-dp-kinds", "gal-cp-variants"] {
                    let tabs = under_tabs(&tree, tabs_id).expect(tabs_id);
                    eprintln!(
                        "{tabs_id} root bounds={:?} kids={}",
                        tabs.layout_bounds,
                        tabs.children.len()
                    );
                    for (i, c) in tabs.children.iter().enumerate() {
                        eprintln!(
                            "  child[{i}] slot={:?} tid={:?} bounds={:?} text={:?} kids={} display={:?}",
                            c.attrs.get("data-slot"),
                            c.template_id,
                            c.layout_bounds,
                            c.text,
                            c.children.len(),
                            c.style.display
                        );
                    }
                    let mut triggers = Vec::new();
                    collect_triggers(tabs, &mut triggers);
                    assert!(
                        triggers.len() >= 3,
                        "{tabs_id}: expected triggers, got {}",
                        triggers.len()
                    );
                    for t in &triggers {
                        eprintln!(
                            "{tabs_id} chip text={:?} bounds={:?}",
                            t.text, t.layout_bounds
                        );
                    }
                    // Prefer paint positions when element bounds were not stamped (0×0).
                    let mut paint_xs: Vec<(String, f32, f32)> = triggers
                        .iter()
                        .filter_map(|t| {
                            let label = t.text.as_deref()?;
                            let n = result.text_nodes.iter().find(|n| n.text == label)?;
                            Some((label.to_string(), n.x, n.width))
                        })
                        .collect();
                    assert!(
                        paint_xs.len() >= 2,
                        "{tabs_id}: missing painted chip labels (got {paint_xs:?})"
                    );
                    paint_xs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                    for w in paint_xs.windows(2) {
                        let (la, xa, wa) = (&w[0].0, w[0].1, w[0].2);
                        let (lb, xb, _wb) = (&w[1].0, w[1].1, w[1].2);
                        assert!(
                            (xb - xa).abs() > 4.0,
                            "{tabs_id}: painted chips share x — '{la}'@x={xa}w={wa} vs '{lb}'@x={xb}"
                        );
                        assert!(
                            xb + 0.5 >= xa + wa.min(8.0),
                            "{tabs_id}: painted chips overlap — '{la}'@x={xa}w={wa} vs '{lb}'@x={xb}"
                        );
                    }
                }
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn forms_date_time_pickers_tabs_mount_only_active() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let tree = rui::core::reactivity::isolate_reads(|| gallery_section_body("forms"));
                assert!(
                    gallery_tid_contains(&tree, "gal-dp-kinds"),
                    "forms must host Date/Time kinds under Tabs"
                );
                assert!(
                    gallery_tid_contains(&tree, "gal-dp-kinds:content:date"),
                    "default date tab content placeholder"
                );
                assert!(
                    !gallery_tid_contains(&tree, "gal-dt__dp_view"),
                    "inactive DateTime picker must not be an eager declarative sibling"
                );
                assert!(
                    !gallery_tid_contains(&tree, "gal-time__tp_view"),
                    "inactive TimePicker must not be an eager declarative sibling"
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn forms_password_input_adornments_visible_when_focused() {
    rui::app::reactive::reset_registered_anchors();
    let mut fs = rui::renderer::font_loader::create_font_system();
    rui::renderer::font_loader::apply_default_families(fs.db_mut());
    rui::text::install_shared_font_system(&mut fs);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let id = "gal-s2-input-pw";
            use_signal(&rui::components::input_value_signal_key(id), String::new)
                .set("secret1".into());
            use_signal("__rui_button_focus_id", String::new)
                .set(format!("{}{id}", rui::components::INPUT_FOCUS_PREFIX));

            let mut tree = rui::core::reactivity::isolate_reads(|| gallery_section_body("forms"));
            let pw = gallery_find_by_tid(&tree, id).expect("password input root");
            let clear = gallery_find_by_tid(pw, &format!("__rui_input:{id}:clear"))
                .expect("clear adornment must mount for clearable=\"true\"");
            let eye = gallery_find_by_tid(pw, &format!("__rui_input:{id}:reveal"))
                .expect("eye adornment must mount for password_toggle=\"true\"");
            assert!(
                (clear.style.opacity - 1.0).abs() < 0.001,
                "clear opacity={}, expected 1",
                clear.style.opacity
            );
            assert!(
                (eye.style.opacity - 1.0).abs() < 0.001,
                "eye opacity={}, expected 1",
                eye.style.opacity
            );

            let mut layout = rui::core::tree::LayoutTree::new();
            let result = layout.build_and_layout(&mut tree, 900.0, 800.0, [0.0, 0.0]);
            let lucide = result
                .text_nodes
                .iter()
                .filter(|n| n.style.font_family.as_deref() == Some("lucide"))
                .filter(|n| n.style.color_resolved()[3] > 0 && n.width > 0.5)
                .count();
            assert!(
                lucide >= 2,
                "focused password input must collect visible lucide adornments; got {lucide}"
            );
        });
    });
    rui::app::reactive::reset_registered_anchors();
}

#[test]
fn forms_section_mounts_wp_r7_reactive_chips() {
    rui::app::reactive::reset_registered_anchors();
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let tree = rui::core::reactivity::isolate_reads(|| {
                InputShowcase(InputShowcaseProps::default())
            });
            assert!(
                gallery_find_tid(&tree, "gal-status-badge__component"),
                "WP-R.7 ReactiveStatusBadge placeholder"
            );
            assert!(
                gallery_find_tid(&tree, "gal-count-chip__component"),
                "WP-R.7 ReactiveCountChip placeholder"
            );
            assert!(
                gallery_tid_contains(&tree, ":bind#")
                    || gallery_tid_contains(&tree, ":bind_icon_name#"),
                "InputShowcase under isolate_reads must keep auto bind leaves"
            );
            assert!(
                gallery_tid_contains(&tree, ":Show#"),
                "<Show when={{badge_ok.get()}}> must lower under isolate_reads"
            );
        });
    });
    rui::app::reactive::reset_registered_anchors();
}

#[test]
fn forms_section_mounts_wp39_submit() {
    rui::app::reactive::reset_registered_anchors();
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let tree = rui::core::reactivity::isolate_reads(|| {
                FormValidateShowcase(FormValidateShowcaseProps::default())
            });
            assert!(
                gallery_find_tid(&tree, "gal-wp39-submit"),
                "WP-3.9 card must mount submit (gal-wp39-submit)"
            );
        });
    });
    rui::app::reactive::reset_registered_anchors();
}

#[test]
fn data_section_mounts_wp310_masonry() {
    rui::app::reactive::reset_registered_anchors();
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let tree = rui::core::reactivity::isolate_reads(|| {
                VirtualMasonryShowcase(VirtualMasonryShowcaseProps::default())
            });
            assert!(
                gallery_find_tid(&tree, GAL_WP310_MASONRY),
                "WP-3.10 card must mount masonry (gal-wp310-masonry)"
            );
        });
    });
    rui::app::reactive::reset_registered_anchors();
}

fn set_scroll_offset(el: &mut rui::core::tree::Element, rest: &str, offset: [f32; 2]) -> bool {
    let want = format!("scroll:{rest}");
    let mut stack: Vec<*mut rui::core::tree::Element> = vec![el];
    while let Some(ptr) = stack.pop() {
        let node = unsafe { &mut *ptr };
        if node.focus_id.as_deref() == Some(want.as_str()) {
            node.scroll_offset = offset;
            return true;
        }
        let n = node.children.len();
        for i in (0..n).rev() {
            stack.push(std::ptr::addr_of_mut!(node.children[i]));
        }
    }
    false
}

fn scroll_metrics_xy(
    result: &rui::core::tree::RenderResult,
    rest: &str,
) -> ([f32; 4], [f32; 2], [f32; 2]) {
    result
        .scroll_metrics
        .iter()
        .find(|(id, ..)| id == rest)
        .map(|m| (m.1, m.2, m.3))
        .unwrap_or_else(|| panic!("missing scroll metrics for {rest}"))
}

#[test]
fn data_section_mounts_timeline() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let (_handle, tree) = create_root(|| gallery_section_body("data"));
        assert!(
            find_tid(&tree, "gal-timeline-builder__timeline"),
            "data section must mount Builder Timeline (gal-timeline-builder)"
        );
        assert!(
            gallery_tid_contains(&tree, "__timeline"),
            "data section must mount at least one Timeline shell"
        );
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn data_section_scroll_metrics_stable_across_offsets() {
    // Gallery data page: charts (overflow:visible ticks), nested Table/Tree/Masonry
    // scrollports, and CarouselStack. Outer `scroll-data` content size must not
    // breathe as cards enter/leave the clip — that jitters the page thumb.
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree =
                    rui::core::reactivity::isolate_reads(|| gallery_section_body("data"));
                let mut layout = rui::core::tree::LayoutTree::new();
                let first = layout.build_and_layout(&mut tree, 900.0, 600.0, [-1.0, -1.0]);
                let (vp0, max0, content0) = scroll_metrics_xy(&first, "scroll-data");
                assert!(
                    content0[1] > vp0[3] + 8.0 && max0[1] > 8.0,
                    "data section must overflow the 600px viewport (content_h={} vp_h={} max_y={})",
                    content0[1],
                    vp0[3],
                    max0[1]
                );

                let samples: Vec<f32> = {
                    let step = (max0[1] / 8.0).max(40.0);
                    let mut v = vec![0.0_f32];
                    let mut y = step;
                    while y < max0[1] {
                        v.push(y);
                        y += step;
                    }
                    v.push(max0[1]);
                    v
                };

                for &sy in &samples {
                    assert!(
                        set_scroll_offset(&mut tree, "scroll-data", [0.0, sy]),
                        "scroll:scroll-data viewport must exist"
                    );
                    let fast = layout.build_and_layout_fast(
                        &mut tree,
                        900.0,
                        600.0,
                        [-1.0, -1.0],
                        true,
                        false,
                    );
                    let (vp, max, content) = scroll_metrics_xy(&fast, "scroll-data");
                    assert!(
                        (content[1] - content0[1]).abs() < 1.0
                            && (content[0] - content0[0]).abs() < 1.0
                            && (vp[2] - vp0[2]).abs() < 1.0
                            && (vp[3] - vp0[3]).abs() < 1.0
                            && (max[1] - max0[1]).abs() < 1.0
                            && (max[0] - max0[0]).abs() < 1.0,
                        "scroll_only metrics jitter at sy={sy}: \
                         base content={content0:?} vp={vp0:?} max={max0:?}; \
                         now content={content:?} vp={vp:?} max={max:?}"
                    );
                }

                assert!(
                    set_scroll_offset(&mut tree, "scroll-data", [0.0, max0[1]]),
                    "scroll:scroll-data viewport must exist"
                );
                let full = layout.build_and_layout(&mut tree, 900.0, 600.0, [-1.0, -1.0]);
                let (vp, max, content) = scroll_metrics_xy(&full, "scroll-data");
                assert!(
                    (content[1] - content0[1]).abs() < 1.0
                        && (content[0] - content0[0]).abs() < 1.0
                        && (max[1] - max0[1]).abs() < 1.0,
                    "full collect at bottom must keep metrics: \
                     base content={content0:?} max={max0:?}; now content={content:?} max={max:?} vp={vp:?}"
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn forms_input_showcase_mark_invalid_button_has_caption() {
    rui::app::reactive::reset_registered_anchors();
    let mut fs = rui::renderer::font_loader::create_font_system();
    rui::renderer::font_loader::apply_default_families(fs.db_mut());
    rui::text::install_shared_font_system(&mut fs);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut tree = InputShowcase(InputShowcaseProps::default());
            fn find_caption<'a>(
                el: &'a mut rui::core::tree::Element,
            ) -> Option<&'a mut rui::core::tree::Element> {
                if el.text.as_deref() == Some("Mark invalid") {
                    return Some(el);
                }
                for child in el.children.iter_mut() {
                    if let Some(found) = find_caption(child) {
                        return Some(found);
                    }
                }
                None
            }
            let caption = find_caption(&mut tree).expect("caption node");
            let color = caption.text_style.color_resolved();
            assert!(
                color[3] > 0,
                "caption alpha must be visible, got {:?}",
                color
            );
            let mut layout = rui::core::tree::LayoutTree::new();
            let result = layout.build_and_layout(&mut tree, 900.0, 900.0, [0.0, 0.0]);
            let node = result
                .text_nodes
                .iter()
                .find(|n| n.text == "Mark invalid")
                .expect("layout must collect Mark invalid");
            assert!(
                node.width > 4.0 && node.height > 4.0,
                "caption box must be visible, got {}x{} at ({},{})",
                node.width,
                node.height,
                node.x,
                node.y
            );
            assert!(
                node.x >= 0.0 && node.y >= 0.0 && node.x < 900.0 && node.y < 900.0,
                "caption must be on-screen, got ({},{})",
                node.x,
                node.y
            );
        });
    });
    rui::app::reactive::reset_registered_anchors();
}

#[test]
fn headless_mount_gallery_app_does_not_overflow() {
    rui::app::reactive::reset_registered_anchors();
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let tree = build_gallery_app();
            assert!(
                !tree.children.is_empty(),
                "gallery root should have children"
            );
        });
    });
}

#[test]
fn button_showcase_format_get_lowers_to_bind_text() {
    // Section pages build under `isolate_reads`; without bind_text lowering,
    // `format!(…, count.get())` would bake a static label and never update.
    rui::app::reactive::reset_registered_anchors();
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let tree = ButtonShowcase(ButtonShowcaseProps::default());
            fn find_bind(el: &rui::core::tree::Element) -> bool {
                if el
                    .template_id
                    .as_deref()
                    .is_some_and(|id| id.contains(":bind#"))
                    && el.text.as_deref().is_some_and(|t| {
                        t.starts_with("Default (") || t.starts_with("Selected variant:")
                    })
                {
                    return true;
                }
                el.children.iter().any(find_bind)
            }
            assert!(
                find_bind(&tree),
                "ButtonShowcase must auto-lower format!(…, signal.get()) to bind_text"
            );
        });
    });
    rui::app::reactive::reset_registered_anchors();
}

#[test]
fn forms_section_does_not_crush_sidebar_width() {
    rui::app::reactive::reset_registered_anchors();
    let mut fs = rui::renderer::font_loader::create_font_system();
    rui::renderer::font_loader::apply_default_families(fs.db_mut());
    rui::text::install_shared_font_system(&mut fs);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            use_signal("gal_nav_section", || "basics".to_string()).set("forms".to_string());
            let mut app = build_gallery_app();
            let mut layout = rui::core::tree::LayoutTree::new();
            let _ = layout.build_and_layout(&mut app, 720.0, 700.0, [-1.0, -1.0]);
            let side = gallery_find_by_tid(&app, "gal-sidebar").expect("gal-sidebar");
            let w = side.layout_bounds[2];
            assert!(
                (w - 256.0).abs() < 1.5,
                "forms page must not crush sidebar below 16rem; got {w}"
            );
        });
    });
    rui::app::reactive::reset_registered_anchors();
}

#[test]
fn overlays_section_effect_flush_completes() {
    // Nested `reactive_anchor` / `<Show when={…get()}>` on this page used to
    // miss `try_reconcile_subtree` (no AppState) and `schedule_effect` into the
    // in-flight deferred flush → `flush_deferred_effects exceeded 100k rounds`.
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let _ = rui::core::reactivity::isolate_reads(|| {
                    GallerySectionOverlays(GallerySectionOverlaysProps::default())
                });
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn overlays_revisit_does_not_shrink_main_pane() {
    // <Portal> on the overlays page was hoisted to the root HStack each visit without
    // cleaning prior lifts → accumulating w/h 100% flex siblings crushed the ScrollArea.
    // Build the section page (same tree as the live `<When>` arm), not
    // `build_gallery_app()`: the latter returns a `reactive_view` placeholder, so
    // `scroll-overlays` is not in the local tree.
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut widths = Vec::new();
                for section in ["overlays", "basics", "overlays", "basics", "overlays"] {
                    rui::app::reactive::reset_registered_anchors();
                    let mut app =
                        rui::core::reactivity::isolate_reads(|| gallery_section_body(section));
                    let _ = rui::layout::layout_tree::hoist_portal_subtrees(&mut app);
                    let mut layout = rui::core::tree::LayoutTree::new();
                    let _ = layout.build_and_layout(&mut app, 1000.0, 700.0, [-1.0, -1.0]);
                    let pane = gallery_find_by_tid(&app, &format!("scroll-{section}"))
                        .expect("scroll pane");
                    widths.push((section, pane.layout_bounds[2]));
                }
                let overlay_widths: Vec<f32> = widths
                    .iter()
                    .filter(|(s, _)| *s == "overlays")
                    .map(|(_, w)| *w)
                    .collect();
                assert!(
                    overlay_widths.len() == 3
                        && overlay_widths.iter().all(|w| *w > 700.0)
                        && (overlay_widths[0] - overlay_widths[2]).abs() < 2.0,
                    "overlays main width must stay wide across revisits; got {widths:?}"
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn gallery_main_pane_uses_remaining_width() {
    // toast_host must not flex-split the main column (reactive_placeholder is w/h 100%).
    rui::app::reactive::reset_registered_anchors();
    let mut fs = rui::renderer::font_loader::create_font_system();
    rui::renderer::font_loader::apply_default_families(fs.db_mut());
    rui::text::install_shared_font_system(&mut fs);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            use_signal("gal_nav_section", || "basics".to_string());
            let mut app = build_gallery_app();
            let mut layout = rui::core::tree::LayoutTree::new();
            let _ = layout.build_and_layout(&mut app, 1000.0, 700.0, [-1.0, -1.0]);
            let side = gallery_find_by_tid(&app, "gal-sidebar").expect("sidebar");
            let inset = gallery_find_slot(&app, "sidebar-inset").expect("sidebar-inset");
            let side_w = side.layout_bounds[2];
            let inset_w = inset.layout_bounds[2];
            assert!((side_w - 256.0).abs() < 1.5, "sidebar width, got {side_w}");
            assert!(
                inset_w > 640.0,
                "SidebarInset must flex-grow into remaining inset shell width; got {inset_w}"
            );
            assert!(
                side_w + inset_w < 1000.0,
                "inset margins/padding must not overflow root; got {}+{}",
                side_w,
                inset_w
            );
        });
    });
    rui::app::reactive::reset_registered_anchors();
}

#[test]
fn gallery_sidebar_collapses_to_icon_rail() {
    rui::app::reactive::reset_registered_anchors();
    let mut fs = rui::renderer::font_loader::create_font_system();
    rui::renderer::font_loader::apply_default_families(fs.db_mut());
    rui::text::install_shared_font_system(&mut fs);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            use_signal("gal_nav_section", || "basics".to_string());
            use_signal("gal-shell__open", || false);
            let mut app = build_gallery_app();
            let mut layout = rui::core::tree::LayoutTree::new();
            let _ = layout.build_and_layout(&mut app, 1000.0, 700.0, [-1.0, -1.0]);
            let side = gallery_find_by_tid(&app, "gal-sidebar").expect("sidebar");
            let w = side.layout_bounds[2];
            assert!(
                (w - 48.0).abs() < 1.5,
                "SidebarTrigger collapse must keep a 3rem icon rail; got {w}"
            );
        });
    });
    rui::app::reactive::reset_registered_anchors();
}

#[test]
fn basics_section_scrolls_when_viewport_shorter() {
    // Basics section content (~800px) must enable Y scroll when the ScrollArea
    // viewport is shorter — same failure mode as gallery after a window-height shrink
    // once surface size tracks Resized (see AppState::resolve_surface_size).
    run_on_gallery_build_stack(|| {
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);

        rui::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut body = gallery_section_body("basics");
                let mut layout = rui::core::tree::LayoutTree::new();
                let tall = layout.build_and_layout(&mut body, 600.0, 2000.0, [-1.0, -1.0]);
                let (_, _tall_vp, _, tall_content) = tall
                    .scroll_metrics
                    .iter()
                    .find(|(id, ..)| id == "scroll-basics")
                    .expect("scroll-basics metrics");
                assert!(
                    tall_content[1] > 600.0,
                    "basics inner column must be taller than a short window (content_h={})",
                    tall_content[1]
                );

                let short = layout.build_and_layout(&mut body, 600.0, 500.0, [-1.0, -1.0]);
                let (_, short_vp, short_max, short_content) = short
                    .scroll_metrics
                    .iter()
                    .find(|(id, ..)| id == "scroll-basics")
                    .expect("scroll-basics metrics");
                assert!(
                    short_content[1] > short_vp[3] + 0.5 && short_max[1] > 1.0,
                    "at 500px viewport, basics must scroll (content_h={} vp_h={} max_y={})",
                    short_content[1],
                    short_vp[3],
                    short_max[1]
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn headless_layout_basics_section_does_not_overflow() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree = gallery_section_body("basics");
                let mut layout = rui::core::tree::LayoutTree::new();
                let _ = layout.build_and_layout(&mut tree, 1280.0, 800.0, [0.0, 0.0]);
            });
        });
    });
}

/// After `Element` heap handles: basics construction + layout must fit the PE
/// default 1 MiB thread **without** `maybe_grow`.
#[test]
fn headless_layout_basics_fits_pe_stack_without_grow() {
    run_on_pe_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree = gallery_section_body("basics");
                let mut layout = rui::core::tree::LayoutTree::new();
                let _ = layout.build_and_layout(&mut tree, 1280.0, 800.0, [0.0, 0.0]);
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

/// Windows PE default for the GUI main thread.
const WINDOWS_PE_STACK: usize = 1024 * 1024;

fn run_on_stack<F>(bytes: usize, f: F)
where
    F: FnOnce() + Send + 'static,
{
    std::thread::Builder::new()
        .stack_size(bytes)
        .spawn(f)
        .expect("spawn gallery stack probe")
        .join()
        .expect("gallery stack probe");
}

fn run_on_pe_stack<F>(f: F)
where
    F: FnOnce() + Send + 'static,
{
    run_on_stack(WINDOWS_PE_STACK, f);
}

/// 1 MiB thread (Windows PE default) + framework [`rui::app::with_ui_build_stack`].
fn run_on_gallery_build_stack<F>(f: F)
where
    F: FnOnce() + Send + 'static,
{
    run_on_pe_stack(|| rui::app::with_ui_build_stack(f));
}

#[test]
fn headless_layout_all_sections_does_not_overflow() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                for key in [
                    "basics",
                    "forms",
                    "overlays",
                    "data",
                    "navigation",
                    "layout",
                    "text",
                    "theme",
                    "about",
                ] {
                    let mut tree = gallery_section_body(key);
                    let mut layout = rui::core::tree::LayoutTree::new();
                    let _ = layout.build_and_layout(&mut tree, 1280.0, 800.0, [0.0, 0.0]);
                }
            });
        });
    });
}

fn find_tid(el: &rui::core::tree::Element, want: &str) -> bool {
    gallery_find_tid(el, want)
}

fn gallery_find_attr(el: &rui::core::tree::Element, key: &str, value: &str) -> bool {
    let mut stack = vec![el];
    while let Some(el) = stack.pop() {
        if el.attrs.get(key).map(String::as_str) == Some(value) {
            return true;
        }
        stack.extend(el.children.iter().rev());
    }
    false
}

#[test]
fn theme_section_mounts_controls() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let (_handle, tree) = create_root(|| gallery_section_body("theme"));
        assert!(
            find_tid(&tree, "gal-theme-apply")
                || gallery_find_attr(&tree, "id", "gal-theme-apply"),
            "theme section must mount apply theme.toml"
        );
        assert!(
            find_tid(&tree, "gal-theme-sheet")
                || gallery_find_attr(&tree, "id", "gal-theme-sheet"),
            "theme section must mount apply styles.toml"
        );
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn text_section_mounts_msdf_card() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let (_handle, tree) = create_root(|| gallery_section_body("text"));
        assert!(
            find_tid(&tree, "gal-msdf-coverage"),
            "text section must mount Coverage sample"
        );
        assert!(
            find_tid(&tree, "gal-msdf-bake"),
            "text section must mount Bake sample"
        );
        assert!(
            find_tid(&tree, "gal-msdf-msdf"),
            "text section must mount three-channel MSDF sample"
        );
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn draw_section_mounts_billboard() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let (_handle, tree) = create_root(|| gallery_section_body("draw"));
        assert!(
            find_tid(&tree, "gal-billboard-default"),
            "draw section must mount default-pitch billboard"
        );
        assert!(
            find_tid(&tree, "gal-billboard-pitch"),
            "draw section must mount steep-pitch billboard"
        );
        assert!(
            gallery_find_attr(&tree, "rui_scene3d_label", "永角"),
            "steep billboard must carry CJK MSDF label"
        );
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn navigation_section_mounts_shell_router_view() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let (_handle, tree) = create_root(|| gallery_section_body("navigation"));
        assert!(
            find_tid(&tree, "gal_shell_router__router_view"),
            "WP-3.5: navigation section must mount Router.view() placeholder"
        );
        assert!(
            find_tid(&tree, GAL_SHELL_TRAY_SHOW),
            "WP-3.6: navigation section must mount tray Show button"
        );
        assert!(
            find_tid(&tree, GAL_SHELL_FRAMELESS),
            "navigation section must mount frameless window button"
        );
        assert!(
            find_tid(&tree, GAL_SHELL_TRANSPARENT),
            "navigation section must mount transparent window button"
        );
        assert!(
            find_tid(&tree, GAL_SHELL_BLUR),
            "navigation section must mount blur window button"
        );
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn full_app_mount_and_layout_fits_pe_stack() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut app = build_gallery_app();
                let mut layout = rui::core::tree::LayoutTree::new();
                let _ = layout.build_and_layout(&mut app, 1280.0, 800.0, [0.0, 0.0]);
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

/// Full gallery shell + default section: 1 MiB PE thread, no `maybe_grow`.
#[test]
fn full_app_mount_and_layout_fits_pe_stack_without_grow() {
    run_on_pe_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut app = build_gallery_app();
                let mut layout = rui::core::tree::LayoutTree::new();
                let _ = layout.build_and_layout(&mut app, 1280.0, 800.0, [0.0, 0.0]);
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn button_showcase_fits_pe_stack() {
    run_on_pe_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree = ButtonShowcase(Default::default());
                let mut layout = rui::core::tree::LayoutTree::new();
                let _ = layout.build_and_layout(&mut tree, 1280.0, 800.0, [0.0, 0.0]);
            });
        });
    });
}

fn rects_intersect(a: [f32; 4], b: [f32; 4]) -> bool {
    let x0 = a[0].max(b[0]);
    let y0 = a[1].max(b[1]);
    let x1 = (a[0] + a[2]).min(b[0] + b[2]);
    let y1 = (a[1] + a[3]).min(b[1] + b[3]);
    x1 > x0 && y1 > y0
}

/// Real Gallery overlays page (not the crate_tests host stub): band assignment +
/// opaque prefix cover over YELLOW. Tall viewport so the local scrim is on-screen
/// without scrolling (same as the user having scrolled it into view).
#[test]
fn overlays_local_scrim_yellow_is_in_capture_prefix() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut root = rui::core::reactivity::isolate_reads(|| {
                    GallerySectionOverlays(GallerySectionOverlaysProps::default())
                });
                assert!(
                    rui::layout::layout_tree::hoist_portal_subtrees(&mut root) >= 1,
                    "local Portal must hoist onto gal-local-scrim-host"
                );
                let mut layout = rui::core::tree::LayoutTree::new();
                let result = layout.build_and_layout(&mut root, 1280.0, 1800.0, [-1.0, -1.0]);
                let overlay_text_start =
                    result.overlay_split.text_start.min(result.text_nodes.len());
                let bands = result.overlay_split.page_stack_band_ranges();
                let yellow_at = result
                    .text_nodes
                    .iter()
                    .position(|t| t.text.contains("YELLOW"));
                let blur_indices: Vec<usize> = result
                    .instances
                    .iter()
                    .enumerate()
                    .filter(|(_, i)| i.backdrop_blur > 0.5)
                    .map(|(i, _)| i)
                    .collect();
                let mut dump = format!(
                    "overlay_text_start={}/{} bands={} blur_inst={:?} yellow={:?} paint_ops={}\n",
                    overlay_text_start,
                    result.text_nodes.len(),
                    bands.len(),
                    blur_indices,
                    yellow_at,
                    result.paint_ops.len(),
                );
                if let Some(yi) = yellow_at {
                    let y = &result.text_nodes[yi];
                    dump.push_str(&format!(
                        "YELLOW xy=({:.1},{:.1}) {:.1}x{:.1} clip={:?}\n",
                        y.x, y.y, y.width, y.height, y.clip_bounds
                    ));
                }
                for (i, b) in bands.iter().enumerate() {
                    let lo = b.text.0.min(overlay_text_start);
                    let hi = b.text.1.min(overlay_text_start);
                    dump.push_str(&format!(
                        "  band[{i}] inst={:?} tex={:?} text={:?} paint={:?} prepare={lo}..{hi} blur={} yellow={}\n",
                        b.instance,
                        b.textured,
                        b.text,
                        b.paint_ops,
                        blur_indices.iter().any(|&p| b.instance.0 <= p && p < b.instance.1),
                        yellow_at.is_some_and(|p| lo <= p && p < hi),
                    ));
                }
                dump.push_str("prefix cover over YELLOW:\n");
                if let Some(yi) = yellow_at {
                    let y = &result.text_nodes[yi];
                    let yrect = [y.x, y.y, y.width.max(1.0), y.height.max(1.0)];
                    let yband = bands.iter().position(|b| b.text.0 <= yi && yi < b.text.1);
                    let local_blur_band = blur_indices.iter().find_map(|&p| {
                        bands
                            .iter()
                            .position(|b| b.instance.0 <= p && p < b.instance.1)
                    });
                    for (bi, b) in bands.iter().enumerate() {
                        if yband.is_some_and(|yb| bi <= yb) {
                            continue;
                        }
                        if local_blur_band.is_some_and(|bb| bi >= bb) {
                            continue;
                        }
                        for ii in b.instance.0..b.instance.1.min(result.instances.len()) {
                            let inst = &result.instances[ii];
                            if inst.color[3] <= 0.05 {
                                continue;
                            }
                            let irect = [
                                inst.rect_pos[0],
                                inst.rect_pos[1],
                                inst.rect_size[0],
                                inst.rect_size[1],
                            ];
                            if rects_intersect(yrect, irect) {
                                dump.push_str(&format!(
                                    "    band{bi} inst{ii} pos={:?} size={:?} a={:.3} blur={}\n",
                                    inst.rect_pos,
                                    inst.rect_size,
                                    inst.color[3],
                                    inst.backdrop_blur
                                ));
                            }
                        }
                    }
                    let dump_path = std::env::temp_dir().join("gallery_local_scrim_bands.txt");
                    let _ = std::fs::write(&dump_path, &dump);
                    let yellow_in_page = yi < overlay_text_start;
                    let text_band = yband;
                    let blur_band = local_blur_band;
                    let in_prefix = match (text_band, blur_band) {
                        (Some(t), Some(b)) => t < b,
                        _ => false,
                    };
                    assert!(
                        yellow_in_page && in_prefix,
                        "YELLOW not in local-scrim capture prefix (text_band={text_band:?} blur_band={blur_band:?} overlay_start={overlay_text_start})\n{dump}"
                    );
                } else {
                    let dump_path = std::env::temp_dir().join("gallery_local_scrim_bands.txt");
                    let _ = std::fs::write(&dump_path, &dump);
                    panic!("YELLOW label missing from GallerySectionOverlays collect\n{dump}");
                }
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}
