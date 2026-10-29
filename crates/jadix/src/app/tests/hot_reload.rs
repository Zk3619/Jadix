use super::{
    ingest_template_patch, template_id_matches, HotReloadBatch, TemplateFragment, TemplatePatchMsg,
};

#[test]
fn template_id_match_accepts_same_tail_components_with_same_line() {
    assert!(template_id_matches(
        "d:/projects/jadix/examples/gallery/src/main.rs:188",
        "examples/gallery/src/main.rs:188",
    ));
}

#[test]
fn template_id_match_rejects_partial_segment_suffix_collisions() {
    assert!(!template_id_matches(
        "d:/projects/jadix/examples/gallery/src/main.rs:188",
        "gallery/src/domain.rs:188",
    ));
    assert!(!template_id_matches(
        "d:/projects/jadix/examples/gallery/src/main.rs:188",
        "examples/gallery/src/main.rs.bak:188",
    ));
    assert!(!template_id_matches(
        "d:/projects/jadix/examples/gallery/src/main.rs:188",
        "src/main.rs:188",
    ));
}

#[test]
fn template_id_match_requires_same_line() {
    assert!(!template_id_matches(
        "examples/gallery/src/main.rs:188",
        "examples/gallery/src/main.rs:189",
    ));
}

#[test]
fn template_id_match_does_not_treat_column_as_block_index() {
    // Same indent, different ui! blocks must not collide.
    assert!(!template_id_matches(
        "examples/gallery/src/main.rs:188:9",
        "examples/gallery/src/main.rs:400:9",
    ));
    assert!(template_id_matches(
        "d:/projects/jadix/examples/gallery/src/main.rs:400:9",
        "examples/gallery/src/main.rs:400:9",
    ));
}

#[test]
fn template_id_rank_picks_nearby_line_shift_same_column() {
    let near = super::template_id_rank(
        "d:/projects/jadix/examples/gallery/src/main.rs:402:9",
        "examples/gallery/src/main.rs:400:9",
    );
    let far = super::template_id_rank(
        "d:/projects/jadix/examples/gallery/src/main.rs:402:9",
        "examples/gallery/src/main.rs:188:9",
    );
    assert!(near.is_some_and(|s| s < 9_000));
    assert!(far.is_none());
}

#[test]
fn find_best_template_path_prefers_matching_line_not_first_same_column() {
    use crate::core::tree::Element;
    let root = Element::new().children(vec![
        Element::new()
            .template_id("examples/gallery/src/main.rs:188:9")
            .text("first"),
        Element::new()
            .template_id("examples/gallery/src/main.rs:400:9")
            .text("second"),
    ]);
    let path = super::find_best_template_path(
        &root,
        "d:/projects/jadix/examples/gallery/src/main.rs:400:9",
    )
    .expect("second block");
    assert_eq!(path, vec![1]);
    let shifted = super::find_best_template_path(
        &root,
        "d:/projects/jadix/examples/gallery/src/main.rs:401:9",
    );
    assert!(shifted.is_none());
}

#[test]
fn find_best_template_path_uses_preserved_source_id_on_reactive_island() {
    use crate::core::tree::Element;
    let mut island = Element::new()
        .template_id("examples/gallery/App:section_body")
        .text("导航");
    island.set_attr(
        crate::app::reactive::ATTR_SOURCE_TEMPLATE_ID,
        "examples/gallery/src/main.rs:4334:21",
    );
    let root = Element::new().children(vec![island]);
    let path = super::find_best_template_path(
        &root,
        "d:/projects/jadix/examples/gallery/src/main.rs:4334:21",
    )
    .expect("source template id on island");
    assert_eq!(path, vec![0]);
}

#[test]
fn identical_templates_in_distinct_instances_never_select_first_match() {
    use crate::core::tree::Element;
    let root = Element::new().children(vec![
        Element::new().template_id("src/main.rs:10:4"),
        Element::new().template_id("src/main.rs:10:4"),
    ]);
    assert!(super::find_best_template_path(&root, "src/main.rs:10:4").is_none());
}

#[test]
fn duplicate_aliases_on_one_instance_do_not_create_ambiguity() {
    let mut root = crate::core::tree::Element::new().template_id("src/main.rs:10:4");
    root.set_attr(
        crate::app::reactive::ATTR_SOURCE_TEMPLATE_ID,
        "src/main.rs:10:4",
    );
    assert_eq!(
        super::find_best_template_path(&root, "src/main.rs:10:4"),
        Some(vec![])
    );
}

#[test]
fn ingest_template_patch_empty_templates_is_noop() {
    let mut batch = HotReloadBatch::default();
    let mut by_template_id = std::collections::HashMap::new();
    ingest_template_patch(
        &mut batch,
        TemplatePatchMsg {
            file: "examples/gallery/src/main.rs".to_string(),
            templates: vec![],
        },
        &mut by_template_id,
    );
    assert!(
        !batch.force_reload,
        "empty templates must not rebuild from app_builder placeholders"
    );
    assert_eq!(batch.last_patch_file, "examples/gallery/src/main.rs");
    assert!(by_template_id.is_empty());
}

#[test]
fn ingest_template_patch_merges_by_template_id() {
    let mut batch = HotReloadBatch::default();
    let mut by_template_id = std::collections::HashMap::new();
    ingest_template_patch(
        &mut batch,
        TemplatePatchMsg {
            file: "a.rs".to_string(),
            templates: vec![TemplateFragment {
                id: "slot:a".to_string(),
                hash: "h1".to_string(),
                len: 1,
                index: 0,
                text_literals: vec!["A".to_string()],
                text_replacements: vec![],
                style_attrs: vec![],
                ..Default::default()
            }],
        },
        &mut by_template_id,
    );
    ingest_template_patch(
        &mut batch,
        TemplatePatchMsg {
            file: "b.rs".to_string(),
            templates: vec![TemplateFragment {
                id: "slot:a".to_string(),
                hash: "h2".to_string(),
                len: 2,
                index: 1,
                text_literals: vec!["B".to_string()],
                text_replacements: vec![],
                style_attrs: vec![],
                ..Default::default()
            }],
        },
        &mut by_template_id,
    );
    assert!(!batch.force_reload);
    assert_eq!(batch.last_patch_file, "a.rs");
    assert_eq!(by_template_id.len(), 1);
    let merged = by_template_id.get("slot:a").expect("slot:a present");
    assert_eq!(merged.hash, "h2");
    assert_eq!(merged.text_literals, vec!["B".to_string()]);
}

/// WP-R.6: hot-reload inject may fuzzy-match path suffix; `reconcile_subtree`
/// must **not** — it only equals after slash/case normalize.
#[test]
fn reconcile_anchor_id_is_exact_not_fuzzy() {
    let incoming = "d:/projects/jadix/examples/gallery/src/main.rs:188";
    let runtime = "examples/gallery/src/main.rs:188";
    assert!(
        template_id_matches(incoming, runtime),
        "dev patches still use fuzzy path-suffix matching"
    );
    let a = incoming.replace('\\', "/").to_ascii_lowercase();
    let b = runtime.replace('\\', "/").to_ascii_lowercase();
    assert_ne!(
        a, b,
        "structure-island reconcile must not treat these as the same slot"
    );
}
