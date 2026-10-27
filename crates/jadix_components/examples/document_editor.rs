//! cargo run -p jadix --example document_editor --features document-editing,document-math,text-undo

use jadix_components::prelude::*;

const SOURCE: &str = r#"# Native document editor

Edit this **rich text**, *emphasis*, `inline code`, or $E = mc^2$.

## Lists and tables

- [x] Semantic document model
- [x] Source reveal while editing
- [ ] Continue writing

| Surface | Model |
| :--- | :--- |
| Markdown | source-preserving blocks |
| Rich text | styled runs |

> A quotation with a [link](https://example.com).

```rust
fn main() {
    println!("hello");
}
```

$$
\frac{a}{b} + \sqrt{x^2+y^2}
$$

```chem
2 H2 + O2 -> 2 H2O
```

```xyz
3
water
O 0 0 0
H 0.958 0 0
H -0.239 0.927 0
```
"#;

fn editor_ui() -> Element {
    let session = use_signal("document-editor-demo-session", || {
        DocumentEditorState::new(SOURCE)
    })
    .peek();
    let toolbar = Element::new()
        .flex_row()
        .gap(8.0)
        .child(Element::from(
            button("doc-live")
                .variant("outline")
                .text("Live")
                .on_click({
                    let session = session.clone();
                    move || session.set_mode(DocumentEditMode::LivePreview)
                }),
        ))
        .child(Element::from(
            button("doc-source")
                .variant("outline")
                .text("Source")
                .on_click({
                    let session = session.clone();
                    move || session.set_mode(DocumentEditMode::Source)
                }),
        ))
        .child(Element::from(
            button("doc-preview")
                .variant("outline")
                .text("Preview")
                .on_click({
                    let session = session.clone();
                    move || session.set_mode(DocumentEditMode::Preview)
                }),
        ))
        .child(Element::from(
            button("doc-bold").variant("outline").text("B").on_click({
                let session = session.clone();
                move || {
                    let _ = session.toggle_mark(DocumentMark::Bold);
                }
            }),
        ))
        .child(Element::from(
            button("doc-undo")
                .variant("outline")
                .text("Undo")
                .on_click({
                    let session = session.clone();
                    move || {
                        session.undo();
                    }
                }),
        ))
        .child(Element::from(
            button("doc-redo")
                .variant("outline")
                .text("Redo")
                .on_click({
                    let session = session.clone();
                    move || {
                        session.redo();
                    }
                }),
        ));
    Element::new()
        .class("flex flex-col w-full h-full bg-background text-foreground")
        .child(title_bar("Document editor"))
        .child(
            Element::new()
                .flex_col()
                .p(16.0)
                .gap(12.0)
                .child(toolbar)
                .child(Element::from(
                    document_editor("document-demo", session).viewport_h(640.0),
                )),
        )
}

fn main() {
    app()
        .title("Document editor")
        .size(1100.0, 800.0)
        .min_size(600.0, 400.0)
        .frameless()
        .resize_border(5.0)
        .mount(editor_ui);
}
