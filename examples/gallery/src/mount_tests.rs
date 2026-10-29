use crate::app::*;
use crate::imports::*;
use crate::pages::*;
use crate::shared::*;
use jadix::core::reactivity::{create_effect, create_root};

#[test]
fn padding_hover_demo_keeps_an_interactive_box_through_reversals() {
    run_on_gallery_build_stack(|| {
        create_root(|| {
            jadix::style::set_css_transition_virtual_timeline(true);
            jadix::style::set_css_transition_timeline_ms(0.0);
            let mut root = crate::pages::layout::AnimAlignmentShowcase(Default::default());
            let mut tree = jadix::layout::LayoutTree::new();
            tree.build_and_layout(&mut root, 900.0, 3000.0, [-1.0; 2]);
            let bounds = gallery_find_by_tid(&root, "gal-padding-hover")
                .unwrap()
                .layout_bounds;
            assert!(bounds[2] > 100.0 && bounds[3] > 40.0, "{bounds:?}");
            for (step, hovered) in [true, false, true, false, true].into_iter().enumerate() {
                let mouse = if hovered {
                    [bounds[0] + bounds[2] * 0.5, bounds[1] + bounds[3] * 0.5]
                } else {
                    [-1.0; 2]
                };
                let mut impact = LayoutStatus::Clean;
                jadix::style::tick_active_css_transitions_subtree(
                    &mut root,
                    mouse,
                    "",
                    true,
                    900.0,
                    3000.0,
                    [0.0; 2],
                    &mut tree,
                    &mut impact,
                );
                jadix::style::set_css_transition_timeline_ms((step + 1) as f64 * 500.0);
                jadix::style::tick_active_css_transitions_subtree(
                    &mut root,
                    mouse,
                    "",
                    true,
                    900.0,
                    3000.0,
                    [0.0; 2],
                    &mut tree,
                    &mut impact,
                );
                let target = gallery_find_by_tid(&root, "gal-padding-hover").unwrap();
                let padding = jadix::layout::lp_length_px(target.style.padding.left).unwrap();
                assert!(
                    (padding - if hovered { 16.0 } else { 4.0 }).abs() < 0.1,
                    "hover={hovered} padding={padding}"
                );
            }
            jadix::style::set_css_transition_virtual_timeline(false);
        });
    });
}

#[test]
fn drawing_canvas_preserves_strokes_on_remount_and_clears_independently() {
    fn canvas(root: &Element) -> Option<&Element> {
        if root.on_mousedown.is_some()
            && root
                .attrs
                .contains_key(jadix::primitives::canvas::ATTR_CANVAS_PAINT)
        {
            Some(root)
        } else {
            root.children.iter().find_map(canvas)
        }
    }
    run_on_gallery_build_stack(|| {
        create_root(|| {
            let first = crate::pages::draw::DrawingCanvasShowcase(Default::default());
            let leaf = canvas(&first).unwrap();
            let down = MouseEvent::new(10.0, 15.0, winit::event::MouseButton::Left, 1);
            (leaf.on_mousedown.as_ref().unwrap().borrow_mut())(&down);
            let moved = MouseEvent::new(80.0, 60.0, winit::event::MouseButton::Left, 1);
            (leaf.on_mousemove.as_ref().unwrap().borrow_mut())(&moved);
            let strokes = use_signal("gal-canvas-strokes", || {
                Rc::new(RefCell::new(Vec::<Vec<(f32, f32)>>::new()))
            })
            .peek();
            assert_eq!(&*strokes.borrow(), &[vec![(10.0, 15.0), (80.0, 60.0)]]);
            drop(first);
            let second = crate::pages::draw::DrawingCanvasShowcase(Default::default());
            assert_eq!(
                strokes.borrow().len(),
                1,
                "leaving the page must not discard strokes"
            );
            let clear = gallery_find_by_tid(&second, "gal-canvas-clear").unwrap();
            (clear.on_click.as_ref().unwrap().borrow_mut())();
            assert!(strokes.borrow().is_empty());
        });
    });
}

#[test]
fn synthetic_stream_image_is_collectable_without_a_canvas_clock() {
    run_on_gallery_build_stack(|| {
        create_root(|| {
            let mut root = crate::pages::draw::ImageStreamShowcase(Default::default());
            let mut tree = jadix::layout::LayoutTree::new();
            tree.build_and_layout(&mut root, 900.0, 900.0, [-1.0; 2]);
            for _ in 0..3 {
                let frame =
                    tree.build_and_layout_fast(&mut root, 900.0, 900.0, [-1.0; 2], true, false);
                assert!(
                    frame
                        .textured_draws
                        .iter()
                        .any(|draw| draw.texture_key == "jadix-gallery-stream"),
                    "stream image must remain visible to collect"
                );
                assert!(
                    !frame
                        .textured_draws
                        .iter()
                        .any(|draw| draw.texture_key.starts_with("jadix_canvas:")),
                    "stream visibility must follow its image instead of a separate clock"
                );
            }
        });
    });
}

#[test]
fn window_session_showcase_mounts_and_reports_missing_session() {
    run_on_gallery_build_stack(|| {
        create_root(|| {
            let root = crate::pages::desktop_windows::WindowSessionShowcase(
                crate::pages::desktop_windows::WindowSessionShowcaseProps::default(),
            );
            let send = gallery_find_by_tid(&root, "gal-session-send").unwrap();
            (send.on_click.as_ref().unwrap().borrow_mut())();
            assert_eq!(
                use_signal("gal-session-status", String::new).peek(),
                "请先打开会话次窗"
            );
        });
    });
}

#[test]
fn gallery_button_showcase_retains_ui_source() {
    run_on_gallery_build_stack(|| {
        create_root(|| {
            let root = crate::pages::basics::ButtonShowcase(
                crate::pages::basics::ButtonShowcaseProps::default(),
            );
            let source = root
                .get_attr("jadix_ui_template_id")
                .unwrap()
                .replace('\\', "/");
            assert!(
                source.starts_with("examples/gallery/src/pages/basics.rs:"),
                "{source}"
            );
        });
    });
}

#[test]
fn responsive_gallery_navigation_changes_section_and_closes_mobile_sheet() {
    run_on_gallery_build_stack(|| {
        create_root(|| {
            let section = Signal::new("basics".to_string());
            let open = Signal::new(true);
            let menu = gallery_nav_menu(section, "gal-mobile-test", Some(open));
            assert!(gallery_find_by_tid(&menu, "gal-mobile-test-branch-0:trigger").is_some());
            let item = gallery_nav_item(
                section,
                "gal-mobile-test",
                "forms",
                "表单与输入",
                "file-text",
                Some(open),
            );
            let action = gallery_find_by_tid(&item, "gal-mobile-test-nav-forms").unwrap();
            (action.on_click.as_ref().unwrap().borrow_mut())();
            assert_eq!(section.peek(), "forms");
            assert!(!open.peek());
        });
    });
}

#[test]
fn gallery_sidebar_branches_toggle_without_changing_the_selected_page() {
    run_on_gallery_build_stack(|| {
        create_root(|| {
            let section = Signal::new("basics".to_string());
            let menu = gallery_nav_menu(section, "gal-tree-test", None);
            let trigger = gallery_find_by_tid(&menu, "gal-tree-test-branch-0:trigger").unwrap();
            let expanded = use_signal("gal-tree-test-branch-0-open", || true);
            assert!(expanded.peek());
            trigger.on_click.as_ref().unwrap().borrow_mut()();
            assert!(!expanded.peek());
            assert_eq!(section.peek(), "basics");
            trigger.on_click.as_ref().unwrap().borrow_mut()();
            assert!(expanded.peek());
            assert_eq!(section.peek(), "basics");
        });
    });
}

#[test]
fn responsive_gallery_showcase_reflows_at_viewport_breakpoints() {
    fn grid(element: &Element) -> Option<&Element> {
        if !element.style.grid_template_columns.is_empty() && element.children.len() == 3 {
            Some(element)
        } else {
            element.children.iter().find_map(grid)
        }
    }
    run_on_gallery_build_stack(|| {
        jadix::style::set_prefers_reduced_motion(Some(true));
        create_root(|| {
            create_effect(|| {
                let mut root = ResponsiveShowcase(ResponsiveShowcaseProps::default());
                let mut layout = jadix::core::tree::LayoutTree::new();
                for width in [320.0, 639.5, 640.0, 767.5, 768.0, 1280.0, 320.0] {
                    let mut impact = jadix::core::tree::LayoutStatus::Clean;
                    jadix::style::sync_class_variants_subtree(
                        &mut root,
                        [-1.0, -1.0],
                        "",
                        false,
                        width,
                        1000.0,
                        &mut layout,
                        &mut impact,
                    );
                    layout.build_and_layout(&mut root, width, 1000.0, [-1.0, -1.0]);
                    fn check_bounds(el: &Element, width: f32) {
                        assert!(
                            el.layout_bounds[0] >= -0.5,
                            "width={width}, node={:?}, bounds={:?}",
                            el.text,
                            el.layout_bounds
                        );
                        assert!(
                            el.layout_bounds[0] + el.layout_bounds[2] <= width + 0.5,
                            "width={width}, node={:?}, bounds={:?}",
                            el.text,
                            el.layout_bounds
                        );
                        for child in &el.children {
                            assert!(
                                child.layout_bounds[1] + child.layout_bounds[3]
                                    <= el.layout_bounds[1] + el.layout_bounds[3] + 0.5,
                                "width={width}, parent={:?} {:?}, child={:?} {:?}",
                                el.attrs.get("data-slot"),
                                el.layout_bounds,
                                child.text,
                                child.layout_bounds,
                            );
                            check_bounds(child, width);
                        }
                    }
                    check_bounds(&root, width);
                    let grid = grid(&root).unwrap();
                    assert_eq!(
                        grid.style.grid_template_columns,
                        jadix::layout::cols_equal_fr(if width >= 1280.0 {
                            3
                        } else if width >= 768.0 {
                            2
                        } else {
                            1
                        }),
                        "width={width}"
                    );
                    if width < 768.0 {
                        assert!(
                            grid.children[1].layout_bounds[1] > grid.children[0].layout_bounds[1]
                        );
                    } else {
                        assert!(
                            (grid.children[1].layout_bounds[1] - grid.children[0].layout_bounds[1])
                                .abs()
                                < 0.5
                        );
                    }
                }
            });
        });
        jadix::style::set_prefers_reduced_motion(None);
    });
}

#[test]
fn anchored_overlay_pinned_attribute_matches_builder_dismiss_behavior() {
    run_on_gallery_build_stack(|| {
        for builder in [false, true] {
            for pinned in [false, true] {
                jadix::app::portal_overlay::clear_overlay_stack();
                jadix::app::reactive::reset_registered_anchors();
                let (owner, _) = create_root(|| {
                    let open = Signal::new(true);
                    let pos = Signal::new([120.0, 80.0]);
                    let kept = std::rc::Rc::new(std::cell::RefCell::new(None));
                    let kept_effect = kept.clone();
                    create_effect(move || {
                        let tree = if builder {
                            jadix::utils::anchored_overlay::anchored_overlay("pinned-builder-check")
                                .open(open)
                                .pinned(pinned)
                                .placement(jadix::utils::anchored_overlay::Placement::point(
                                    pos, 0.0,
                                ))
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
                    jadix::utils::popup::arm_underlay_close();
                    assert_eq!(
                        jadix::app::portal_overlay::try_dismiss_outside_press(),
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
        jadix::app::portal_overlay::clear_overlay_stack();
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn responsive_gallery_breakpoint_keeps_previous_presented_cards() {
    fn grid(element: &Element) -> Option<&Element> {
        if !element.style.grid_template_columns.is_empty() && element.children.len() == 3 {
            Some(element)
        } else {
            element.children.iter().find_map(grid)
        }
    }
    run_on_gallery_build_stack(|| {
        create_root(|| {
            create_effect(|| {
                let mut root = ResponsiveShowcase(ResponsiveShowcaseProps::default());
                let mut tree = jadix::core::tree::LayoutTree::new();
                for width in [700.0, 900.0] {
                    let mut impact = jadix::core::tree::LayoutStatus::Clean;
                    jadix::style::sync_class_variants_subtree(
                        &mut root,
                        [-1.0; 2],
                        "",
                        false,
                        width,
                        1000.0,
                        &mut tree,
                        &mut impact,
                    );
                    let old = grid(&root).unwrap().children[1].layout_bounds;
                    let frame = tree.build_and_layout(&mut root, width, 1000.0, [-1.0; 2]);
                    if width == 900.0 {
                        assert!(
                            frame.instances.iter().any(|instance| {
                                (instance.rect_pos[0] - old[0]).abs() < 0.1
                                    && (instance.rect_pos[1] - old[1]).abs() < 0.1
                                    && (instance.rect_size[0] - old[2]).abs() < 0.1
                                    && (instance.rect_size[1] - old[3]).abs() < 0.1
                            }),
                            "breakpoint change snapped instead of projecting the previous card: old={old:?}, paints={:?}",
                            frame
                                .instances
                                .iter()
                                .map(|instance| (instance.rect_pos, instance.rect_size))
                                .collect::<Vec<_>>()
                        );
                    }
                }
            });
        });
    });
}

#[test]
fn responsive_gallery_card_contains_wrapped_actions_in_scroll_viewport() {
    fn check_children(element: &Element, width: f32) {
        for child in &element.children {
            assert!(
                child.layout_bounds[1] + child.layout_bounds[3]
                    <= element.layout_bounds[1] + element.layout_bounds[3] + 0.5,
                "width={width}, parent={:?} {:?}, child={:?} {:?}",
                element.attrs.get("data-slot"),
                element.layout_bounds,
                child.text,
                child.layout_bounds,
            );
            check_children(child, width);
        }
    }
    fn card(element: &Element) -> Option<&Element> {
        if element
            .attrs
            .get("data-slot")
            .is_some_and(|slot| slot == "card")
        {
            Some(element)
        } else {
            element.children.iter().find_map(card)
        }
    }
    run_on_gallery_build_stack(|| {
        jadix::style::set_prefers_reduced_motion(Some(true));
        create_root(|| {
            create_effect(|| {
                let mut root = ui! {
                    <VStack w="100%" h="100%" min_w="0">
                        <ScrollArea id="responsive-height-test" flex_grow="1" min_h="0" overflow_y="auto">
                            <VStack w="100%" min_w="0" gap="1rem" class="p-3 sm:p-4 lg:p-6">
                                <ResponsiveShowcase />
                                <MyCard title="Next card"><Label>"Sibling content"</Label></MyCard>
                            </VStack>
                        </ScrollArea>
                    </VStack>
                };
                let mut tree = jadix::core::tree::LayoutTree::new();
                for width in [1280.0, 768.0, 640.0, 340.0, 290.0, 640.0, 340.0] {
                    let mut impact = jadix::core::tree::LayoutStatus::Clean;
                    jadix::style::sync_class_variants_subtree(
                        &mut root,
                        [-1.0; 2],
                        "",
                        false,
                        width,
                        460.0,
                        &mut tree,
                        &mut impact,
                    );
                    tree.build_and_layout(&mut root, width, 460.0, [-1.0; 2]);
                    check_children(card(&root).expect("responsive card"), width);
                }
            });
        });
        jadix::style::set_prefers_reduced_motion(None);
    });
}

#[test]
fn layout_motion_showcase_toggles_react_without_subscribing_animation_frames() {
    run_on_gallery_build_stack(|| {
        create_root(|| {
            let renders = Rc::new(Cell::new(0));
            let kept = Rc::new(RefCell::new(None));
            let count = renders.clone();
            let content = kept.clone();
            create_effect(move || {
                count.set(count.get() + 1);
                *content.borrow_mut() = Some(crate::pages::motion::build_layout_motion_showcase());
            });
            let click =
                gallery_find_by_tid(kept.borrow().as_ref().unwrap(), "gal-motion-toggle-gap")
                    .unwrap()
                    .on_click
                    .clone()
                    .unwrap();
            assert_eq!(renders.get(), 1);
            (click.borrow_mut())();
            assert_eq!(
                renders.get(),
                2,
                "layout signal must rebuild its local island"
            );
            let mut root = kept.borrow_mut().take().unwrap();
            let mut tree = jadix::layout::LayoutTree::new();
            tree.build_and_layout(&mut root, 900.0, 900.0, [-1.0; 2]);
            jadix::style::tick_animations(0.05);
            assert_eq!(
                renders.get(),
                2,
                "animation samples must not rebuild declarations"
            );
        });
    });
}

#[test]
fn theme_media_page_remount_preserves_live_presentation_signals() {
    run_on_gallery_build_stack(|| {
        create_root(|| {
            let revision = Signal::new(0);
            create_effect(move || {
                let _ = revision.get();
                let _ = crate::pages::media::MediaPlayerShowcase(
                    crate::pages::media::MediaPlayerShowcaseProps::default(),
                );
            });
            for cycle in 1..4 {
                revision.set(cycle);
            }
        });
    });
}

#[test]
fn responsive_gallery_shell_padding_starts_from_previous_spacing() {
    fn content(element: &Element) -> Option<&Element> {
        if element
            .attrs
            .get("jadix_utility_class")
            .is_some_and(|class| class.contains("p-3 sm:p-4 lg:p-6"))
        {
            Some(element)
        } else {
            element.children.iter().find_map(content)
        }
    }
    fn frame(root: &mut Element, tree: &mut jadix::layout::LayoutTree, width: f32) -> f32 {
        let mut impact = jadix::core::tree::LayoutStatus::Clean;
        jadix::style::sync_class_variants_subtree(
            root,
            [-1.0; 2],
            "",
            false,
            width,
            900.0,
            tree,
            &mut impact,
        );
        tree.build_and_layout(root, width, 900.0, [-1.0; 2]);
        let content = content(root).unwrap();
        content.children[0].layout_bounds[0] - content.layout_bounds[0]
    }
    run_on_gallery_build_stack(|| {
        jadix::style::set_css_transition_virtual_timeline(true);
        jadix::style::set_css_transition_timeline_ms(0.0);
        create_root(|| {
            create_effect(|| {
                let mut root = GallerySectionAbout(GallerySectionAboutProps::default());
                let mut tree = jadix::layout::LayoutTree::new();
                let mut previous = frame(&mut root, &mut tree, 1100.0);
                assert!((previous - 24.0).abs() < 0.1);
                for (width, target) in
                    [(1000.0, 16.0), (620.0, 12.0), (740.0, 16.0), (1100.0, 24.0)]
                {
                    let start = frame(&mut root, &mut tree, width);
                    assert!(
                        (start - previous).abs() < 0.1,
                        "responsive padding must start at {previous}px, not snap to {start}px"
                    );
                    jadix::style::advance_css_transition_timeline_ms(100.0);
                    let middle = frame(&mut root, &mut tree, width);
                    assert!(
                        middle > previous.min(target) && middle < previous.max(target),
                        "expected a real padding intermediate frame: {previous} -> {middle} -> {target}"
                    );
                    jadix::style::advance_css_transition_timeline_ms(100.0);
                    let end = frame(&mut root, &mut tree, width);
                    assert!((end - target).abs() < 0.1);
                    previous = target;
                }
                jadix::style::set_prefers_reduced_motion(Some(true));
                assert!((frame(&mut root, &mut tree, 620.0) - 12.0).abs() < 0.1);
                jadix::style::set_prefers_reduced_motion(None);
            });
        });
        jadix::style::set_css_transition_virtual_timeline(false);
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
fn gallery_section_body(section: &str) -> jadix::core::tree::Element {
    match section {
        "basics" => GallerySectionBasics(GallerySectionBasicsProps::default()),
        "forms" => GallerySectionForms(GallerySectionFormsProps::default()),
        "feedback" => GallerySectionFeedback(GallerySectionFeedbackProps::default()),
        "data" => GallerySectionData(GallerySectionDataProps::default()),
        "navigation" => GallerySectionNavigation(GallerySectionNavigationProps::default()),
        "overlays" => GallerySectionOverlays(GallerySectionOverlaysProps::default()),
        "events" => GallerySectionEvents(GallerySectionEventsProps::default()),
        "drag" => GallerySectionDrag(GallerySectionDragProps::default()),
        "layout" => GallerySectionLayout(GallerySectionLayoutProps::default()),
        "responsive" => GallerySectionResponsive(GallerySectionResponsiveProps::default()),
        "style" => GallerySectionStyle(GallerySectionStyleProps::default()),
        "transforms" => GallerySectionTransforms(GallerySectionTransformsProps::default()),
        "text" => GallerySectionText(GallerySectionTextProps::default()),
        "theme" => GallerySectionTheme(GallerySectionThemeProps::default()),
        "draw" => GallerySectionDraw(GallerySectionDrawProps::default()),
        "scene" => GallerySectionScene(GallerySectionSceneProps::default()),
        "media" => GallerySectionMedia(GallerySectionMediaProps::default()),
        "motion" => GallerySectionMotion(GallerySectionMotionProps::default()),
        "documents" => GallerySectionDocuments(GallerySectionDocumentsProps::default()),
        "code" => GallerySectionCode(GallerySectionCodeProps::default()),
        "desktop" => GallerySectionDesktop(GallerySectionDesktopProps::default()),
        "workspace" => GallerySectionWorkspace(GallerySectionWorkspaceProps::default()),
        "reactivity" => GallerySectionReactivity(GallerySectionReactivityProps::default()),
        "rendering" => GallerySectionRendering(GallerySectionRenderingProps::default()),
        "about" => GallerySectionAbout(GallerySectionAboutProps::default()),
        _ => panic!("unknown gallery section: {section}"),
    }
}

#[test]
fn gallery_catalog_has_unique_pages_and_populated_groups() {
    let keys: std::collections::HashSet<_> =
        GALLERY_SECTIONS.iter().map(|entry| entry.key).collect();
    assert_eq!(keys.len(), GALLERY_SECTIONS.len());
    assert_eq!(GALLERY_GROUPS.len(), 7);
    assert_eq!(keys.len(), 25);
    for group in GALLERY_GROUPS {
        assert!(GALLERY_SECTIONS.iter().any(|entry| entry.group == *group));
    }
    for entry in GALLERY_SECTIONS {
        assert!(GALLERY_GROUPS.contains(&entry.group));
        assert!(!entry.title.is_empty());
    }
}

#[test]
fn document_controls_switch_modes_and_restore_content() {
    use jadix::text::DocumentEditMode;
    run_on_gallery_build_stack(|| {
        create_root(|| {
            let tree = DocumentEditingShowcase(Default::default());
            let editor = gallery_document_state();
            let original = editor.document().source().text().to_string();
            for (id, mode) in [
                ("source", DocumentEditMode::Source),
                ("preview", DocumentEditMode::Preview),
                ("live", DocumentEditMode::LivePreview),
            ] {
                let button = gallery_find_by_tid(&tree, &format!("gallery-document-{id}")).unwrap();
                (button.on_click.as_ref().unwrap().borrow_mut())();
                assert_eq!(editor.snapshot().mode, mode);
            }
            editor.set_document("Changed document");
            let reset = gallery_find_by_tid(&tree, "gallery-document-reset").unwrap();
            (reset.on_click.as_ref().unwrap().borrow_mut())();
            assert_eq!(editor.document().source().text(), original);
        });
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn document_session_survives_page_owner_rebuilds() {
    use jadix::text::DocumentEditMode;
    run_on_gallery_build_stack(|| {
        for iteration in 0..3 {
            let (owner, tree) = create_root(|| DocumentEditingShowcase(Default::default()));
            let editor = gallery_document_state();
            if iteration == 0 {
                editor.set_document("# Persistent content");
            }
            assert_eq!(editor.document().source().text(), "# Persistent content");
            let button = gallery_find_by_tid(&tree, "gallery-document-source").unwrap();
            (button.on_click.as_ref().unwrap().borrow_mut())();
            assert_eq!(editor.snapshot().mode, DocumentEditMode::Source);
            owner.dispose();
            jadix::app::reactive::reset_registered_anchors();
            assert!(editor.signal().is_alive());
        }
    });
}

#[test]
fn code_and_diff_controls_update_their_independent_state() {
    use jadix_document::DiffMode;
    run_on_gallery_build_stack(|| {
        create_root(|| {
            let code = CodeEditingShowcase(Default::default());
            let source = use_signal("gallery-code-source", String::new);
            let original = source.peek();
            assert!(original.contains("ui!"));
            assert!(original.contains("button("));
            source.set("fn changed() {}".into());
            let reset = gallery_find_by_tid(&code, "gallery-code-reset").unwrap();
            (reset.on_click.as_ref().unwrap().borrow_mut())();
            assert_eq!(source.peek(), original);
            let diff = DocumentDiffShowcase(Default::default());
            let mode = use_signal("gallery-diff-mode", || DiffMode::Split);
            for (id, expected) in [("unified", DiffMode::Unified), ("split", DiffMode::Split)] {
                let button = gallery_find_by_tid(&diff, &format!("gallery-diff-{id}")).unwrap();
                (button.on_click.as_ref().unwrap().borrow_mut())();
                assert_eq!(mode.peek(), expected);
            }
        });
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn navigation_section_keeps_the_router_showcase() {
    run_on_gallery_build_stack(|| {
        let (_owner, tree) = create_root(|| gallery_section_body("navigation"));
        assert!(find_tid(&tree, "gal_shell_router__router_view"));
        jadix::app::reactive::reset_registered_anchors();
    });
}

fn gallery_find_tid(el: &jadix::core::tree::Element, want: &str) -> bool {
    gallery_find_by_tid(el, want).is_some()
}

fn gallery_find_by_tid<'a>(
    el: &'a jadix::core::tree::Element,
    want: &str,
) -> Option<&'a jadix::core::tree::Element> {
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
    el: &'a jadix::core::tree::Element,
    slot: &str,
) -> Option<&'a jadix::core::tree::Element> {
    let mut stack = vec![el];
    while let Some(el) = stack.pop() {
        if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
            return Some(el);
        }
        stack.extend(el.children.iter().rev());
    }
    None
}

fn gallery_tid_contains(el: &jadix::core::tree::Element, needle: &str) -> bool {
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

fn find_mouse_log_node(el: &jadix::core::tree::Element) -> Option<&jadix::core::tree::Element> {
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
    el: &'a jadix::core::tree::Element,
    want: &str,
) -> Option<&'a jadix::core::tree::Element> {
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

fn subtree_has_bg(el: &jadix::core::tree::Element, token: [f32; 4]) -> bool {
    color_close(el.style.bg_color, token) || el.children.iter().any(|c| subtree_has_bg(c, token))
}

fn find_lca_click_caption(el: &jadix::core::tree::Element) -> Option<&jadix::core::tree::Element> {
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
        jadix::app::reactive::reset_registered_anchors();
        jadix::style::set_theme_mode(jadix::style::ThemeMode::Light);
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let tree = jadix::core::reactivity::isolate_reads(|| {
                    MouseEventShowcase(MouseEventShowcaseProps::default())
                });
                let outer =
                    gallery_find_by_tid(&tree, "gal-mouse-lca-outer").expect("gal-mouse-lca-outer");
                let inner = gallery_find_by_focus_id(&tree, "gal-mouse-lca-target")
                    .expect("gal-mouse-lca-target");
                let muted = jadix::style::parse_color("muted");
                let info = jadix::style::parse_color("info");
                let primary = jadix::style::parse_color("primary");
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn mouse_event_lca_click_caption_is_plain_definite_width_leaf() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        jadix::style::set_theme_mode(jadix::style::ThemeMode::Light);
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree = jadix::core::reactivity::isolate_reads(|| {
                    MouseEventShowcase(MouseEventShowcaseProps::default())
                });
                let mut layout = jadix::core::tree::LayoutTree::new();
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn mouse_event_swallow_hit_path_excludes_pad() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree = jadix::core::reactivity::isolate_reads(|| {
                    MouseEventShowcase(MouseEventShowcaseProps::default())
                });
                let mut layout = jadix::core::tree::LayoutTree::new();
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
                let path = jadix::app::pointer_events::hit_path(
                    &tree,
                    b[0] + b[2] * 0.5,
                    b[1] + b[3] * 0.5,
                );
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn mouse_event_pad_hit_path_includes_pad_under_hint_label() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree = jadix::core::reactivity::isolate_reads(|| {
                    MouseEventShowcase(MouseEventShowcaseProps::default())
                });
                let mut layout = jadix::core::tree::LayoutTree::new();
                let _ = layout.build_and_layout(&mut tree, 720.0, 900.0, [0.0, 0.0]);
                let pad = gallery_find_by_focus_id(&tree, "gal-mouse-pad").expect("gal-mouse-pad");
                let b = pad.layout_bounds;
                assert!(
                    b[2] > 8.0 && b[3] > 8.0,
                    "hit pad must have a box, got {b:?}"
                );
                let x = b[0] + b[2] * 0.5;
                let y = b[1] + b[3] * 0.5;
                let path = jadix::app::pointer_events::hit_path(&tree, x, y);
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn mouse_event_lca_center_click_increments_count() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree = jadix::core::reactivity::isolate_reads(|| {
                    MouseEventShowcase(MouseEventShowcaseProps::default())
                });
                let mut layout = jadix::core::tree::LayoutTree::new();
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
                let path = jadix::app::pointer_events::hit_path(&tree, x, y);
                let ids: Vec<&str> = path.iter().filter_map(|e| e.focus_id.as_deref()).collect();
                assert!(
                    ids.contains(&"gal-mouse-lca-target"),
                    "clicking the blue target (including its caption) must hit gal-mouse-lca-target, got {ids:?}"
                );
                let click_path = jadix::app::pointer_events::click_dispatch_path(&path, &path);
                let (cb, _) =
                    jadix::app::pointer_events::resolve_click_from_path(&tree, &click_path);
                (*cb.expect("LCA on_click on press/release path").borrow_mut())();
                let lca = use_signal("gal_mouse_lca_clicks", || 0i32);
                assert_eq!(
                    lca.peek(),
                    1,
                    "clicking the LCA target center must increment click count"
                );
            });
        });
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn mouse_event_log_is_plain_definite_width_leaf() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree = jadix::core::reactivity::isolate_reads(|| {
                    MouseEventShowcase(MouseEventShowcaseProps::default())
                });
                let mut layout = jadix::core::tree::LayoutTree::new();
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn mouse_event_log_panel_reserves_height_before_first_move() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree = jadix::core::reactivity::isolate_reads(|| {
                    MouseEventShowcase(MouseEventShowcaseProps::default())
                });
                let mut layout = jadix::core::tree::LayoutTree::new();
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn forms_section_under_isolate_reads_keeps_mark_invalid_caption() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                // Match App: `<When>` arm builds the section component (isolate_reads inside).
                let mut tree =
                    jadix::core::reactivity::isolate_reads(|| gallery_section_body("forms"));
                fn find_btn(
                    el: &jadix::core::tree::Element,
                ) -> Option<&jadix::core::tree::Element> {
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
                let mut layout = jadix::core::tree::LayoutTree::new();
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn forms_color_picker_variants_tabs_mount_only_active() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let tree = jadix::core::reactivity::isolate_reads(|| gallery_section_body("forms"));
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn picker_showcase_tabs_chips_layout_alone() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree = jadix::core::reactivity::isolate_reads(|| {
                    PickerPlaceholdersShowcase(PickerPlaceholdersShowcaseProps::default())
                });
                let mut layout = jadix::core::tree::LayoutTree::new();
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
                    el: &'a jadix::core::tree::Element,
                    out: &mut Vec<&'a jadix::core::tree::Element>,
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn forms_tabs_chips_after_scroll_into_view_do_not_overlap() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree =
                    jadix::core::reactivity::isolate_reads(|| gallery_section_body("forms"));
                let mut layout = jadix::core::tree::LayoutTree::new();
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
                    &jadix::core::scroll_viewport::scroll_offset_y_signal_key("scroll-forms"),
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn forms_date_time_tabs_chips_do_not_overlap() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree =
                    jadix::core::reactivity::isolate_reads(|| gallery_section_body("forms"));
                let mut layout = jadix::core::tree::LayoutTree::new();
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
                    el: &'a jadix::core::tree::Element,
                    out: &mut Vec<&'a jadix::core::tree::Element>,
                ) {
                    if el.attrs.get("data-slot").map(|s| s.as_str()) == Some("tabs-trigger") {
                        out.push(el);
                    }
                    for c in &el.children {
                        collect_triggers(c, out);
                    }
                }
                fn under_tabs<'a>(
                    el: &'a jadix::core::tree::Element,
                    tabs_id: &str,
                ) -> Option<&'a jadix::core::tree::Element> {
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn forms_date_time_pickers_tabs_mount_only_active() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let tree = jadix::core::reactivity::isolate_reads(|| gallery_section_body("forms"));
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn forms_password_input_adornments_visible_when_focused() {
    jadix_lucide::install().expect("install Lucide icon catalog and font");
    jadix::app::reactive::reset_registered_anchors();
    let mut fs = jadix::renderer::font_loader::create_font_system();
    jadix::renderer::font_loader::apply_default_families(fs.db_mut());
    jadix::text::install_shared_font_system(&mut fs);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let id = "gal-s2-input-pw";
            use_signal(
                &jadix_components::components::input_value_signal_key(id),
                String::new,
            )
            .set("secret1".into());
            use_signal("__jadix_button_focus_id", String::new).set(format!(
                "{}{id}",
                jadix_components::components::INPUT_FOCUS_PREFIX
            ));

            let mut tree = jadix::core::reactivity::isolate_reads(|| gallery_section_body("forms"));
            let pw = gallery_find_by_tid(&tree, id).expect("password input root");
            let clear = gallery_find_by_tid(pw, &format!("__jadix_input:{id}:clear"))
                .expect("clear adornment must mount for clearable=\"true\"");
            let eye = gallery_find_by_tid(pw, &format!("__jadix_input:{id}:reveal"))
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

            let mut layout = jadix::core::tree::LayoutTree::new();
            let result = layout.build_and_layout(&mut tree, 900.0, 800.0, [0.0, 0.0]);
            let pw = gallery_find_by_tid(&tree, id).unwrap();
            for suffix in ["clear", "reveal"] {
                let button =
                    gallery_find_by_tid(pw, &format!("__jadix_input:{id}:{suffix}")).unwrap();
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
                    n.texture_key.starts_with("jadix_lucide_") && n.opacity > 0.01 && inside(n.rect)
                });
                assert!(
                    glyph || texture,
                    "focused password {suffix} must collect a visible icon inside {:?}",
                    button.layout_bounds
                );
            }
        });
    });
    jadix::app::reactive::reset_registered_anchors();
}

fn gallery_collect_input_bounds(
    el: &jadix::core::tree::Element,
    out: &mut Vec<(String, [f32; 4])>,
) {
    if let Some(fid) = el.focus_id.as_deref() {
        if let Some(rest) = fid.strip_prefix(jadix_components::components::INPUT_FOCUS_PREFIX) {
            out.push((rest.to_string(), el.layout_bounds));
        }
    }
    for c in &el.children {
        gallery_collect_input_bounds(c, out);
    }
}

fn gallery_tree_has_input_caret(el: &jadix::core::tree::Element) -> bool {
    el.text_style.input_caret.is_some() || el.children.iter().any(gallery_tree_has_input_caret)
}

/// Gallery forms: wrap Inputs in card「2) Input」used to drop `input_caret` from
/// `text_nodes` once the first field was only a chrome sliver — event loop fell
/// to bare `Wait`. 2b InputAdornment is the control that must still paint a caret
/// under the same scroll.
#[test]
fn forms_input_showcase_keeps_caret_when_first_field_partially_clipped() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                const VW: f32 = 520.0;
                const VH: f32 = 800.0;
                let showcase = "gal-s2-input-a";
                let adornment = "gal-ia-search";
                use_signal(
                    &jadix_components::components::input_value_signal_key(showcase),
                    String::new,
                )
                .set("hello".into());
                use_signal(
                    &jadix_components::components::input_value_signal_key(adornment),
                    String::new,
                )
                .set("query".into());

                #[expect(
                    clippy::type_complexity,
                    reason = "The test probe returns the existing set of independent observations for comparison"
                )]
                fn collect_after_clip(
                    focus_rest: &str,
                    vw: f32,
                    vh: f32,
                ) -> (bool, bool, bool, f32, Vec<(String, [f32; 4])>, usize) {
                    use_signal("__jadix_button_focus_id", String::new).set(format!(
                        "{}{focus_rest}",
                        jadix_components::components::INPUT_FOCUS_PREFIX
                    ));
                    let mut tree =
                        jadix::core::reactivity::isolate_reads(|| gallery_section_body("forms"));
                    let mut layout = jadix::core::tree::LayoutTree::new();
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
                        &jadix::core::scroll_viewport::scroll_offset_y_signal_key("scroll-forms"),
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn forms_section_mounts_wp_r7_reactive_chips() {
    jadix::app::reactive::reset_registered_anchors();
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let tree = jadix::core::reactivity::isolate_reads(|| {
                InputShowcase(InputShowcaseProps::default())
            });
            assert!(
                gallery_find_tid(&tree, "gal-status-badge__component"),
                "能力 ReactiveStatusBadge placeholder"
            );
            assert!(
                gallery_find_tid(&tree, "gal-count-chip__component"),
                "能力 ReactiveCountChip placeholder"
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
    jadix::app::reactive::reset_registered_anchors();
}

#[test]
fn forms_section_mounts_wp39_submit() {
    jadix::app::reactive::reset_registered_anchors();
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let tree = jadix::core::reactivity::isolate_reads(|| {
                FormValidateShowcase(FormValidateShowcaseProps::default())
            });
            assert!(
                gallery_find_tid(&tree, "gal-wp39-submit"),
                "能力 card must mount submit (gal-wp39-submit)"
            );
        });
    });
    jadix::app::reactive::reset_registered_anchors();
}

#[test]
fn data_section_mounts_wp310_masonry() {
    jadix::app::reactive::reset_registered_anchors();
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let tree = jadix::core::reactivity::isolate_reads(|| {
                VirtualMasonryShowcase(VirtualMasonryShowcaseProps::default())
            });
            assert!(
                gallery_find_tid(&tree, GAL_WP310_MASONRY),
                "能力 card must mount masonry (gal-wp310-masonry)"
            );
        });
    });
    jadix::app::reactive::reset_registered_anchors();
}

fn set_scroll_offset(el: &mut jadix::core::tree::Element, rest: &str, offset: [f32; 2]) -> bool {
    let want = format!("scroll:{rest}");
    let mut stack: Vec<*mut jadix::core::tree::Element> = vec![el];
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
    result: &jadix::core::tree::RenderResult,
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
        jadix::app::reactive::reset_registered_anchors();
        let (_handle, tree) = create_root(|| gallery_section_body("data"));
        assert!(
            find_tid(&tree, "gal-timeline-builder__timeline"),
            "data section must mount Builder Timeline (gal-timeline-builder)"
        );
        assert!(
            gallery_tid_contains(&tree, "__timeline"),
            "data section must mount at least one Timeline shell"
        );
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn data_section_scroll_metrics_stable_across_offsets() {
    // Gallery data page: charts (overflow:visible ticks), nested Table/Tree/Masonry
    // scrollports, and CarouselStack. Outer `scroll-data` content size must not
    // breathe as cards enter/leave the clip — that jitters the page thumb.
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree =
                    jadix::core::reactivity::isolate_reads(|| gallery_section_body("data"));
                let mut layout = jadix::core::tree::LayoutTree::new();
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

/// First paint of the data page shows invoice + data_table cells; the next
/// paint-only / hover chrome frame must keep them (gallery flash / empty rows).
#[test]
fn data_section_table_texts_survive_skip_and_hover() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree =
                    jadix::core::reactivity::isolate_reads(|| gallery_section_body("data"));
                let mut layout = jadix::core::tree::LayoutTree::new();
                let first = layout.build_and_layout(&mut tree, 1280.0, 800.0, [-1.0, -1.0]);
                let texts = |r: &jadix::core::tree::RenderResult| {
                    r.text_nodes
                        .iter()
                        .map(|n| n.text.clone())
                        .collect::<Vec<_>>()
                };
                let t0 = texts(&first);
                // Invoice composite is in-tree (not a reactive_view island).
                // data_table cells wait on remount drain — assert those in jadix table_paint.
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
                    fn find_row(
                        el: &jadix::core::tree::Element,
                    ) -> Option<&jadix::core::tree::Element> {
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
                let mut impact = jadix::core::tree::LayoutStatus::Clean;
                jadix::style::sync_class_variants_subtree(
                    &mut tree,
                    mouse,
                    "",
                    false,
                    1280.0,
                    800.0,
                    &mut layout,
                    &mut impact,
                );
                jadix::style::tick_active_css_transitions_subtree(
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn forms_input_showcase_mark_invalid_button_has_caption() {
    jadix::app::reactive::reset_registered_anchors();
    let mut fs = jadix::renderer::font_loader::create_font_system();
    jadix::renderer::font_loader::apply_default_families(fs.db_mut());
    jadix::text::install_shared_font_system(&mut fs);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let mut tree = InputShowcase(InputShowcaseProps::default());
            fn find_caption(
                el: &mut jadix::core::tree::Element,
            ) -> Option<&mut jadix::core::tree::Element> {
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
            let mut layout = jadix::core::tree::LayoutTree::new();
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
    jadix::app::reactive::reset_registered_anchors();
}

#[test]
fn headless_mount_gallery_app_does_not_overflow() {
    jadix::app::reactive::reset_registered_anchors();
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let tree = jadix::authoring::mount::with_inline_mount(build_gallery_app);
            assert!(
                !tree.children.is_empty(),
                "gallery root should have children"
            );
        });
    });
    jadix::app::reactive::reset_registered_anchors();
}

#[test]
fn button_showcase_format_get_lowers_to_bind_text() {
    // Section pages build under `isolate_reads`; without bind_text lowering,
    // `format!(…, count.get())` would bake a static label and never update.
    jadix::app::reactive::reset_registered_anchors();
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let tree = ButtonShowcase(ButtonShowcaseProps::default());
            fn find_bind(el: &jadix::core::tree::Element) -> bool {
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
    jadix::app::reactive::reset_registered_anchors();
}

#[test]
fn forms_section_does_not_crush_sidebar_width() {
    jadix::app::reactive::reset_registered_anchors();
    let mut fs = jadix::renderer::font_loader::create_font_system();
    jadix::renderer::font_loader::apply_default_families(fs.db_mut());
    jadix::text::install_shared_font_system(&mut fs);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            use_signal("gal_nav_section", || "basics".to_string()).set("forms".to_string());
            let mut app = jadix::authoring::mount::with_inline_mount(build_gallery_app);
            let mut layout = jadix::core::tree::LayoutTree::new();
            let _ = layout.build_and_layout(&mut app, 720.0, 700.0, [-1.0, -1.0]);
            let side = gallery_find_by_tid(&app, "gal-sidebar").expect("gal-sidebar");
            let w = side.layout_bounds[2];
            assert!(
                (w - 256.0).abs() < 1.5,
                "forms page must not crush sidebar below 16rem; got {w}"
            );
        });
    });
    jadix::app::reactive::reset_registered_anchors();
}

#[test]
fn overlays_section_effect_flush_completes() {
    // Nested `reactive_anchor` / `<Show when={…get()}>` on this page used to
    // miss `try_reconcile_subtree` (no AppState) and `schedule_effect` into the
    // in-flight deferred flush → `flush_deferred_effects exceeded 100k rounds`.
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let _ = jadix::core::reactivity::isolate_reads(|| {
                    GallerySectionOverlays(GallerySectionOverlaysProps::default())
                });
            });
        });
        jadix::app::reactive::reset_registered_anchors();
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
        jadix::app::reactive::reset_registered_anchors();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut widths = Vec::new();
                for section in ["overlays", "basics", "overlays", "basics", "overlays"] {
                    jadix::app::reactive::reset_registered_anchors();
                    let mut app =
                        jadix::core::reactivity::isolate_reads(|| gallery_section_body(section));
                    let _ = jadix::layout::layout_tree::hoist_portal_subtrees(&mut app);
                    let mut layout = jadix::core::tree::LayoutTree::new();
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn gallery_main_pane_uses_remaining_width() {
    // toast_host must not flex-split the main column (reactive_placeholder is w/h 100%).
    jadix::app::reactive::reset_registered_anchors();
    let mut fs = jadix::renderer::font_loader::create_font_system();
    jadix::renderer::font_loader::apply_default_families(fs.db_mut());
    jadix::text::install_shared_font_system(&mut fs);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            use_signal("gal_nav_section", || "basics".to_string());
            let mut app = jadix::authoring::mount::with_inline_mount(build_gallery_app);
            let mut layout = jadix::core::tree::LayoutTree::new();
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
    jadix::app::reactive::reset_registered_anchors();
}

#[test]
fn gallery_sidebar_collapses_to_icon_rail() {
    jadix::app::reactive::reset_registered_anchors();
    let mut fs = jadix::renderer::font_loader::create_font_system();
    jadix::renderer::font_loader::apply_default_families(fs.db_mut());
    jadix::text::install_shared_font_system(&mut fs);
    let (_handle, _) = create_root(|| {
        create_effect(|| {
            use_signal("gal_nav_section", || "basics".to_string());
            use_signal("gal-shell__open", || false);
            let mut app = jadix::authoring::mount::with_inline_mount(build_gallery_app);
            let mut layout = jadix::core::tree::LayoutTree::new();
            let _ = layout.build_and_layout(&mut app, 1000.0, 700.0, [-1.0, -1.0]);
            let side = gallery_find_by_tid(&app, "gal-sidebar").expect("sidebar");
            let w = side.layout_bounds[2];
            assert!(
                (w - 48.0).abs() < 1.5,
                "SidebarTrigger collapse must keep a 3rem icon rail; got {w}"
            );
        });
    });
    jadix::app::reactive::reset_registered_anchors();
}

#[test]
fn basics_section_scrolls_when_viewport_shorter() {
    // Basics section content (~800px) must enable Y scroll when the ScrollArea
    // viewport is shorter — same failure mode as gallery after a window-height shrink
    // once surface size tracks Resized (see AppState::resolve_surface_size).
    run_on_gallery_build_stack(|| {
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);

        jadix::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut body = gallery_section_body("basics");
                let mut layout = jadix::core::tree::LayoutTree::new();
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn headless_layout_basics_section_does_not_overflow() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree = gallery_section_body("basics");
                let mut layout = jadix::core::tree::LayoutTree::new();
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
        jadix::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree = gallery_section_body("basics");
                let mut layout = jadix::core::tree::LayoutTree::new();
                let _ = layout.build_and_layout(&mut tree, 1280.0, 800.0, [0.0, 0.0]);
            });
        });
        jadix::app::reactive::reset_registered_anchors();
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

/// 1 MiB thread (Windows PE default) + framework [`jadix::app::with_ui_build_stack`].
fn run_on_gallery_build_stack<F>(f: F)
where
    F: FnOnce() + Send + 'static,
{
    run_on_pe_stack(|| jadix::app::with_ui_build_stack(f));
}

#[test]
fn headless_layout_all_sections_does_not_overflow() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                for entry in GALLERY_SECTIONS {
                    let mut tree = gallery_section_body(entry.key);
                    let mut layout = jadix::core::tree::LayoutTree::new();
                    let _ = layout.build_and_layout(&mut tree, 1280.0, 800.0, [0.0, 0.0]);
                }
            });
        });
    });
}

fn gallery_section_scroll_id(key: &str) -> String {
    format!("scroll-{key}")
}

/// Page ScrollArea uses `overflow_x: auto`. Wrapping / 100% cards must not enable
/// a horizontal thumb — the live Gallery bug after switching sidebar sections.
#[test]
fn gallery_section_pages_must_not_enable_x_scroll() {
    fn overflow_bounds(root: &Element, width: f32, output: &mut Vec<String>) {
        let [left, _, size, _] = root.layout_bounds;
        if left + size > width + 0.5 {
            output.push(format!(
                "{:?} {:?} {:?} {:?}",
                root.template_id,
                root.focus_id,
                root.attrs.get("data-slot"),
                root.layout_bounds
            ));
        }
        for child in &root.children {
            overflow_bounds(child, width, output);
        }
    }
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                for entry in GALLERY_SECTIONS {
                    let key = entry.key;
                    jadix::app::reactive::reset_registered_anchors();
                    let rest = gallery_section_scroll_id(key);
                    let mut tree = gallery_section_body(key);
                    let mut layout = jadix::core::tree::LayoutTree::new();
                    for &(w, h) in &[(640.0, 700.0), (800.0, 700.0), (900.0, 700.0)] {
                        let result = layout.build_and_layout(&mut tree, w, h, [-1.0, -1.0]);
                        let (vp, max, content) = scroll_metrics_xy(&result, &rest);
                        let mut overflow = Vec::new();
                        if max[0] >= 0.5 {
                            overflow_bounds(&tree, w, &mut overflow);
                        }
                        assert!(
                            max[0] < 0.5,
                            "{key} at {w}x{h} must not enable horizontal scroll \
                             (content_w={:.1} vp_w={:.1} max_x={:.1}); overflowing nodes={:?}",
                            content[0],
                            vp[2],
                            max[0],
                            overflow
                        );
                    }
                }
            });
        });
        jadix::app::reactive::reset_registered_anchors();
    });
}

fn find_tid(el: &jadix::core::tree::Element, want: &str) -> bool {
    gallery_find_tid(el, want)
}

fn gallery_find_attr(el: &jadix::core::tree::Element, key: &str, value: &str) -> bool {
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
        jadix::app::reactive::reset_registered_anchors();
        let (_handle, tree) = create_root(|| gallery_section_body("theme"));
        assert!(
            find_tid(&tree, "gal-theme-apply") || gallery_find_attr(&tree, "id", "gal-theme-apply"),
            "theme section must mount apply theme.toml"
        );
        assert!(
            find_tid(&tree, "gal-theme-sheet") || gallery_find_attr(&tree, "id", "gal-theme-sheet"),
            "theme section must mount apply styles.toml"
        );
        for control in [
            "gal-theme-preset-shadcn",
            "gal-theme-preset-fluent",
            "gal-theme-preset-material3",
            "gal-theme-preset-liquid-glass",
            "gal-theme-density",
            "gal-theme-radius",
            "gal-theme-refraction",
            "gal-theme-dispersion",
            "gal-theme-opacity",
            "gal-theme-preview-input",
            "gal-theme-preview-popover",
            "gal-theme-preview-dialog",
            "gal-theme-group-top",
            "gal-theme-group-middle",
            "gal-theme-group-bottom",
        ] {
            assert!(
                find_tid(&tree, control),
                "theme studio must mount {control}"
            );
        }
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn theme_section_switches_all_presets_and_keeps_form_state() {
    run_on_gallery_build_stack(|| {
        let overrides = jadix::style::theme_overrides();
        let previous_mode = jadix::style::theme_mode();
        create_root(|| {
            let checked = use_signal("gal_theme_preview_checked", || true);
            checked.set(false);
            for mode in [
                jadix::style::ThemeMode::Light,
                jadix::style::ThemeMode::Dark,
            ] {
                jadix::style::set_theme_mode(mode);
                for preset in jadix::style::ThemePreset::ALL {
                    let tree = ThemeStyleShowcase(ThemeStyleShowcaseProps::default());
                    let id = format!("gal-theme-preset-{}", preset.name());
                    let button = gallery_find_by_tid(&tree, &id).expect("preset button");
                    (button.on_click.as_ref().unwrap().borrow_mut())();
                    assert_eq!(jadix::style::theme_preset(), preset);
                    assert!(!checked.peek());
                    let tree = ThemeStyleShowcase(ThemeStyleShowcaseProps::default());
                    assert!(find_tid(&tree, "gal-theme-preview-primary"));
                    assert!(
                        find_tid(&tree, "gal-theme-design-note")
                            || gallery_find_attr(&tree, "id", "gal-theme-design-note")
                    );
                }
            }
        });
        jadix::style::install_theme_overrides(overrides).unwrap();
        jadix::style::set_theme_mode(previous_mode);
    });
}

#[test]
fn themed_buttons_and_motion_labels_keep_their_first_layout_geometry() {
    run_on_gallery_build_stack(|| {
        let previous = jadix::style::get_theme();
        let overrides = jadix::style::theme_overrides();
        let mut fonts = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fonts.db_mut());
        jadix::text::install_shared_font_system(&mut fonts);
        for preset in [
            jadix::style::ThemePreset::Fluent,
            jadix::style::ThemePreset::LiquidGlass,
        ] {
            jadix::style::set_theme_preset(preset);
            for motion in [false, true] {
                let (owner, mut tree) = create_root(|| {
                    if motion {
                        crate::pages::motion::build_layout_motion_showcase()
                    } else {
                        ThemeStyleShowcase(Default::default())
                    }
                });
                let mut layout = jadix::layout::LayoutTree::new();
                let first = layout.build_and_layout(&mut tree, 1280.0, 1600.0, [-1.0; 2]);
                let next = layout.build_and_layout(&mut tree, 1280.0, 1600.0, [-1.0; 2]);
                let needles: &[&str] = if motion {
                    &["Card 1", "Card 2", "Card 3"]
                } else {
                    &["Fluent", "Liquid Glass"]
                };
                for needle in needles {
                    let initial = first
                        .text_nodes
                        .iter()
                        .find(|text| text.text.contains(needle))
                        .unwrap_or_else(|| panic!("{preset:?} first frame misses {needle}"));
                    let settled = next
                        .text_nodes
                        .iter()
                        .find(|text| text.text == initial.text)
                        .unwrap();
                    assert!(
                        initial.width > 0.0
                            && initial.height > 0.0
                            && initial.style.color_resolved()[3] > 0,
                        "{needle} must have visible ink"
                    );
                    assert!(
                        (initial.x - settled.x).abs() < 0.1 && (initial.y - settled.y).abs() < 0.1,
                        "{preset:?}/{needle} first frame ({},{}) differs from ({},{})",
                        initial.x,
                        initial.y,
                        settled.x,
                        settled.y
                    );
                }
                owner.dispose();
                jadix::app::reactive::reset_registered_anchors();
            }
        }
        jadix::style::install_theme_overrides(overrides).unwrap();
        jadix::style::set_theme(previous);
    });
}

#[test]
fn theme_section_glass_covers_controls_and_supports_opaque_fallback() {
    run_on_gallery_build_stack(|| {
        let overrides = jadix::style::theme_overrides();
        let previous_mode = jadix::style::theme_mode();
        jadix::style::install_theme_overrides(
            jadix::style::parse_theme_overrides_str("preset = 'liquid-glass'").unwrap(),
        )
        .unwrap();
        create_root(|| {
            for mode in [
                jadix::style::ThemeMode::Light,
                jadix::style::ThemeMode::Dark,
            ] {
                jadix::style::set_theme_mode(mode);
                let tree = ThemeStyleShowcase(ThemeStyleShowcaseProps::default());
                let content = gallery_find_slot(&tree, "card").unwrap();
                assert!(content.style.glass.is_some());
                assert!(content.style.bg_color[3] < 1.0);
                let mut stack = vec![&tree];
                let mut glass_count = 0;
                while let Some(element) = stack.pop() {
                    glass_count += usize::from(element.style.glass.is_some());
                    stack.extend(element.children.iter());
                }
                assert!(
                    glass_count > 10,
                    "cards and ordinary controls share the optical material"
                );
                assert!(gallery_find_by_tid(&tree, "gal-theme-optical-stage").is_some());
                assert!(gallery_find_by_tid(&tree, "gal-theme-optical-grid").is_some());
                let toggle = gallery_find_by_tid(&tree, "gal-theme-opacity").unwrap();
                (toggle.on_click.as_ref().unwrap().borrow_mut())();
                let opaque = ThemeStyleShowcase(ThemeStyleShowcaseProps::default());
                let mut stack = vec![&opaque];
                while let Some(element) = stack.pop() {
                    assert!(element.style.glass.is_none());
                    assert_eq!(element.style.backdrop_blur, 0.0);
                    stack.extend(element.children.iter());
                }
                let toggle = gallery_find_by_tid(&opaque, "gal-theme-opacity").unwrap();
                (toggle.on_click.as_ref().unwrap().borrow_mut())();
            }
        });
        jadix::style::install_theme_overrides(overrides).unwrap();
        jadix::style::set_theme_mode(previous_mode);
    });
}

#[test]
fn theme_section_switches_light_dark_and_system_preferences() {
    use jadix::style::{ThemeMode, ThemePreference};
    run_on_gallery_build_stack(|| {
        let previous_theme = jadix::style::get_theme();
        let previous_preference = jadix::style::theme_preference();
        create_root(|| {
            for (name, preference, mode) in [
                ("light", ThemePreference::Light, Some(ThemeMode::Light)),
                ("dark", ThemePreference::Dark, Some(ThemeMode::Dark)),
                ("system", ThemePreference::System, None),
            ] {
                let tree = ThemeStyleShowcase(Default::default());
                let button = gallery_find_by_tid(&tree, &format!("gal-theme-mode-{name}")).unwrap();
                (button.on_click.as_ref().unwrap().borrow_mut())();
                assert_eq!(jadix::style::theme_preference(), preference);
                if let Some(mode) = mode {
                    assert_eq!(jadix::style::theme_mode(), mode);
                }
                let updated = ThemeStyleShowcase(Default::default());
                let selected =
                    gallery_find_by_tid(&updated, &format!("gal-theme-mode-{name}")).unwrap();
                assert_eq!(
                    selected.attrs.get("data-variant").map(String::as_str),
                    Some("secondary")
                );
            }
        });
        jadix::style::set_theme_preference(previous_preference);
        jadix::style::set_theme(previous_theme);
    });
}

#[test]
fn text_section_mounts_msdf_card() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn scene_section_mounts_billboard() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let (_handle, tree) = create_root(|| gallery_section_body("scene"));
        assert!(
            find_tid(&tree, "gal-billboard-default"),
            "draw section must mount default-pitch billboard"
        );
        assert!(
            find_tid(&tree, "gal-billboard-pitch"),
            "draw section must mount steep-pitch billboard"
        );
        assert!(
            gallery_find_attr(&tree, "jadix_scene3d_label", "永角"),
            "steep billboard must carry CJK MSDF label"
        );
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn desktop_section_mounts_window_and_shell_controls() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let (_handle, tree) = create_root(|| gallery_section_body("desktop"));
        for id in [
            "gal-desktop-fullscreen-toggle",
            "gal-desktop-fullscreen-exit",
            "gal-desktop-fullscreen-refresh",
            "gal-desktop-fullscreen-shortcuts",
            "gal-desktop-instance-single",
            "gal-desktop-instance-profile",
            "gal-desktop-instance-multiple",
            "gal-desktop-window-dependent",
            "gal-desktop-window-independent",
            "gal-desktop-window-topmost",
        ] {
            assert!(find_tid(&tree, id), "navigation must mount {id}");
        }
        assert!(
            find_tid(&tree, GAL_SHELL_TRAY_SHOW),
            "能力: navigation section must mount tray Show button"
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn desktop_demo_arguments_select_only_explicit_instance_modes() {
    for (args, expected) in [
        (vec![], None),
        (vec!["--gallery-instance-demo=invalid"], None),
        (vec!["single"], None),
        (
            vec!["--other", "--gallery-instance-demo=single"],
            Some("single"),
        ),
        (vec!["--gallery-instance-demo=profile"], Some("profile")),
        (vec!["--gallery-instance-demo=multiple"], Some("multiple")),
    ] {
        assert_eq!(
            instance_demo_mode(args.into_iter().map(std::ffi::OsString::from)),
            expected
        );
    }
}

#[test]
fn desktop_demo_second_launch_updates_count_and_preserves_payload() {
    run_on_gallery_build_stack(|| {
        let working_directory = std::path::PathBuf::from("C:/Gallery 演示目录");
        let (_owner, tree) = create_root(gallery_instance_demo_root);
        assert!(find_tid(&tree, "gal-desktop-primary-close"));
        for count in 1..=2 {
            record_second_instance(jadix::app::InstanceLaunch {
                args: vec!["gallery.exe".into(), format!("文档 {count}.txt").into()],
                working_directory: working_directory.clone(),
            });
            let (_owner, tree) = create_root(gallery_instance_demo_root);
            assert!(gallery_subtree_has_text(
                &tree,
                &format!("收到重复启动：{count} 次")
            ));
            assert!(gallery_subtree_has_text(
                &tree,
                &format!("文档 {count}.txt")
            ));
            assert!(gallery_subtree_has_text(
                &tree,
                working_directory.to_str().unwrap()
            ));
        }
    });
}

#[test]
fn desktop_demo_secondary_mounts_controls_and_preserves_initial_policy() {
    for (independent, on_top) in [(false, false), (true, false), (true, true)] {
        run_on_gallery_build_stack(move || {
            let (_owner, tree) = create_root(|| gallery_desktop_window_root(independent, on_top));
            for id in [
                "gal-desktop-window-close",
                "gal-desktop-window-quit",
                "gal-desktop-window-independent",
                "gal-desktop-fullscreen-toggle",
            ] {
                assert!(find_tid(&tree, id), "secondary must mount {id}");
            }
            let follow = gallery_find_by_tid(&tree, "gal-desktop-window-follow").unwrap();
            assert!(gallery_subtree_has_text(
                follow,
                if independent {
                    "独立存活"
                } else {
                    "随主窗关闭"
                }
            ));
            let priority = gallery_find_by_tid(&tree, "gal-desktop-window-priority").unwrap();
            assert!(gallery_subtree_has_text(
                priority,
                if on_top {
                    "取消全局置顶"
                } else {
                    "启用全局置顶"
                }
            ));
        });
    }
}

fn subtree_nodes_with_class_util(el: &jadix::core::tree::Element, util: &str) -> usize {
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
        jadix::app::reactive::reset_registered_anchors();
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn full_app_mount_and_layout_fits_pe_stack() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut app = build_gallery_app();
                let mut layout = jadix::core::tree::LayoutTree::new();
                let _ = layout.build_and_layout(&mut app, 1280.0, 800.0, [0.0, 0.0]);
            });
        });
        jadix::app::reactive::reset_registered_anchors();
    });
}

/// Full gallery shell + default section: 1 MiB PE thread, no `maybe_grow`.
#[test]
fn full_app_mount_and_layout_fits_pe_stack_without_grow() {
    run_on_pe_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut app = build_gallery_app();
                let mut layout = jadix::core::tree::LayoutTree::new();
                let _ = layout.build_and_layout(&mut app, 1280.0, 800.0, [0.0, 0.0]);
            });
        });
        jadix::app::reactive::reset_registered_anchors();
    });
}

#[test]
fn button_showcase_fits_pe_stack() {
    run_on_pe_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree = ButtonShowcase(Default::default());
                let mut layout = jadix::core::tree::LayoutTree::new();
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
        jadix::app::reactive::reset_registered_anchors();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut root = jadix::core::reactivity::isolate_reads(|| {
                    GallerySectionOverlays(GallerySectionOverlaysProps::default())
                });
                assert!(
                    jadix::layout::layout_tree::hoist_portal_subtrees(&mut root) >= 1,
                    "local Portal must hoist onto gal-local-scrim-host"
                );
                let mut layout = jadix::core::tree::LayoutTree::new();
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

fn instance_fill_mpx(instances: &[jadix::renderer::InstanceInput], vw: f32, vh: f32) -> f32 {
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
        jadix::app::reactive::reset_registered_anchors();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                for section in ["basics", "layout", "draw"] {
                    jadix::app::reactive::reset_registered_anchors();
                    let mut app =
                        jadix::core::reactivity::isolate_reads(|| gallery_section_body(section));
                    let _ = jadix::layout::layout_tree::hoist_portal_subtrees(&mut app);
                    let mut layout = jadix::core::tree::LayoutTree::new();
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

/// Scan viewport-sized slices so expensive content cannot hide below the fold.
#[test]
#[ignore = "performance scan; run explicitly with --ignored --nocapture"]
fn gallery_scroll_segment_cost() {
    run_on_gallery_build_stack(|| {
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
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
                    jadix::app::reactive::reset_registered_anchors();
                    let mut root =
                        jadix::core::reactivity::isolate_reads(|| gallery_section_body(section));
                    let mut tree = jadix::core::tree::LayoutTree::new();
                    let first = tree.build_and_layout(&mut root, 1920.0, 1080.0, [-1.0; 2]);
                    let Some((rest, _, _, content)) = first
                        .scroll_metrics
                        .iter()
                        .find(|(rest, _, _, _)| rest == &format!("scroll-{section}"))
                        .cloned()
                    else {
                        continue;
                    };
                    let offset = jadix::core::reactivity::use_signal(
                        &jadix::core::scroll_viewport::scroll_offset_y_signal_key(&rest),
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
                        eprintln!(
                            "scroll/{section} y={y:.0}/{max:.0}: wall={:.2}ms collect={:.2} stamp={:.2} inst={} text={}",
                            times[1], stats.0, stats.1, stats.2, stats.3
                        );
                        assert!(
                            times[1] < 10.0,
                            "gallery/{section} scroll segment y={y:.0} costs {:.2}ms before GPU work",
                            times[1]
                        );
                    }
                }
            });
        });
        jadix::app::reactive::reset_registered_anchors();
    });
}

/// Clicking the RSX ViewportModal trigger must not eat the card's VStack gap
/// (button label toggles 打开→关闭 while DialogFade · RSX / SheetSlide stay put).
#[test]
fn viewport_modal_rsx_open_keeps_card_vstack_gap() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        jadix::app::portal_overlay::clear_overlay_stack();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::renderer::font_loader::ensure_coverage_for_text(
            &mut fs,
            "打开 RSX ViewportModal 关闭 RSX 模态 视口舞台 presence DialogFade SheetSlide",
        );
        jadix::text::install_shared_font_system(&mut fs);

        let text_y = |result: &jadix::layout::layout_tree::RenderResult, needle: &str| -> f32 {
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
            jadix::app::reactive::reset_registered_anchors();
            let mut tree = ViewportModalShowcase(ViewportModalShowcaseProps::default());
            let _ = jadix::layout::layout_tree::hoist_portal_subtrees(&mut tree);
            let mut layout = jadix::core::tree::LayoutTree::new();
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

        jadix::app::portal_overlay::clear_overlay_stack();
        jadix::app::reactive::reset_registered_anchors();
    });
}

/// Live Gallery chat card: Latest jump control must stay centered inside the
/// 24rem card, not clip on the left overflow edge.
#[test]
fn message_scroller_latest_button_stays_inside_chat_card() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut root = jadix::core::reactivity::isolate_reads(|| {
                    MessageScrollerShowcase(MessageScrollerShowcaseProps::default())
                });
                let mut layout = jadix::core::tree::LayoutTree::new();
                for _ in 0..3 {
                    let _ = layout.build_and_layout(&mut root, 420.0, 900.0, [-1.0, -1.0]);
                }
                use_signal(
                    &jadix::core::scroll_viewport::scroll_offset_y_signal_key(
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

fn gallery_subtree_has_text(el: &jadix::core::tree::Element, needle: &str) -> bool {
    if el.text.as_deref().is_some_and(|t| t.contains(needle)) {
        return true;
    }
    el.children
        .iter()
        .any(|c| gallery_subtree_has_text(c, needle))
}

fn gallery_find_slot_with_text<'a>(
    el: &'a jadix::core::tree::Element,
    slot: &str,
    needle: &str,
) -> Option<&'a jadix::core::tree::Element> {
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
        jadix::app::reactive::reset_registered_anchors();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree = jadix::core::reactivity::isolate_reads(|| {
                    AttachmentShowcase(AttachmentShowcaseProps::default())
                });
                let mut layout = jadix::core::tree::LayoutTree::new();
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
        jadix::app::reactive::reset_registered_anchors();
    });
}

/// Gallery 基础「Status (Marker inside Message)」: shimmer caption must keep the
/// full 「Oliver is typing...」 ink. Nested Message `min-w-0` used to clip it to
/// 「Oliver i」 while dest-reset flashed the stub.
#[test]
fn basics_oliver_is_typing_shimmer_must_keep_full_ink() {
    run_on_gallery_build_stack(|| {
        jadix::app::reactive::reset_registered_anchors();
        let mut fs = jadix::renderer::font_loader::create_font_system();
        jadix::renderer::font_loader::apply_default_families(fs.db_mut());
        jadix::text::install_shared_font_system(&mut fs);
        let (_handle, _) = create_root(|| {
            create_effect(|| {
                let mut tree = jadix::core::reactivity::isolate_reads(|| {
                    MessageShowcase(MessageShowcaseProps::default())
                });
                let mut layout = jadix::core::tree::LayoutTree::new();
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
                        .map(jadix::text::intrinsic_width)
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
        jadix::app::reactive::reset_registered_anchors();
    });
}
