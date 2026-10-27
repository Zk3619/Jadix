use jadix_components::prelude::*;

#[cfg(feature = "code")]
#[test]
fn code_editor_is_independent_of_highlighting_and_language_servers() {
    let (_owner, ()) = jadix_components::core::reactivity::create_root(|| {
        let editor: Element = jadix_components::components::code_editor("feature-code")
            .initial("let value = 1;")
            .into();
        assert!(editor.input_paint_sync.is_some());
    });
}

#[cfg(feature = "document")]
#[test]
fn document_view_is_available_without_editing_or_science() {
    let (_owner, ()) = jadix_components::core::reactivity::create_root(|| {
        let document = Document::markdown("# Title\n\ncontent\n");
        assert!(matches!(
            document.blocks()[0].kind,
            DocumentBlockKind::Heading(1)
        ));
        let view: Element = document_view("feature-document").document(document).into();
        assert_eq!(view.attrs["data-slot"], "document-view");
    });
}
