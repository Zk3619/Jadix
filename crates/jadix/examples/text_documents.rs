//! cargo run -p jadix --example text_documents --features document-diff

use jadix::prelude::*;

fn document_ui() -> Element {
    let source = use_signal("documents-example-source", || {
        TextDocument::new(
            (1..=100_000)
                .map(|line| format!("{line:06}: let value = compute({line});\n"))
                .collect::<String>(),
        )
    });
    let comparison = use_signal("documents-example-diff", || {
        let original = source.peek();
        let modified = original
            .replace(0..0, "// Added first line\n")
            .expect("valid byte boundary");
        DiffDocument::new(original, modified)
    });
    let mode = use_signal("documents-example-mode", || DiffMode::Split);
    Element::new()
        .class("flex flex-col w-full h-full bg-background text-foreground")
        .child(title_bar("Document views"))
        .child(
            Element::new()
                .flex_col()
                .p(16.0)
                .gap(12.0)
                .child(Element::from(
                    button("diff-mode")
                        .variant("outline")
                        .text(if mode.get() == DiffMode::Split {
                            "Unified diff"
                        } else {
                            "Split diff"
                        })
                        .on_click(move || {
                            mode.update(|mode| {
                                *mode = if *mode == DiffMode::Split {
                                    DiffMode::Unified
                                } else {
                                    DiffMode::Split
                                };
                            })
                        }),
                ))
                .child(Element::from(
                    text_view("source-preview")
                        .document_signal(source)
                        .line_numbers(true)
                        .viewport_h(240.0),
                ))
                .child(Element::from(
                    diff_view("source-diff", comparison.peek())
                        .document_signal(comparison)
                        .mode(mode.get())
                        .viewport_h(240.0),
                )),
        )
}

fn main() {
    app()
        .title("Document views")
        .size(1100.0, 680.0)
        .min_size(600.0, 400.0)
        .frameless()
        .resize_border(5.0)
        .mount(document_ui);
}
