use super::*;
use crate::app::*;
use crate::imports::*;
use crate::pages::*;
use crate::shared::*;
use rui::core::reactivity::{create_effect, create_root};

#[test]
fn anchored_overlay_pinned_attribute_matches_builder_dismiss_behavior() {
    run_on_gallery_build_stack(|| {
        for builder in [false, true] {
            for pinned in [false, true] {
                rui::app::portal_overlay::clear_overlay_stack();
                rui::app::reactive::reset_registered_anchors();
                let (owner, _) = create_root(|| {
                    let open = Signal::new(true);
                    let pos = Signal::new([120.0, 80.0]);
                    let kept = std::rc::Rc::new(std::cell::RefCell::new(None));
                    let kept_effect = kept.clone();
                    create_effect(move || {
                        let tree = if builder {
                            rui::utils::anchored_overlay::anchored_overlay("pinned-builder-check")
                                .open(open)
                                .pinned(pinned)
                                .placement(rui::utils::anchored_overlay::Placement::point(pos, 0.0))
                                .child(|| Element::new().w(100.0).h(60.0))
                        } else {
                            ui! {
                                <AnchoredOverlay id="pinned-rsx-check" openSignal={open}>
                                    <AnchoredOverlayContent placement="point" pos={pos} pinned={pinned}>
                                        <VStack w="100px" h="60px" />
                                    </AnchoredOverlayContent>
                                </AnchoredOverlay>
                            }
                        };
                        *kept_effect.borrow_mut() = Some(tree);
                    });
                    rui::utils::popup::arm_underlay_close();
                    assert_eq!(
                        rui::app::portal_overlay::try_dismiss_outside_press(),
                        !pinned,
                        "builder={builder}, pinned={pinned}"
                    );
                    assert_eq!(open.peek(), pinned);
                    open.set(false);
                    assert!(!open.peek(), "author close must work for a resident panel");
                });
                owner.dispose();
            }
        }
        rui::app::portal_overlay::clear_overlay_stack();
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn generator_motion_tracks_have_independent_binding_targets() {
    run_on_gallery_build_stack(|| {
        let (_owner, _) = create_root(|| {
            create_effect(|| {
                let root = GeneratorShowcase(GeneratorShowcaseProps::default());
                let mut stack = vec![&root];
                let mut targets = std::collections::HashMap::new();
                while let Some(el) = stack.pop() {
                    if let Some(id) = el.template_id.as_deref() {
                        if id.starts_with("gal_gen_tx:") {
                            *targets.entry(id.to_string()).or_insert(0) += 1;
                        }
                    }
                    stack.extend(el.children.iter());
                }
                for track in ["spring", "follow", "inert", "wave", "seq"] {
                    assert_eq!(
                        targets.get(&format!("gal_gen_tx:{track}")),
                        Some(&1),
                        "each driver must patch its own mounted ball; targets={targets:?}"
                    );
                }
            });
        });
    });
}

#[test]
fn generator_go_reset_caption_keeps_its_authored_width() {
    run_on_gallery_build_stack(|| {
        create_root(|| {
            create_effect(|| {
                let root = GeneratorShowcase(GeneratorShowcaseProps::default());
                let label = gallery_find_by_tid(&root, "gal_gen_css_go_lbl").unwrap();
                assert!(
                    !label.style.size.width.is_auto(),
                    "Button caption conversion must retain fixed-size binding"
                );
            });
        });
    });
}

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

fn find_mouse_log_node(el: &rui::core::tree::Element) -> Option<&rui::core::tree::Element> {
    let mut stack = vec![el];
    while let Some(el) = stack.pop() {
        if el
            .text
            .as_deref()
            .is_some_and(|t| t.contains("client=") && t.contains("movement="))
        {
            return Some(el);
        }
        stack.extend(el.children.iter().rev());
    }
    None
}

fn gallery_find_by_focus_id<'a>(
    el: &'a rui::core::tree::Element,
    want: &str,
) -> Option<&'a rui::core::tree::Element> {
    let mut stack = vec![el];
    while let Some(el) = stack.pop() {
        if el.focus_id.as_deref() == Some(want) {
            return Some(el);
        }
        stack.extend(el.children.iter().rev());
    }
    None
}

fn rgb_dist(a: [f32; 4], b: [f32; 4]) -> f32 {
    let d0 = a[0] - b[0];
    let d1 = a[1] - b[1];
    let d2 = a[2] - b[2];
    (d0 * d0 + d1 * d1 + d2 * d2).sqrt()
}

fn color_close(inst: [f32; 4], token: [f32; 4]) -> bool {
    inst[3] > 0.5 && rgb_dist(inst, token) < 0.08
}

fn subtree_has_bg(el: &rui::core::tree::Element, token: [f32; 4]) -> bool {
    color_close(el.style.bg_color, token) || el.children.iter().any(|c| subtree_has_bg(c, token))
}

fn find_lca_click_caption(el: &rui::core::tree::Element) -> Option<&rui::core::tree::Element> {
    let mut stack = vec![el];
    while let Some(el) = stack.pop() {
        if el
            .text
            .as_deref()
            .is_some_and(|t| t.contains("目标 click=") && t.contains("拖进蓝色块"))
        {
            return Some(el);
        }
        stack.extend(el.children.iter().rev());
    }
    None
}

#[test]
fn mouse_event_lca_target_is_info_blue_on_muted_pad() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        rui::style::set_theme_mode(rui::style::ThemeMode::Light);
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let tree = rui::core::reactivity::isolate_reads(|| {
                    MouseEventShowcase(MouseEventShowcaseProps::default())
                });
                let outer =
                    gallery_find_by_tid(&tree, "gal-mouse-lca-outer").expect("gal-mouse-lca-outer");
                let inner = gallery_find_by_focus_id(&tree, "gal-mouse-lca-target")
                    .expect("gal-mouse-lca-target");
                let muted = rui::style::parse_color("muted");
                let info = rui::style::parse_color("info");
                let primary = rui::style::parse_color("primary");
                assert!(
                    info[2] > info[0] + 0.15,
                    "info must be theme blue, got {info:?}"
                );
                assert!(
                    primary[0] + primary[1] + primary[2] < 0.2,
                    "light primary is black — must not be the LCA target fill"
                );
                assert!(
                    rgb_dist(muted, info) > 0.12,
                    "copy says gray pad vs blue target; muted vs info must be distinct"
                );
                assert!(
                    color_close(outer.style.bg_color, muted),
                    "LCA pad must be muted gray, got {:?}",
                    outer.style.bg_color
                );
                assert!(
                    subtree_has_bg(inner, info),
                    "LCA target must paint info (theme blue), not primary/secondary"
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn mouse_event_lca_click_caption_is_plain_definite_width_leaf() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        rui::style::set_theme_mode(rui::style::ThemeMode::Light);
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
                let caption = find_lca_click_caption(&tree).expect("lca click caption");
                assert!(
                    !caption.style.size.width.is_auto(),
                    "w=100% caption must not shrink-wrap, or every click cosmic-prepares"
                );
                assert!(
                    caption.children.is_empty() && caption.text_spans.is_none(),
                    "caption must be a plain text leaf so bind_text can stay paint-only"
                );
                assert!(
                    caption.layout_bounds[2] > 100.0,
                    "caption must already have a definite used box, got {:?}",
                    caption.layout_bounds
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn mouse_event_swallow_hit_path_excludes_pad() {
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
                let swallow = gallery_find_by_focus_id(&tree, "gal-mouse-swallow")
                    .expect("gal-mouse-swallow");
                let parent = gallery_find_by_focus_id(&tree, "gal-mouse-swallow-parent")
                    .expect("gal-mouse-swallow-parent");
                assert!(
                    swallow.on_mousedown.is_some() && swallow.on_mousemove.is_none(),
                    "inner block must stop mousedown only — not rewrite the hit-pad log"
                );
                assert!(
                    parent.on_mousedown.is_some(),
                    "stopPropagation demo needs a real parent listener"
                );
                let b = swallow.layout_bounds;
                assert!(
                    b[2] > 8.0 && b[3] > 8.0,
                    "swallow must have a hittable box, got {b:?}"
                );
                let path =
                    rui::app::pointer_events::hit_path(&tree, b[0] + b[2] * 0.5, b[1] + b[3] * 0.5);
                let ids: Vec<&str> = path.iter().filter_map(|e| e.focus_id.as_deref()).collect();
                assert!(
                    ids.contains(&"gal-mouse-swallow"),
                    "hit path must include swallow, got {ids:?}"
                );
                assert!(
                    ids.contains(&"gal-mouse-swallow-parent"),
                    "hit path must include swallow parent, got {ids:?}"
                );
                assert!(
                    !ids.contains(&"gal-mouse-pad"),
                    "swallow is not a child of the hit pad; writing the pad log from here dest-resets the readout: {ids:?}"
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn mouse_event_pad_hit_path_includes_pad_under_hint_label() {
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
                let pad = gallery_find_by_focus_id(&tree, "gal-mouse-pad").expect("gal-mouse-pad");
                let b = pad.layout_bounds;
                assert!(
                    b[2] > 8.0 && b[3] > 8.0,
                    "hit pad must have a box, got {b:?}"
                );
                let x = b[0] + b[2] * 0.5;
                let y = b[1] + b[3] * 0.5;
                let path = rui::app::pointer_events::hit_path(&tree, x, y);
                let ids: Vec<&str> = path.iter().filter_map(|e| e.focus_id.as_deref()).collect();
                assert!(
                    ids.contains(&"gal-mouse-pad"),
                    "sliding the pad (including the hint label) must hit gal-mouse-pad, got {ids:?}"
                );
                assert!(
                    path.iter().any(|e| e.on_mousemove.is_some()),
                    "hit path must carry the pad on_mousemove, ids={ids:?}"
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn mouse_event_lca_center_click_increments_count() {
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
                let inner = gallery_find_by_focus_id(&tree, "gal-mouse-lca-target")
                    .expect("gal-mouse-lca-target");
                let b = inner.layout_bounds;
                assert!(
                    b[2] > 8.0 && b[3] > 8.0,
                    "LCA target must have a box, got {b:?}"
                );
                let x = b[0] + b[2] * 0.5;
                let y = b[1] + b[3] * 0.5;
                let path = rui::app::pointer_events::hit_path(&tree, x, y);
                let ids: Vec<&str> = path.iter().filter_map(|e| e.focus_id.as_deref()).collect();
                assert!(
                    ids.contains(&"gal-mouse-lca-target"),
                    "clicking the blue target (including its caption) must hit gal-mouse-lca-target, got {ids:?}"
                );
                let click_path = rui::app::pointer_events::click_dispatch_path(&path, &path);
                let (cb, _) = rui::app::pointer_events::resolve_click_from_path(&tree, &click_path);
                (*cb.expect("LCA on_click on press/release path").borrow_mut())();
                let lca = use_signal("gal_mouse_lca_clicks", || 0i32);
                assert_eq!(
                    lca.peek(),
                    1,
                    "clicking the LCA target center must increment click count"
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn mouse_event_log_is_plain_definite_width_leaf() {
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
                let log = find_mouse_log_node(&tree).expect("mouse log text node");
                assert!(
                    !log.style.size.width.is_auto(),
                    "w=100% log must not shrink-wrap, or every mousemove cosmic-prepares"
                );
                assert!(
                    log.children.is_empty() && log.text_spans.is_none(),
                    "log must be a plain text leaf so bind_text can stay paint-only"
                );
                assert!(
                    log.layout_bounds[2] > 100.0 && log.layout_bounds[3] > 50.0,
                    "log must already have a definite used box, got {:?}",
                    log.layout_bounds
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
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

                let tabs_y = gallery_find_by_tid(&tree, "gal-dp-kinds")
                    .map(|el| el.layout_bounds[1])
                    .expect("gal-dp-kinds");
                let scroll_y = (tabs_y - 48.0).max(0.0);
                // Collect reads `scroll:` offsets from live signals, not the Element
                // field. Writing only `scroll_offset` leaves the chips below the 800px
                // clip and they drop out of `text_nodes`.
                assert!(
                    set_scroll_offset(&mut tree, "scroll-forms", [0.0, scroll_y]),
                    "scroll:scroll-forms viewport must exist"
                );
                use_signal(
                    &rui::core::scroll_viewport::scroll_offset_y_signal_key("scroll-forms"),
                    || 0.0_f32,
                )
                .set_silent(scroll_y);

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
                    !gallery_tid_contains(&tree, "gal-time__dp_view"),
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
            let pw = gallery_find_by_tid(&tree, id).unwrap();
            for suffix in ["clear", "reveal"] {
                let button =
                    gallery_find_by_tid(pw, &format!("__rui_input:{id}:{suffix}")).unwrap();
                let [x, y, w, h] = button.layout_bounds;
                let inside = |rect: [f32; 4]| {
                    rect[2] > 0.5
                        && rect[3] > 0.5
                        && rect[0] >= x - 0.5
                        && rect[1] >= y - 0.5
                        && rect[0] + rect[2] <= x + w + 0.5
                        && rect[1] + rect[3] <= y + h + 0.5
                };
                // Named Lucide icons use SVG textures when svg is enabled;
                // glyphon is the fallback. Check each actual adornment's paint.
                let glyph = result.text_nodes.iter().any(|n| {
                    n.style.font_family.as_deref() == Some("lucide")
                        && n.style.color_resolved()[3] > 0
                        && inside([n.x, n.y, n.width, n.height])
                });
                let texture = result.textured_draws.iter().any(|n| {
                    n.texture_key.starts_with("rui_lucide_") && n.opacity > 0.01 && inside(n.rect)
                });
                assert!(
                    glyph || texture,
                    "focused password {suffix} must collect a visible icon inside {:?}",
                    button.layout_bounds
                );
            }
        });
    });
    rui::app::reactive::reset_registered_anchors();
}

fn gallery_collect_input_bounds(el: &rui::core::tree::Element, out: &mut Vec<(String, [f32; 4])>) {
    if let Some(fid) = el.focus_id.as_deref() {
        if let Some(rest) = fid.strip_prefix(rui::components::INPUT_FOCUS_PREFIX) {
            out.push((rest.to_string(), el.layout_bounds));
        }
    }
    for c in &el.children {
        gallery_collect_input_bounds(c, out);
    }
}

fn gallery_tree_has_input_caret(el: &rui::core::tree::Element) -> bool {
    el.text_style.input_caret.is_some() || el.children.iter().any(gallery_tree_has_input_caret)
}

/// Gallery forms: wrap Inputs in card「2) Input」used to drop `input_caret` from
/// `text_nodes` once the first field was only a chrome sliver — event loop fell
/// to bare `Wait`. 2b InputAdornment is the control that must still paint a caret
/// under the same scroll.
#[test]
fn forms_input_showcase_keeps_caret_when_first_field_partially_clipped() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                const VW: f32 = 520.0;
                const VH: f32 = 800.0;
                let showcase = "gal-s2-input-a";
                let adornment = "gal-ia-search";
                use_signal(
                    &rui::components::input_value_signal_key(showcase),
                    String::new,
                )
                .set("hello".into());
                use_signal(
                    &rui::components::input_value_signal_key(adornment),
                    String::new,
                )
                .set("query".into());

                fn collect_after_clip(
                    focus_rest: &str,
                    vw: f32,
                    vh: f32,
                ) -> (bool, bool, bool, f32, Vec<(String, [f32; 4])>, usize) {
                    use_signal("__rui_button_focus_id", String::new).set(format!(
                        "{}{focus_rest}",
                        rui::components::INPUT_FOCUS_PREFIX
                    ));
                    let mut tree =
                        rui::core::reactivity::isolate_reads(|| gallery_section_body("forms"));
                    let mut layout = rui::core::tree::LayoutTree::new();
                    let _ = layout.build_and_layout(&mut tree, vw, vh, [-1.0, -1.0]);
                    let mut bounds = Vec::new();
                    gallery_collect_input_bounds(&tree, &mut bounds);
                    let first = bounds
                        .iter()
                        .find(|(id, _)| id == "gal-s2-input-a")
                        .map(|(_, b)| *b)
                        .expect("gal-s2-input-a");
                    let scroll_y = first[1] + first[3] - 8.0;
                    assert!(
                        set_scroll_offset(&mut tree, "scroll-forms", [0.0, scroll_y]),
                        "scroll:scroll-forms viewport must exist"
                    );
                    use_signal(
                        &rui::core::scroll_viewport::scroll_offset_y_signal_key("scroll-forms"),
                        || 0.0_f32,
                    )
                    .set_silent(scroll_y);
                    let clipped =
                        layout.build_and_layout_fast(&mut tree, vw, vh, [-1.0, -1.0], true, false);
                    let mut after = Vec::new();
                    gallery_collect_input_bounds(&tree, &mut after);
                    let caret = clipped
                        .text_nodes
                        .iter()
                        .find(|n| n.style.input_caret.is_some());
                    let caret_fully_in_clip = caret.is_some_and(|n| match n.clip_bounds {
                        None => n.y >= -0.5 && n.y + n.height <= vh + 0.5,
                        Some(c) => {
                            n.width > 0.0
                                && n.height > 0.0
                                && c[2] > 0.0
                                && c[3] > 0.0
                                && n.x >= c[0] - 0.5
                                && n.y >= c[1] - 0.5
                                && n.x + n.width <= c[0] + c[2] + 0.5
                                && n.y + n.height <= c[1] + c[3] + 0.5
                        }
                    });
                    (
                        gallery_tree_has_input_caret(&tree),
                        caret.is_some(),
                        caret_fully_in_clip,
                        scroll_y,
                        after,
                        clipped.text_nodes.len(),
                    )
                }

                let (tree_a, nodes_a, in_clip_a, sy_a, after_a, ntext_a) =
                    collect_after_clip(showcase, VW, VH);
                assert!(
                    nodes_a,
                    "2) Input focused field must keep caret in text_nodes when the first wrap field is a chrome sliver.\n\
                     tree_caret={tree_a} scroll_y={sy_a} text_nodes={ntext_a} inputs={after_a:?}"
                );
                assert!(
                    !in_clip_a,
                    "2) sliver leaf must sit outside its overflow clip (Windows Imm hang trigger).\n\
                     tree_caret={tree_a} scroll_y={sy_a} text_nodes={ntext_a} inputs={after_a:?}"
                );

                let (tree_b, nodes_b, in_clip_b, sy_b, after_b, ntext_b) =
                    collect_after_clip(adornment, VW, VH);
                assert!(
                    nodes_b,
                    "2b InputAdornment is the control: caret must survive the same first-field clip.\n\
                     tree_caret={tree_b} scroll_y={sy_b} text_nodes={ntext_b} inputs={after_b:?}"
                );
                assert!(
                    in_clip_b,
                    "2b leaf must stay fully inside the ScrollArea clip under the same scroll.\n\
                     tree_caret={tree_b} scroll_y={sy_b} text_nodes={ntext_b} inputs={after_b:?}"
                );
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
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

/// First paint of the data page shows invoice + data_table cells; the next
/// paint-only / hover chrome frame must keep them (gallery flash / empty rows).
#[test]
fn data_section_table_texts_survive_skip_and_hover() {
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
                let first = layout.build_and_layout(&mut tree, 1280.0, 800.0, [-1.0, -1.0]);
                let texts = |r: &rui::core::tree::RenderResult| {
                    r.text_nodes
                        .iter()
                        .map(|n| n.text.clone())
                        .collect::<Vec<_>>()
                };
                let t0 = texts(&first);
                // Invoice composite is in-tree (not a reactive_view island).
                // data_table cells wait on remount drain — assert those in rui table_paint.
                for needle in ["INV001", "Paid", "$250.00", "INV002", "Invoice"] {
                    assert!(
                        t0.iter().any(|t| t.contains(needle)),
                        "first collect must paint `{needle}`; texts={t0:?}"
                    );
                }

                layout.set_collect_skip_allowed(true);
                let _ = layout.build_and_layout_fast(
                    &mut tree,
                    1280.0,
                    800.0,
                    [-1.0, -1.0],
                    false,
                    false,
                );
                let skipped = layout.build_and_layout_fast(
                    &mut tree,
                    1280.0,
                    800.0,
                    [-1.0, -1.0],
                    false,
                    false,
                );
                let t1 = texts(&skipped);
                for needle in ["INV001", "Paid", "$250.00", "INV002"] {
                    assert!(
                        t1.iter().any(|t| t.contains(needle)),
                        "paint-only skip must keep `{needle}`; texts={t1:?}"
                    );
                }

                let (mouse, on_hover) = {
                    fn find_row<'a>(
                        el: &'a rui::core::tree::Element,
                    ) -> Option<&'a rui::core::tree::Element> {
                        if el.focus_id.as_deref() == Some("inv-0") {
                            return Some(el);
                        }
                        for c in &el.children {
                            if let Some(hit) = find_row(c) {
                                return Some(hit);
                            }
                        }
                        None
                    }
                    let row = find_row(&tree).expect("inv-0 row");
                    let b = row.layout_bounds;
                    ([b[0] + b[2] * 0.5, b[1] + b[3] * 0.5], row.on_hover.clone())
                };
                if let Some(hover) = on_hover {
                    hover.borrow_mut()(true);
                }
                let mut impact = rui::core::tree::LayoutStatus::Clean;
                rui::style::sync_class_variants_subtree(
                    &mut tree,
                    mouse,
                    "",
                    false,
                    1280.0,
                    800.0,
                    &mut layout,
                    &mut impact,
                );
                rui::style::tick_active_css_transitions_subtree(
                    &mut tree,
                    mouse,
                    "",
                    false,
                    1280.0,
                    800.0,
                    [0.0, 0.0],
                    &mut layout,
                    &mut impact,
                );
                let hovered =
                    layout.build_and_layout_fast(&mut tree, 1280.0, 800.0, mouse, true, false);
                let t2 = texts(&hovered);
                for needle in ["INV001", "Paid", "$250.00", "INV002"] {
                    assert!(
                        t2.iter().any(|t| t.contains(needle)),
                        "hover chrome must not drop `{needle}`; texts={t2:?}"
                    );
                }
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
                    "events",
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

fn gallery_section_scroll_id(key: &str) -> &'static str {
    match key {
        "events" => "scroll-events",
        "forms" => "scroll-forms",
        "overlays" => "scroll-overlays",
        "draw" => "scroll-draw",
        "data" => "scroll-data",
        "navigation" => "scroll-navigation",
        "layout" => "scroll-layout",
        "text" => "scroll-text",
        "theme" => "scroll-theme",
        "about" => "scroll-about",
        _ => "scroll-basics",
    }
}

/// Page ScrollArea uses `overflow_x: auto`. Wrapping / 100% cards must not enable
/// a horizontal thumb — the live Gallery bug after switching sidebar sections.
#[test]
fn gallery_section_pages_must_not_enable_x_scroll() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                for key in [
                    "basics",
                    "events",
                    "forms",
                    "overlays",
                    "draw",
                    "data",
                    "navigation",
                    "layout",
                    "text",
                    "theme",
                    "about",
                ] {
                    rui::app::reactive::reset_registered_anchors();
                    let rest = gallery_section_scroll_id(key);
                    let mut tree = gallery_section_body(key);
                    let mut layout = rui::core::tree::LayoutTree::new();
                    for &(w, h) in &[(640.0, 700.0), (800.0, 700.0), (900.0, 700.0)] {
                        let result = layout.build_and_layout(&mut tree, w, h, [-1.0, -1.0]);
                        let (vp, max, content) = scroll_metrics_xy(&result, rest);
                        assert!(
                            max[0] < 0.5,
                            "{key} at {w}x{h} must not enable horizontal scroll \
                             (content_w={:.1} vp_w={:.1} max_x={:.1})",
                            content[0],
                            vp[2],
                            max[0]
                        );
                    }
                }
            });
        });
        rui::app::reactive::reset_registered_anchors();
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
            find_tid(&tree, "gal-theme-apply") || gallery_find_attr(&tree, "id", "gal-theme-apply"),
            "theme section must mount apply theme.toml"
        );
        assert!(
            find_tid(&tree, "gal-theme-sheet") || gallery_find_attr(&tree, "id", "gal-theme-sheet"),
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
        assert!(
            find_tid(&tree, GAL_SHELL_GLASS_BLUR),
            "navigation section must mount custom Blur recipe button"
        );
        assert!(
            find_tid(&tree, GAL_SHELL_GLASS_AERO),
            "navigation section must mount Aero recipe button"
        );
        assert!(
            find_tid(&tree, GAL_SHELL_GLASS_ACRYLIC),
            "navigation section must mount Acrylic recipe button"
        );
        assert!(
            find_tid(&tree, GAL_SHELL_GLASS_MICA),
            "navigation section must mount Mica recipe button"
        );
        rui::app::reactive::reset_registered_anchors();
    });
}

fn subtree_nodes_with_class_util(el: &rui::core::tree::Element, util: &str) -> usize {
    let here = el
        .class_set
        .as_ref()
        .is_some_and(|cs| cs.variants.iter().any(|(_, u)| u == util)) as usize;
    here + el
        .children
        .iter()
        .map(|c| subtree_nodes_with_class_util(c, util))
        .sum::<usize>()
}

#[test]
fn frameless_title_bar_close_overrides_hover_bg_destructive() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let (_handle, tree) = create_root(gallery_frameless_root);
        assert_eq!(
            subtree_nodes_with_class_util(&tree, "bg-destructive"),
            1,
            "gallery caption close must overlay hover:bg-destructive; min/max stay ghost accent"
        );
        assert_eq!(
            subtree_nodes_with_class_util(&tree, "text-destructive-foreground"),
            1,
            "close hover text must overlay destructive-foreground for contrast on red"
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

fn instance_fill_mpx(instances: &[rui::renderer::InstanceInput], vw: f32, vh: f32) -> f32 {
    let mut area = 0.0f32;
    for i in instances {
        let x0 = i.rect_pos[0].max(0.0);
        let y0 = i.rect_pos[1].max(0.0);
        let x1 = (i.rect_pos[0] + i.rect_size[0]).min(vw);
        let y1 = (i.rect_pos[1] + i.rect_size[1]).min(vh);
        if x1 > x0 && y1 > y0 {
            area += (x1 - x0) * (y1 - y0);
        }
    }
    area / 1_000_000.0
}

/// Default window vs maximized: CPU layout/collect + fill. Interactive stutter
/// after maximize is this path every hover/scroll frame, not the first mount.
///
/// `build_gallery_app()` is a reactive_view shell — use the live `<When>` page tree.
#[test]
fn gallery_layout_cost_default_vs_maximized() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                for section in ["basics", "layout", "draw"] {
                    rui::app::reactive::reset_registered_anchors();
                    let mut app =
                        rui::core::reactivity::isolate_reads(|| gallery_section_body(section));
                    let _ = rui::layout::layout_tree::hoist_portal_subtrees(&mut app);
                    let mut layout = rui::core::tree::LayoutTree::new();
                    let sizes = [(800.0, 600.0), (1280.0, 800.0), (1920.0, 1080.0)];
                    let mut fast_ms = [0.0f32; 3];
                    let mut inst = [0usize; 3];
                    for (i, (w, h)) in sizes.iter().copied().enumerate() {
                        let first = layout.build_and_layout(&mut app, w, h, [0.0, 0.0]);
                        layout.set_collect_skip_allowed(true);
                        let _ =
                            layout.build_and_layout_fast(&mut app, w, h, [0.0, 0.0], false, false);
                        let t = std::time::Instant::now();
                        let fast =
                            layout.build_and_layout_fast(&mut app, w, h, [0.0, 0.0], false, false);
                        fast_ms[i] = t.elapsed().as_secs_f32() * 1000.0;
                        inst[i] = fast.instances.len();
                        let fill = instance_fill_mpx(&fast.instances, w, h);
                        let (skipped, patched) = layout.last_collect_skip_counts();
                        eprintln!(
                            "gallery/{section} {w:.0}x{h:.0}: first taffy={:.2} collect={:.2} inst={} text={} fill={fill:.2}Mpx | fast wall={:.2}ms taffy={:.2} collect={:.2} inst={} text={} skip={skipped} patch={patched}",
                            first.prof.taffy_compute_ms,
                            first.prof.collect_ms,
                            first.instances.len(),
                            first.text_nodes.len(),
                            fast_ms[i],
                            fast.prof.taffy_compute_ms,
                            fast.prof.collect_ms,
                            fast.instances.len(),
                            fast.text_nodes.len(),
                        );
                    }
                    assert!(
                        inst[2] > 15,
                        "gallery/{section} must collect the real page at 1920x1080, not the shell placeholder (inst {}→{})",
                        inst[0],
                        inst[2]
                    );
                    // Debug collect+cascade of the live page is far above a release 60Hz
                    // slice. Guard the maximize stutter (this path every hover frame)
                    // relatively when the small window already shows a real page, and
                    // keep a debug-aware absolute ceiling.
                    let max_ms = if cfg!(debug_assertions) { 28.0 } else { 12.0 };
                    assert!(
                        fast_ms[2] < max_ms,
                        "gallery/{section} maximized 1920x1080 paint-only layout {fast:.2}ms \
                         (800x600={small:.2}ms inst {}→{} budget={max_ms:.0}ms) would drop frames after maximize",
                        inst[0],
                        inst[2],
                        fast = fast_ms[2],
                        small = fast_ms[0],
                    );
                    if inst[0] >= 15 {
                        assert!(
                            fast_ms[2] <= fast_ms[0] * 1.35 + 3.0,
                            "gallery/{section} maximize must not inflate paint-only layout vs 800x600 \
                             ({fast:.2}ms vs {small:.2}ms inst {}→{})",
                            inst[0],
                            inst[2],
                            fast = fast_ms[2],
                            small = fast_ms[0],
                        );
                    }
                }
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

/// Scan viewport-sized slices so expensive content cannot hide below the fold.
#[test]
#[ignore = "performance scan; run explicitly with --ignored --nocapture"]
fn gallery_scroll_segment_cost() {
    run_on_gallery_build_stack(|| {
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_owner, _) = create_root(|| {
            create_effect(|| {
                for section in [
                    "basics",
                    "forms",
                    "events",
                    "overlays",
                    "data",
                    "draw",
                    "navigation",
                    "layout",
                    "text",
                    "theme",
                ] {
                    rui::app::reactive::reset_registered_anchors();
                    let mut root =
                        rui::core::reactivity::isolate_reads(|| gallery_section_body(section));
                    let mut tree = rui::core::tree::LayoutTree::new();
                    let first = tree.build_and_layout(&mut root, 1920.0, 1080.0, [-1.0; 2]);
                    let Some((rest, _, _, content)) = first
                        .scroll_metrics
                        .iter()
                        .find(|(rest, _, _, _)| rest == &format!("scroll-{section}"))
                        .cloned()
                    else {
                        continue;
                    };
                    let offset = rui::core::reactivity::use_signal(
                        &rui::core::scroll_viewport::scroll_offset_y_signal_key(&rest),
                        || 0.0_f32,
                    );
                    tree.set_collect_skip_allowed(true);
                    let max = (content[1] - 1080.0).max(0.0);
                    for step in 0..=((max / 900.0).ceil() as usize) {
                        let y = (step as f32 * 900.0).min(max);
                        let mut times = Vec::new();
                        let mut stats = (0.0, 0.0, 0, 0);
                        for frame in 0..4 {
                            offset.set_silent(y + frame as f32);
                            let start = std::time::Instant::now();
                            let out = tree.build_and_layout_fast(
                                &mut root, 1920.0, 1080.0, [-1.0; 2], true, true,
                            );
                            if frame > 0 {
                                times.push(start.elapsed().as_secs_f32() * 1000.0);
                            }
                            stats = (
                                out.prof.collect_ms,
                                out.prof.collect_stamp_ms,
                                out.instances.len(),
                                out.text_nodes.len(),
                            );
                        }
                        times.sort_by(f32::total_cmp);
                        eprintln!("scroll/{section} y={y:.0}/{max:.0}: wall={:.2}ms collect={:.2} stamp={:.2} inst={} text={}", times[1], stats.0, stats.1, stats.2, stats.3);
                        assert!(times[1] < 10.0,
                            "gallery/{section} scroll segment y={y:.0} costs {:.2}ms before GPU work", times[1]);
                    }
                }
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

/// Clicking the RSX ViewportModal trigger must not eat the card's VStack gap
/// (button label toggles 打开→关闭 while DialogFade · RSX / SheetSlide stay put).
#[test]
fn viewport_modal_rsx_open_keeps_card_vstack_gap() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        rui::app::portal_overlay::clear_overlay_stack();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::renderer::font_loader::ensure_coverage_for_text(
            &mut fs,
            "打开 RSX ViewportModal 关闭 RSX 模态 视口舞台 presence DialogFade SheetSlide",
        );
        rui::text::install_shared_font_system(&mut fs);

        let text_y = |result: &rui::layout::layout_tree::RenderResult, needle: &str| -> f32 {
            result
                .text_nodes
                .iter()
                .find(|t| t.text.contains(needle))
                .map(|t| t.y)
                .unwrap_or_else(|| {
                    panic!(
                        "missing `{needle}` in {:?}",
                        result
                            .text_nodes
                            .iter()
                            .map(|t| (t.text.as_str(), t.y, t.height))
                            .collect::<Vec<_>>()
                    )
                })
        };
        let layout_card = |width: f32| {
            rui::app::reactive::reset_registered_anchors();
            let mut tree = ViewportModalShowcase(ViewportModalShowcaseProps::default());
            let _ = rui::layout::layout_tree::hoist_portal_subtrees(&mut tree);
            let mut layout = rui::core::tree::LayoutTree::new();
            layout.build_and_layout(&mut tree, width, 800.0, [0.0, 0.0])
        };

        let (_handle, _) = create_root(|| {
            let open = use_signal("gal_vm_dlg_open", || false);
            let closed = layout_card(180.0);
            let gap_closed = text_y(&closed, "SheetSlide") - text_y(&closed, "DialogFade");
            let below_closed = text_y(&closed, "SheetSlide")
                - closed
                    .text_nodes
                    .iter()
                    .find(|t| t.text.contains("打开 RSX") || t.text.contains("关闭 RSX"))
                    .map(|t| t.y + t.height)
                    .unwrap_or(0.0);

            open.set(true);
            let opened = layout_card(180.0);
            let gap_open = text_y(&opened, "SheetSlide") - text_y(&opened, "DialogFade");

            open.set(false);
            let wide = layout_card(400.0);
            let below_wide = text_y(&wide, "SheetSlide")
                - wide
                    .text_nodes
                    .iter()
                    .find(|t| t.text.contains("打开 RSX") || t.text.contains("关闭 RSX"))
                    .map(|t| t.y + t.height)
                    .unwrap_or(0.0);

            eprintln!(
                "gallery RSX ViewportModal closed gap={gap_closed:.1} below={below_closed:.1} open gap={gap_open:.1} wide below={below_wide:.1}"
            );
            assert!(
                (gap_open - gap_closed).abs() < 1.0,
                "opening RSX ViewportModal must not change DialogFade→SheetSlide spacing; closed={gap_closed:.1} open={gap_open:.1}"
            );
            assert!(
                (below_closed - below_wide).abs() < 1.0,
                "0×0 Content must not wrap under the trigger; narrow below={below_closed:.1} wide below={below_wide:.1}"
            );
        });

        rui::app::portal_overlay::clear_overlay_stack();
        rui::app::reactive::reset_registered_anchors();
    });
}

/// Live Gallery chat card: Latest jump control must stay centered inside the
/// 24rem card, not clip on the left overflow edge.
#[test]
fn message_scroller_latest_button_stays_inside_chat_card() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut root = rui::core::reactivity::isolate_reads(|| {
                    MessageScrollerShowcase(MessageScrollerShowcaseProps::default())
                });
                let mut layout = rui::core::tree::LayoutTree::new();
                for _ in 0..3 {
                    let _ = layout.build_and_layout(&mut root, 420.0, 900.0, [-1.0, -1.0]);
                }
                use_signal(
                    &rui::core::scroll_viewport::scroll_offset_y_signal_key(
                        "gal-ms-showcase:viewport",
                    ),
                    || 0.0_f32,
                )
                .set_silent(0.0);
                let result = layout.build_and_layout(&mut root, 420.0, 900.0, [-1.0, -1.0]);
                let btn = gallery_find_slot(&root, "message-scroller-button")
                    .expect("MessageScroller Latest button");
                let scroller =
                    gallery_find_slot(&root, "message-scroller").expect("MessageScroller root");
                let b = btn.layout_bounds;
                let s = scroller.layout_bounds;
                let visual_x = b[0] + b[2] * btn.style.translate_pct[0];
                let btn_cx = visual_x + b[2] * 0.5;
                let scroller_cx = s[0] + s[2] * 0.5;
                assert!(
                    s[2] > 200.0,
                    "chat scroller must have a real card width, got {s:?}"
                );
                assert!(
                    b[2] > 24.0 && b[2] < s[2] * 0.7,
                    "Latest pill must hug the label, not stretch; btn={b:?} scroller={s:?}"
                );
                assert!(
                    visual_x > s[0] + 8.0 && visual_x + b[2] < s[0] + s[2] - 8.0,
                    "Latest pill must stay inside the chat clip; visual_x={visual_x:.1} btn={b:?} scroller={s:?}"
                );
                assert!(
                    (btn_cx - scroller_cx).abs() < s[2] * 0.25,
                    "Latest pill must sit near the chat center, not the left edge; \
                     btn_cx={btn_cx:.1} scroller_cx={scroller_cx:.1} scroller={s:?}"
                );
                let latest = result.text_nodes.iter().find(|t| {
                    t.text == "Latest" || t.text == "最新" || t.text.eq_ignore_ascii_case("latest")
                });
                if let Some(latest) = latest {
                    assert!(
                        latest.x > s[0] + 16.0,
                        "Latest glyph must not sit on the left clip; x={} clip={:?} scroller={s:?}",
                        latest.x,
                        latest.clip_bounds
                    );
                }
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

fn gallery_subtree_has_text(el: &rui::core::tree::Element, needle: &str) -> bool {
    if el.text.as_deref().is_some_and(|t| t.contains(needle)) {
        return true;
    }
    el.children
        .iter()
        .any(|c| gallery_subtree_has_text(c, needle))
}

fn gallery_find_slot_with_text<'a>(
    el: &'a rui::core::tree::Element,
    slot: &str,
    needle: &str,
) -> Option<&'a rui::core::tree::Element> {
    if el.attrs.get("data-slot").map(String::as_str) == Some(slot)
        && gallery_subtree_has_text(el, needle)
    {
        return Some(el);
    }
    el.children
        .iter()
        .find_map(|c| gallery_find_slot_with_text(c, slot, needle))
}

/// Live Attachment state bind must sit outside AttachmentGroup (`overflow-x: auto`),
/// otherwise the chip is clipped and the idle/uploading/… buttons look like no-ops.
#[test]
fn attachment_live_state_chip_is_not_clipped_by_group() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree = rui::core::reactivity::isolate_reads(|| {
                    AttachmentShowcase(AttachmentShowcaseProps::default())
                });
                let mut layout = rui::core::tree::LayoutTree::new();
                for width in [360.0, 720.0, 1100.0] {
                    let _ = layout.build_and_layout(&mut tree, width, 1600.0, [0.0, 0.0]);
                    let group =
                        gallery_find_slot(&tree, "attachment-group").expect("AttachmentGroup");
                    let first_y = group.children.first().unwrap().layout_bounds[1];
                    assert!(
                        group
                            .children
                            .iter()
                            .any(|child| child.layout_bounds[1] > first_y + 1.0),
                        "attachments must wrap at {width}px"
                    );
                    for child in &group.children {
                        let bounds = child.layout_bounds;
                        assert!(
                            bounds[0] >= 0.0 && bounds[0] + bounds[2] <= width + 1.0,
                            "attachment overflows {width}px: {bounds:?}"
                        );
                    }
                    assert!(
                        !gallery_subtree_has_text(group, "Reactive state:"),
                        "live state chip must not live inside the overflow-x group"
                    );
                    let live = gallery_find_slot_with_text(&tree, "attachment", "Reactive state:")
                        .expect("live Reactive state chip");
                    let b = live.layout_bounds;
                    assert!(
                        b[2] > 80.0 && b[3] > 24.0,
                        "live chip must have a real used box, got {b:?}"
                    );
                    assert!(
                        b[0] >= 0.0 && b[0] + b[2] <= width + 1.0,
                        "live chip must stay inside the card, not past the overflow clip; bounds={b:?}"
                    );
                }
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}

/// Gallery 基础「Status (Marker inside Message)」: shimmer caption must keep the
/// full 「Oliver is typing...」 ink. Nested Message `min-w-0` used to clip it to
/// 「Oliver i」 while dest-reset flashed the stub.
#[test]
fn basics_oliver_is_typing_shimmer_must_keep_full_ink() {
    run_on_gallery_build_stack(|| {
        rui::app::reactive::reset_registered_anchors();
        let mut fs = rui::renderer::font_loader::create_font_system();
        rui::renderer::font_loader::apply_default_families(fs.db_mut());
        rui::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree = rui::core::reactivity::isolate_reads(|| {
                    MessageShowcase(MessageShowcaseProps::default())
                });
                let mut layout = rui::core::tree::LayoutTree::new();
                for &(w, h) in &[(400.0, 900.0), (720.0, 900.0)] {
                    let result = layout.build_and_layout(&mut tree, w, h, [-1.0, -1.0]);
                    let caption = result
                        .text_nodes
                        .iter()
                        .find(|n| n.text.contains("Oliver is typing"))
                        .unwrap_or_else(|| {
                            panic!(
                                "missing Oliver is typing... at {w}x{h}; olivers={:?}",
                                result
                                    .text_nodes
                                    .iter()
                                    .filter(|n| n.text.contains("Oliver"))
                                    .map(|n| format!(
                                        "{:?} {:.1}x{:.1} clip={:?}",
                                        n.text, n.width, n.height, n.clip_bounds
                                    ))
                                    .collect::<Vec<_>>()
                            )
                        });
                    let ink = caption
                        .prepared
                        .as_ref()
                        .map(rui::text::intrinsic_width)
                        .unwrap_or(0.0);
                    assert!(
                        caption.text.contains("Oliver is typing"),
                        "caption string truncated at {w}x{h}: {:?}",
                        caption.text
                    );
                    assert!(
                        caption.width + 1.0 >= ink,
                        "Status (Marker inside Message) caption box too narrow at {w}x{h}\n \
                         box={:.1}x{:.1} ink={:.1} clip={:?} fx={:?} text={:?}",
                        caption.width,
                        caption.height,
                        ink,
                        caption.clip_bounds,
                        caption.glyph_fx,
                        caption.text
                    );
                    if let Some(clip) = caption.clip_bounds {
                        assert!(
                            clip[2] + 1.0 >= ink,
                            "caption collect clip crops Oliver is typing... at {w}x{h}\n \
                             clip={clip:?} ink={ink:.1} box_w={:.1}",
                            caption.width
                        );
                    }
                }
            });
        });
        rui::app::reactive::reset_registered_anchors();
    });
}
