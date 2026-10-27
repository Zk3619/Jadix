#![cfg(all(feature = "document-science", feature = "document-editing"))]

use jadix_components::{
    core::reactivity::{create_root, Signal},
    prelude::*,
    text::protein::ProteinBackbone,
};
use std::path::PathBuf;
#[cfg(feature = "file-persistence")]
use std::time::Duration;

fn atom(serial: usize, chain: &str, number: i32, position: [f32; 3]) -> String {
    format!("ATOM  {serial:5}  CA  ALA {chain}{number:4}    {:8.3}{:8.3}{:8.3}  1.00 20.00           C\n", position[0], position[1], position[2])
}

#[test]
fn protein_ribbon_has_surfaces_and_does_not_bridge_chains_or_gaps() {
    let source = [
        atom(1, "A", 1, [0.0, 0.0, 0.0]),
        atom(2, "A", 2, [3.7, 0.2, 0.0]),
        atom(3, "A", 3, [6.8, 2.0, 1.0]),
        "TER\n".into(),
        atom(4, "B", 1, [7.0, 2.2, 1.1]),
        atom(5, "B", 2, [9.9, 3.0, 2.0]),
        atom(6, "B", 8, [40.0, 0.0, 0.0]),
    ]
    .concat();
    let protein = ProteinBackbone::from_pdb(&source).unwrap();
    assert_eq!(protein.segments.len(), 2);
    let ribbon = protein.ribbon();
    assert!(ribbon.len() > 20);
    assert!(ribbon
        .iter()
        .flat_map(|quad| quad.corners)
        .flatten()
        .all(f32::is_finite));
    assert!(ribbon
        .iter()
        .all(|quad| quad.corners.iter().all(|point| point[0] < 12.0)));
    assert!(ProteinBackbone::from_pdb("ATOM\n").is_err());
    let (_owner, ()) = create_root(|| {
        let view = jadix_components::components::protein_view::protein_view(protein);
        assert!(view.children.is_empty());
        assert_eq!(
            view.get_attr("data-slot").map(String::as_str),
            Some("protein-view")
        );
    });
}

#[test]
fn markdown_inline_html_keeps_markdown_and_the_entire_table_or_list() {
    let source="Before **bold** <span style=\"color:red\">red <em>nested</em></span> after.\n\n| A | B |\n| --- | --- |\n| <sup>2</sup> | **bold** |\n\n- one <kbd>key</kbd>\n- two\n";
    let document = Document::markdown(source);
    let html: Vec<_> = document
        .blocks()
        .iter()
        .filter(|block| block.kind == DocumentBlockKind::Html)
        .collect();
    assert_eq!(html.len(), 3);
    assert!(html[0]
        .payload
        .as_deref()
        .unwrap()
        .contains("<strong>bold</strong>"));
    assert!(html[0]
        .payload
        .as_deref()
        .unwrap()
        .contains("<em>nested</em>"));
    assert!(html[1].payload.as_deref().unwrap().contains("<table>"));
    assert!(html[2].payload.as_deref().unwrap().contains("<ul>"));
    for block in html {
        assert_eq!(block.edit_range, Some(block.source.clone()));
    }
    let heading = Document::markdown("# Header <sup>2</sup>\n");
    assert_eq!(heading.blocks()[0].kind, DocumentBlockKind::Heading(1));
    assert_eq!(heading.blocks()[0].text(), "Header 2");
}

#[cfg(feature = "document-math")]
#[test]
fn mixed_html_and_math_uses_vector_formula_not_literal_tex() {
    let document = Document::markdown("<b>Formula:</b> $x^2 + \\frac{1}{2}$\n");
    let html = document.blocks()[0].payload.as_deref().unwrap();
    assert!(html.contains("<svg"));
    assert!(!html.contains("\\frac"));
}

#[test]
fn fences_inside_longer_fences_are_code_and_indented_code_is_dedented() {
    let document =
        Document::markdown("````md\n```rust\nlet value=1;\n```\n````\n\n    let indented = 1;\n");
    let code: Vec<_> = document
        .blocks()
        .iter()
        .filter(|block| matches!(block.kind, DocumentBlockKind::Code { .. }))
        .collect();
    assert!(code.iter().any(|block| block.text() == "```rust"));
    assert!(code.iter().any(|block| block.text() == "```"));
    assert!(code.iter().any(|block| block.text() == "let indented = 1;"));
}

#[test]
fn continued_list_items_do_not_repeat_bullets_or_checkboxes() {
    let document = Document::markdown("- [x] first\n  continued\n\n  second paragraph\n\n- next\n");
    let items: Vec<_> = document
        .blocks()
        .iter()
        .filter_map(|block| {
            if let DocumentBlockKind::ListItem {
                marker, checked, ..
            } = &block.kind
            {
                Some((marker.as_str(), *checked))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        items
            .iter()
            .filter(|(marker, _)| !marker.is_empty())
            .count(),
        2
    );
    assert_eq!(
        items
            .iter()
            .filter(|(_, checked)| checked.is_some())
            .count(),
        1
    );
}

#[test]
fn source_editors_expose_a_virtual_number_gutter() {
    fn sync(element: &mut Element, layout: &mut jadix_components::core::tree::LayoutTree) {
        if let Some(callback) = element.reactive_sync.clone() {
            callback.run(element, layout);
        }
        for child in &mut element.children {
            sync(child, layout);
        }
    }
    fn gutter(element: &Element) -> Option<&Element> {
        if element.get_attr("data-editor-line-numbers").is_some() {
            Some(element)
        } else {
            element.children.iter().find_map(gutter)
        }
    }
    let (_owner, ()) = create_root(|| {
        let value = Signal::new(
            (0..10000)
                .map(|line| format!("line {line}\n"))
                .collect::<String>(),
        );
        let editor: Element = text_editor("extension-numbered-source")
            .value_signal(value)
            .line_numbers(true)
            .wrap("none")
            .rows(8)
            .into();
        let mut layout = jadix_components::core::tree::LayoutTree::new();
        let mut editor = editor;
        layout.build_and_layout(&mut editor, 800.0, 300.0, [-1.0, -1.0]);
        sync(&mut editor, &mut layout);
        let labels = gutter(&editor).unwrap();
        assert!(labels.text.as_deref().unwrap_or("").starts_with("1\n"));
        assert!((1..100).contains(&labels.text.as_deref().unwrap_or("").lines().count()));
        let scroll =
            jadix_components::utils::text_field::textarea_scroll_rest("extension-numbered-source");
        use_signal(
            &jadix_components::core::scroll_viewport::scroll_offset_y_signal_key(&scroll),
            || 0.0_f32,
        )
        .set_silent(1800.0);
        sync(&mut editor, &mut layout);
        assert!(
            gutter(&editor)
                .unwrap()
                .text
                .as_deref()
                .unwrap()
                .lines()
                .next()
                .unwrap()
                .parse::<usize>()
                .unwrap()
                > 50
        );
    });
}

#[test]
fn image_paths_resolve_from_document_directory_and_decode_url_spaces() {
    let root = PathBuf::from("article");
    assert_eq!(
        jadix_components::utils::document_assets::image_path("images/my%20photo.png", Some(&root)),
        Some(root.join("images/my photo.png"))
    );
    assert!(
        jadix_components::utils::document_assets::image_path("javascript:alert(1)", None).is_none()
    );
    assert!(jadix_components::utils::document_assets::image_path("a%00.png", None).is_none());
}

#[test]
#[cfg(feature = "syntax-highlighting")]
fn syntax_colors_preserve_unicode_and_cross_line_comments() {
    let source: std::sync::Arc<str> =
        "/* comment\n still comment */\nfn main() { let 中文 = \"😀\"; }\n".into();
    let spans = jadix_components::text::syntax::highlight_code(&source, "rs", false);
    assert_eq!(
        spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>(),
        source.as_ref()
    );
    let second = jadix_components::text::syntax::highlight_block_line(&source, "rs", false, 1);
    assert_eq!(
        second
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>(),
        " still comment */"
    );
    #[cfg(feature = "syntax-highlighting")]
    assert!(spans.windows(2).any(|pair| pair[0].color != pair[1].color));
}

#[test]
fn editing_a_fence_refreshes_shared_lexical_context_without_replacing_other_rows() {
    let original = Document::markdown("```rust\nlet value = 1;\nlet next = 2;\n```\n\nafter\n");
    let insertion = original.source().text().find("let value").unwrap();
    let updated = original.replace(insertion..insertion, "/* ").unwrap();
    let payload = updated.blocks()[1].payload.as_ref().unwrap();
    assert_eq!(payload.as_ref(), "/* let value = 1;\nlet next = 2;\n");
    for (index, block) in updated.blocks().iter().enumerate().take(4) {
        assert!(std::sync::Arc::ptr_eq(
            block.payload.as_ref().unwrap(),
            payload
        ));
        if index != 1 {
            assert_eq!(block.id, original.blocks()[index].id);
        }
    }
    let fresh = Document::markdown(updated.source().text());
    assert_eq!(payload, fresh.blocks()[1].payload.as_ref().unwrap());
    assert_eq!(
        original.blocks()[1].payload.as_deref().unwrap(),
        "let value = 1;\nlet next = 2;\n"
    );
    #[cfg(feature = "syntax-highlighting")]
    {
        let before = jadix_components::text::syntax::highlight_block_line(
            original.blocks()[1].payload.as_ref().unwrap(),
            "rs",
            false,
            1,
        );
        let after = jadix_components::text::syntax::highlight_block_line(payload, "rs", false, 1);
        assert_ne!(before[0].color, after[0].color);
    }
}

#[cfg(feature = "webview")]
#[test]
fn html_images_are_embedded_only_inside_the_explicit_asset_root() {
    let directory = std::env::temp_dir().join(format!(
        "jadix-image-root-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = directory.join("article");
    std::fs::create_dir_all(&root).unwrap();
    ::image::save_buffer(
        root.join("photo space.png"),
        &[255, 0, 0, 255],
        1,
        1,
        ::image::ColorType::Rgba8,
    )
    .unwrap();
    ::image::save_buffer(
        directory.join("outside.png"),
        &[0, 0, 255, 255],
        1,
        1,
        ::image::ColorType::Rgba8,
    )
    .unwrap();
    let (_owner, ()) = create_root(|| {
        let view = jadix_components::components::html_document::html_document_view_with_assets("asset-sandbox-test", "<img src='photo%20space.png'><img src='../outside.png'><script><img src='hidden.png'></script>", Some(&root));
        fn html_surface(element: &Element) -> Option<&String> {
            element
                .get_attr("jadix_webview_html")
                .or_else(|| element.children.iter().find_map(html_surface))
        }
        let page = html_surface(&view).expect("HTML document must retain its WebView surface");
        assert!(page.contains("const localImages={\"photo%20space.png\":\"data:image/png;base64,"));
        assert!(!page.contains("outside.png\":\"data:"));
        assert!(page.contains("image.src=image.dataset.jadixOriginalSrc"));
    });
    std::fs::remove_dir_all(directory).unwrap();
}

#[cfg(feature = "file-persistence")]
#[test]
fn autosave_debounces_cancels_and_keeps_external_edits() {
    let directory = std::env::temp_dir().join(format!(
        "jadix-autosave-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("document.md");
    std::fs::write(&path, "original").unwrap();
    let (owner, (settings, text)) = create_root({
        let path = path.clone();
        move || {
            let settings =
                Signal::new(jadix_components::utils::editor_settings::AutosaveSettings {
                    enabled: true,
                    delay_ms: 250,
                });
            let text = Signal::new("original".to_string());
            jadix_components::utils::autosave::install_autosave(
                settings,
                move || {
                    Some(jadix_components::utils::autosave::SaveRequest {
                        key: 1,
                        path: path.clone(),
                        contents: text.get(),
                        expected: "original".into(),
                    })
                },
                |_| {},
            );
            (settings, text)
        }
    });
    text.set("first".into());
    std::thread::sleep(Duration::from_millis(120));
    text.set("latest".into());
    std::thread::sleep(Duration::from_millis(170));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
    for _ in 0..40 {
        if std::fs::read_to_string(&path).unwrap() == "latest" {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "latest");
    std::fs::write(&path, "external").unwrap();
    text.set("must not overwrite".into());
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "external");
    settings.set(jadix_components::utils::editor_settings::AutosaveSettings {
        enabled: false,
        delay_ms: 250,
    });
    std::fs::write(&path, "original").unwrap();
    text.set("disabled".into());
    std::thread::sleep(Duration::from_millis(350));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
    drop(owner);
    std::fs::remove_dir_all(directory).unwrap();
}
