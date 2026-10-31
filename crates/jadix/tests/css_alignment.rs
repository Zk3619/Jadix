use jadix::core::tree::Element;
use jadix::style::{apply_class_list, TextAlign, TextAlignLast};
use taffy::style::{AlignContent, AlignItems, AlignmentSafety};

#[test]
fn invalid_alignment_values_preserve_previous_authored_values() {
    for invalid in [
        "",
        "unknown",
        "safe stretch",
        "safe baseline",
        "center trailing",
        "revert",
        "revert-layer",
    ] {
        let element = Element::new()
            .justify("center")
            .justify(invalid)
            .flex_items("center")
            .flex_items(invalid)
            .align_content_kw("center")
            .align_content_kw(invalid)
            .align_self_kw("center")
            .align_self_kw(invalid)
            .justify_items_kw("center")
            .justify_items_kw(invalid)
            .justify_self_kw("center")
            .justify_self_kw(invalid)
            .text_align("center")
            .text_align(invalid)
            .text_align_last("center")
            .text_align_last(invalid);
        assert_eq!(
            element.style.justify_content,
            Some(AlignContent::CENTER),
            "{invalid}"
        );
        assert_eq!(
            element.style.align_content,
            Some(AlignContent::CENTER),
            "{invalid}"
        );
        assert_eq!(
            element.style.align_items,
            Some(AlignItems::CENTER),
            "{invalid}"
        );
        assert_eq!(
            element.style.align_self,
            Some(AlignItems::CENTER),
            "{invalid}"
        );
        assert_eq!(
            element.style.justify_items,
            Some(AlignItems::CENTER),
            "{invalid}"
        );
        assert_eq!(
            element.style.justify_self,
            Some(AlignItems::CENTER),
            "{invalid}"
        );
        assert_eq!(
            element.text_style.align,
            Some(TextAlign::Center),
            "{invalid}"
        );
        assert_eq!(
            element.text_style.align_last,
            Some(TextAlignLast::Center),
            "{invalid}"
        );
    }
    let invalid = String::from("invalid");
    assert_eq!(
        Element::new()
            .justify("center")
            .justify(invalid.clone())
            .style
            .justify_content,
        Some(AlignContent::CENTER)
    );
    assert_eq!(
        Element::new()
            .flex_items("center")
            .flex_items(&invalid)
            .style
            .align_items,
        Some(AlignItems::CENTER)
    );
}

#[test]
fn text_baseline_alignment_uses_glyph_metrics_and_box_insets() {
    use glyphon::{Attrs, Buffer, FontSystem, Metrics, Shaping};
    let mut fonts =
        FontSystem::new_with_locale_and_db("en-US".into(), glyphon::fontdb::Database::new());
    fonts.db_mut().load_font_data(
        include_bytes!("../../../crates/jadix_cosmic_text/fonts/FiraMono-Medium.ttf").to_vec(),
    );
    let mut first = Buffer::new(&mut fonts, Metrics::new(16.0, 30.0));
    first.set_text(
        "small",
        &Attrs::new().family(glyphon::Family::Name("Fira Mono")),
        Shaping::Basic,
        None,
    );
    first.shape_until_scroll(&mut fonts, false);
    let small_baseline = first.layout_runs().next().expect("font run").line_y;
    let mut second = Buffer::new(&mut fonts, Metrics::new(32.0, 44.0));
    second.set_text(
        "large",
        &Attrs::new().family(glyphon::Family::Name("Fira Mono")),
        Shaping::Basic,
        None,
    );
    second.shape_until_scroll(&mut fonts, false);
    let large_baseline = second.layout_runs().next().expect("font run").line_y;
    jadix::text::install_shared_font_system(&mut fonts);
    let mut root = Element::new()
        .flex_items("baseline")
        .w(300.0)
        .h(100.0)
        .children([
            Element::new()
                .text("small")
                .font_family("Fira Mono")
                .text_size(16.0)
                .text_line_height(30.0)
                .pt(5.0),
            Element::new()
                .text("large")
                .font_family("Fira Mono")
                .text_size(32.0)
                .text_line_height(44.0)
                .pt(9.0),
        ]);
    let mut tree = jadix::layout::layout_tree::LayoutTree::new();
    tree.build_and_layout(&mut root, 300.0, 100.0, [-1.0; 2]);
    let small_bounds = root.children[0].layout_bounds;
    let large_bounds = root.children[1].layout_bounds;
    jadix::text::clear_shared_font_system();
    let small_world = small_bounds[1] + 5.0 + small_baseline;
    let large_world = large_bounds[1] + 9.0 + large_baseline;
    assert!(
        (small_world - large_world).abs() < 0.05,
        "glyph baselines: {small_world} vs {large_world}; boxes {small_bounds:?} {large_bounds:?}"
    );
    assert!(
        (small_bounds[1] + small_bounds[3] - large_bounds[1] - large_bounds[3]).abs() > 0.1,
        "baseline alignment must not fall back to box bottoms"
    );
}

#[test]
fn authored_text_alignment_survives_state_and_viewport_variants() {
    use jadix::core::tree::LayoutStatus;
    use jadix::style::{resolve_class_variants, VariantContext};
    let mut element = Element::new()
        .class("hover:bg-red-500 md:text-blue-500")
        .text_align("right")
        .text_align_last("center");
    for (hovered, viewport_w) in [(false, 320.0), (true, 768.0), (false, 767.5), (true, 639.5)] {
        let context = VariantContext {
            hovered,
            viewport_w,
            ..Default::default()
        };
        resolve_class_variants(&mut element, &context);
        assert_eq!(element.text_style.align, Some(TextAlign::Right));
        assert_eq!(element.text_style.align_last, Some(TextAlignLast::Center));
    }
    assert_eq!(element.set_text_align("left"), LayoutStatus::LayoutChanged);
    assert_eq!(
        element.set_text_align_last("right"),
        LayoutStatus::LayoutChanged
    );
    resolve_class_variants(&mut element, &VariantContext::default());
    assert_eq!(element.text_style.align, Some(TextAlign::Left));
    assert_eq!(element.text_style.align_last, Some(TextAlignLast::Right));
    assert_eq!(element.set_text_align("left"), LayoutStatus::Clean);
    assert_eq!(element.set_text_align_last("right"), LayoutStatus::Clean);
}

#[test]
fn unchanged_last_alignment_does_not_promote_color_changes_to_layout() {
    use jadix::core::tree::LayoutStatus;
    let mut element = Element::new().class("text-last-center bg-red-500");
    assert_eq!(
        apply_class_list(&mut element, "text-last-center bg-blue-500"),
        LayoutStatus::StyleChanged
    );
    assert_eq!(
        apply_class_list(&mut element, "text-last-center bg-blue-500"),
        LayoutStatus::Clean
    );
    assert_eq!(
        apply_class_list(&mut element, "text-last-right bg-blue-500"),
        LayoutStatus::LayoutChanged
    );
}

#[test]
fn match_parent_recomputes_on_the_same_retained_tree() {
    use jadix::style::TextDirection;
    let mut inherited = Element::new()
        .text_align("match-parent")
        .child(Element::new().text("grandchild"));
    inherited.template_id = Some("retained-match-parent".into());
    inherited.focus_id = Some("stable-focus".into());
    let mut root = Element::new().text_align("left").children([
        inherited,
        Element::new().text("explicit").text_align("center"),
    ]);
    let mut tree = jadix::layout::layout_tree::LayoutTree::new();
    for (align, direction, expected) in [
        ("left", TextDirection::Ltr, TextAlign::Left),
        ("right", TextDirection::Ltr, TextAlign::Right),
        ("start", TextDirection::Rtl, TextAlign::Right),
        ("end", TextDirection::Rtl, TextAlign::Left),
        ("start", TextDirection::Ltr, TextAlign::Left),
    ] {
        root.set_text_align(align);
        root.text_style.direction = direction;
        tree.build_and_layout(&mut root, 200.0, 120.0, [-1.0; 2]);
        assert_eq!(
            root.children[0].text_style.align_resolved(),
            expected,
            "{align} {direction:?}"
        );
        assert_eq!(
            root.children[0].children[0].text_style.align_resolved(),
            expected
        );
        assert_eq!(
            root.children[1].text_style.align_resolved(),
            TextAlign::Center
        );
        assert_eq!(
            root.children[0].template_id.as_deref(),
            Some("retained-match-parent")
        );
        assert_eq!(root.children[0].focus_id.as_deref(), Some("stable-focus"));
    }
    root.children[0].set_text_align("left");
    root.set_text_align("right");
    tree.build_and_layout(&mut root, 200.0, 120.0, [-1.0; 2]);
    assert_eq!(
        root.children[0].text_style.align_resolved(),
        TextAlign::Left
    );
}

#[test]
fn match_parent_survives_paint_variants_and_restores_after_alignment_variants() {
    use jadix::core::tree::LayoutStatus;
    use jadix::style::{resolve_class_variants, VariantContext};
    let mut root = Element::new().text_align("right").children([
        Element::new()
            .class("hover:bg-red-500")
            .text_align("match-parent")
            .text("paint"),
        Element::new()
            .class("hover:text-left")
            .text_align("match-parent")
            .text("alignment"),
    ]);
    let mut tree = jadix::layout::layout_tree::LayoutTree::new();
    tree.build_and_layout(&mut root, 200.0, 120.0, [-1.0; 2]);
    let hovered = VariantContext {
        hovered: true,
        ..Default::default()
    };
    assert_eq!(
        resolve_class_variants(&mut root.children[0], &hovered),
        LayoutStatus::StyleChanged
    );
    assert_eq!(
        root.children[0].text_style.align,
        Some(TextAlign::MatchParent)
    );
    assert_eq!(
        root.children[0].text_style.align_resolved(),
        TextAlign::Right
    );
    assert_eq!(
        resolve_class_variants(&mut root.children[1], &hovered),
        LayoutStatus::LayoutChanged
    );
    assert_eq!(
        root.children[1].text_style.align_resolved(),
        TextAlign::Left
    );
    assert_eq!(
        resolve_class_variants(&mut root.children[1], &VariantContext::default()),
        LayoutStatus::LayoutChanged
    );
    tree.build_and_layout(&mut root, 200.0, 120.0, [-1.0; 2]);
    assert_eq!(
        root.children[1].text_style.align,
        Some(TextAlign::MatchParent)
    );
    assert_eq!(
        root.children[1].text_style.align_resolved(),
        TextAlign::Right
    );
}

#[test]
fn alignment_builders_preserve_css_positions_and_safety() {
    for (keyword, expected) in [
        ("normal", AlignItems::NORMAL),
        ("start", AlignItems::START),
        ("flex-start", AlignItems::FLEX_START),
        ("self-end", AlignItems::SELF_END),
        ("first baseline", AlignItems::BASELINE),
        ("safe center", AlignItems::SAFE_CENTER),
        (
            " UNSAFE\tSELF-START ",
            AlignItems::SELF_START.with_safety(AlignmentSafety::Unsafe),
        ),
    ] {
        let element = Element::new()
            .flex_items(keyword)
            .align_self_kw(keyword)
            .justify_items_kw(keyword)
            .justify_self_kw(keyword);
        assert_eq!(element.style.align_items, Some(expected), "{keyword}");
        assert_eq!(element.style.align_self, Some(expected), "{keyword}");
        assert_eq!(element.style.justify_items, Some(expected), "{keyword}");
        assert_eq!(element.style.justify_self, Some(expected), "{keyword}");
    }
    for (keyword, expected) in [
        ("normal", AlignContent::NORMAL),
        ("start", AlignContent::START),
        ("flex-start", AlignContent::FLEX_START),
        ("stretch", AlignContent::STRETCH),
        ("safe end", AlignContent::SAFE_END),
        ("space-evenly", AlignContent::SPACE_EVENLY),
        (
            "unsafe center",
            AlignContent::CENTER.with_safety(AlignmentSafety::Unsafe),
        ),
    ] {
        let element = Element::new().justify(keyword).align_content_kw(keyword);
        assert_eq!(element.style.justify_content, Some(expected), "{keyword}");
        assert_eq!(element.style.align_content, Some(expected), "{keyword}");
    }
    let element = Element::new().align_self_kw("auto").justify_self_kw("auto");
    assert_eq!(element.style.align_self, None);
    assert_eq!(element.style.justify_self, None);
}

#[test]
fn alignment_utilities_reach_all_eight_properties() {
    let mut element = Element::new();
    apply_class_list(&mut element, "items-safe-center content-evenly self-end justify-between justify-items-self-start justify-self-unsafe-end text-justify text-last-center");
    assert_eq!(element.style.align_items, Some(AlignItems::SAFE_CENTER));
    assert_eq!(
        element.style.align_content,
        Some(AlignContent::SPACE_EVENLY)
    );
    assert_eq!(element.style.align_self, Some(AlignItems::FLEX_END));
    assert_eq!(
        element.style.justify_content,
        Some(AlignContent::SPACE_BETWEEN)
    );
    assert_eq!(element.style.justify_items, Some(AlignItems::SELF_START));
    assert_eq!(
        element.style.justify_self,
        Some(AlignItems::END.with_safety(AlignmentSafety::Unsafe))
    );
    assert_eq!(element.text_style.align, Some(TextAlign::Justify));
    assert_eq!(element.text_style.align_last, Some(TextAlignLast::Center));
}

fn child_position(keyword: &str, reverse: bool, container_width: f32, child_width: f32) -> f32 {
    use taffy::prelude::*;
    let mut parent = Element::new().w(container_width).h(60.0).justify(keyword);
    parent.style.flex_direction = if reverse {
        FlexDirection::RowReverse
    } else {
        FlexDirection::Row
    };
    let mut child = Element::new().w(child_width).h(20.0);
    child.style.flex_shrink = 0.0;
    let mut tree: TaffyTree<()> = TaffyTree::new();
    let child = tree.new_leaf(child.style.to_taffy_style()).unwrap();
    let parent = tree
        .new_with_children(parent.style.to_taffy_style(), &[child])
        .unwrap();
    tree.compute_layout(
        parent,
        Size {
            width: AvailableSpace::Definite(container_width),
            height: AvailableSpace::Definite(60.0),
        },
    )
    .unwrap();
    tree.layout(child).unwrap().location.x
}

#[test]
fn logical_start_and_overflow_safety_change_real_layout() {
    assert_eq!(child_position("start", true, 200.0, 40.0), 0.0);
    assert_eq!(child_position("flex-start", true, 200.0, 40.0), 160.0);
    assert_eq!(child_position("safe center", false, 40.0, 80.0), 0.0);
    assert_eq!(child_position("unsafe center", false, 40.0, 80.0), -20.0);
}

#[test]
fn text_align_last_values_and_inheritance() {
    for (keyword, expected) in [
        ("auto", TextAlignLast::Auto),
        ("start", TextAlignLast::Start),
        ("end", TextAlignLast::End),
        ("left", TextAlignLast::Left),
        ("right", TextAlignLast::Right),
        ("center", TextAlignLast::Center),
        ("justify", TextAlignLast::Justify),
        ("initial", TextAlignLast::Auto),
    ] {
        let element = Element::new().text_align_last(keyword);
        assert_eq!(element.text_style.align_last, Some(expected));
    }
    for keyword in ["inherit", "unset"] {
        let parent = Element::new().text_align_last(TextAlignLast::Right);
        let child = Element::new().text_align_last(keyword);
        let resolved = child.text_style.effective_ancestry(&parent.text_style);
        assert_eq!(resolved.align_last, Some(TextAlignLast::Right));
    }
    assert!(TextAlignLast::parse_declaration("safe center").is_none());
    assert!(TextAlignLast::parse_declaration("center trailing").is_none());
    assert!(TextAlignLast::parse_declaration("revert").is_none());
    assert!(TextAlignLast::parse_declaration("revert-layer").is_none());
    let element = Element::new()
        .text_align_last("center")
        .text_align_last("unknown");
    assert_eq!(element.text_style.align_last, Some(TextAlignLast::Center));
}

#[test]
fn final_alignment_recascades_after_a_parent_change() {
    let mut root = Element::new().text_align_last("center").children([
        Element::new().text("inherited").text_align_last("inherit"),
        Element::new().text("explicit").text_align_last("left"),
    ]);
    let mut tree = jadix::layout::layout_tree::LayoutTree::new();
    tree.build_and_layout(&mut root, 200.0, 100.0, [-1.0; 2]);
    assert_eq!(
        root.children[0].text_style.align_last,
        Some(TextAlignLast::Center)
    );
    assert_eq!(
        root.children[1].text_style.align_last,
        Some(TextAlignLast::Left)
    );
    root = root.text_align_last("right");
    tree.build_and_layout(&mut root, 200.0, 100.0, [-1.0; 2]);
    assert_eq!(
        root.children[0].text_style.align_last,
        Some(TextAlignLast::Right)
    );
    assert_eq!(
        root.children[1].text_style.align_last,
        Some(TextAlignLast::Left)
    );
}

#[cfg(feature = "macros")]
#[test]
fn rsx_accepts_css_alignment_attributes() {
    use jadix::prelude::*;
    let element = ui! {
        <Box align_items="safe center" align_content="space-evenly" align_self="self-start"
             justify_content="normal" justify_items="first baseline" justify_self="auto"
             text_align="justify" text_align_last="center">
            <Text text_align_last="right">"last line"</Text>
        </Box>
    };
    assert_eq!(element.style.align_items, Some(AlignItems::SAFE_CENTER));
    assert_eq!(
        element.style.align_content,
        Some(AlignContent::SPACE_EVENLY)
    );
    assert_eq!(element.style.align_self, Some(AlignItems::SELF_START));
    assert_eq!(element.style.justify_content, Some(AlignContent::NORMAL));
    assert_eq!(element.style.justify_items, Some(AlignItems::BASELINE));
    assert_eq!(element.style.justify_self, None);
    assert_eq!(element.text_style.align_last, Some(TextAlignLast::Center));
    assert_eq!(
        element.children[0].text_style.align_last,
        Some(TextAlignLast::Right)
    );
}

fn shape(
    text: &str,
    align: glyphon::cosmic_text::Align,
    last: Option<glyphon::cosmic_text::AlignLast>,
    width: f32,
) -> glyphon::Buffer {
    use glyphon::cosmic_text::{fontdb, Attrs, Buffer, FontSystem, Metrics, Shaping, Wrap};
    let mut fonts = FontSystem::new_with_locale_and_db("en-US".into(), fontdb::Database::new());
    fonts.db_mut().load_font_data(
        include_bytes!("../../../crates/jadix_cosmic_text/fonts/FiraMono-Medium.ttf").to_vec(),
    );
    fonts.db_mut().load_font_data(
        include_bytes!("../../../crates/jadix_cosmic_text/fonts/NotoSansHebrew.ttf").to_vec(),
    );
    let mut buffer = Buffer::new(&mut fonts, Metrics::new(16.0, 22.0));
    buffer.set_size(Some(width), None);
    buffer.set_wrap(Wrap::Word);
    buffer.set_text(text, &Attrs::new(), Shaping::Advanced, None);
    for paragraph in &mut buffer.lines {
        paragraph.set_align(Some(align));
        paragraph.set_align_last(last);
    }
    buffer.shape_until_scroll(&mut fonts, false);
    buffer
}

fn glyph_bounds(line: &glyphon::cosmic_text::LayoutLine) -> (f32, f32) {
    let left = line
        .glyphs
        .iter()
        .map(|glyph| glyph.x)
        .fold(f32::INFINITY, f32::min);
    let right = line
        .glyphs
        .iter()
        .map(|glyph| glyph.x + glyph.w)
        .fold(f32::NEG_INFINITY, f32::max);
    (left, right)
}

#[test]
fn final_alignment_changes_only_each_paragraphs_final_visual_line() {
    use glyphon::cosmic_text::{Align, AlignLast};
    let text = "one two three four five six seven eight\none two three four five six seven eight";
    let original = shape(text, Align::Left, None, 150.0);
    let centered = shape(text, Align::Left, Some(AlignLast::Center), 150.0);
    assert_eq!(original.lines.len(), centered.lines.len());
    for (original, centered) in original.lines.iter().zip(&centered.lines) {
        let original = original.layout_opt().unwrap();
        let centered = centered.layout_opt().unwrap();
        assert!(original.len() > 1);
        assert_eq!(original.len(), centered.len());
        let final_index = centered.len() - 1;
        for (index, (original, centered)) in original.iter().zip(centered).enumerate() {
            let (original_x, original_end) = glyph_bounds(original);
            let (centered_x, _) = glyph_bounds(centered);
            if index == final_index {
                let expected = (150.0 - (original_end - original_x)) * 0.5;
                assert!((centered_x - original_x - expected).abs() < 1.0);
            } else {
                assert!((centered_x - original_x).abs() < 0.01);
            }
        }
    }
}

#[test]
fn final_justify_expands_a_single_line_without_inserting_characters() {
    use glyphon::cosmic_text::{Align, AlignLast};
    let original = shape("one two", Align::Left, None, 180.0);
    let justified = shape("one two", Align::Left, Some(AlignLast::Justified), 180.0);
    let original = &original.lines[0];
    let justified = &justified.lines[0];
    assert_eq!(original.text(), justified.text());
    assert_eq!(justified.layout_opt().unwrap().len(), 1);
    let original = &original.layout_opt().unwrap()[0];
    let justified = &justified.layout_opt().unwrap()[0];
    assert_eq!(original.glyphs.len(), justified.glyphs.len());
    assert!(glyph_bounds(original).1 < 120.0);
    assert!((glyph_bounds(justified).1 - 180.0).abs() < 1.0);
}

#[test]
fn final_start_and_end_follow_paragraph_bidi_direction() {
    use glyphon::cosmic_text::{Align, AlignLast};
    let start = shape("שלום עולם", Align::Left, Some(AlignLast::Start), 180.0);
    let end = shape("שלום עולם", Align::Left, Some(AlignLast::End), 180.0);
    assert!(start.lines[0].shape_opt().unwrap().rtl);
    let start = glyph_bounds(&start.lines[0].layout_opt().unwrap()[0]);
    let end = glyph_bounds(&end.lines[0].layout_opt().unwrap()[0]);
    assert!(start.0 > end.0 + 20.0);
    assert!((start.1 - 180.0).abs() < 1.0);
    assert!(end.0.abs() < 1.0);
}

#[test]
fn every_final_line_position_has_the_expected_ltr_offset() {
    use glyphon::cosmic_text::{Align, AlignLast};
    let original = shape("one two", Align::Left, None, 180.0);
    let original = glyph_bounds(&original.lines[0].layout_opt().unwrap()[0]);
    let remaining = 180.0 - (original.1 - original.0);
    for (align, expected) in [
        (AlignLast::Start, 0.0),
        (AlignLast::Left, 0.0),
        (AlignLast::End, remaining),
        (AlignLast::Right, remaining),
        (AlignLast::Center, remaining * 0.5),
    ] {
        let buffer = shape("one two", Align::Left, Some(align), 180.0);
        let bounds = glyph_bounds(&buffer.lines[0].layout_opt().unwrap()[0]);
        assert!((bounds.0 - original.0 - expected).abs() < 1.0, "{align:?}");
    }
}

#[test]
fn changing_final_alignment_invalidates_only_layout_and_survives_split() {
    use glyphon::cosmic_text::{Align, AlignLast};
    let mut buffer = shape("one two", Align::Left, None, 180.0);
    let line = &mut buffer.lines[0];
    assert!(line.layout_opt().is_some());
    assert!(line.shape_opt().is_some());
    assert!(!line.set_align_last(None));
    assert!(line.layout_opt().is_some());
    assert!(line.set_align_last(Some(AlignLast::Right)));
    assert!(line.layout_opt().is_none());
    assert!(line.shape_opt().is_some());
    let split = line.split_off(4);
    assert_eq!(split.align_last(), Some(AlignLast::Right));
    assert_eq!(line.align_last(), Some(AlignLast::Right));
    line.reset_new(
        "new",
        glyphon::cosmic_text::LineEnding::None,
        glyphon::cosmic_text::AttrsList::new(&glyphon::Attrs::new()),
        glyphon::Shaping::Advanced,
    );
    assert_eq!(line.align_last(), None);
}
