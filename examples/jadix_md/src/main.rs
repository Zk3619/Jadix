use jadix::app::reactive::{bind_text, reactive_view};
use jadix_components::prelude::*;
use std::{path::PathBuf, rc::Rc};

#[derive(Clone, PartialEq)]
struct FileState {
    path: Option<PathBuf>,
    saved: String,
    html: bool,
    generation: u64,
}

struct Workspace {
    autosave: Signal<jadix::utils::editor_settings::AutosaveSettings>,
    editor: DocumentEditorState,
    html: Signal<String>,
    html_mode: Signal<DocumentEditMode>,
    surface: Signal<(bool, u64)>,
    file: Signal<FileState>,
    status: Signal<String>,
}

impl Workspace {
    fn text(&self) -> String {
        if self.file.peek().html {
            self.html.peek()
        } else {
            self.editor.document().source().text().into()
        }
    }

    fn may_replace(&self) -> bool {
        self.text() == self.file.peek().saved
            || rfd::MessageDialog::new()
                .set_title("未保存的修改")
                .set_description("放弃当前修改并打开另一份文档？")
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                == rfd::MessageDialogResult::Yes
    }

    fn load(&self, path: Option<PathBuf>, text: String, html: bool) {
        self.editor.set_document(text.clone());
        self.editor.finish_editing();
        self.html.set(text.clone());
        self.file.set(FileState {
            path,
            saved: text,
            html,
            generation: self.file.peek().generation + 1,
        });
        self.surface.set((html, self.file.peek().generation));
        self.status.set("已打开文档".into());
    }

    fn open(&self) {
        if !self.may_replace() {
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("文档", &["md", "markdown", "txt", "html", "htm"])
            .pick_file()
        {
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    let html = matches!(
                        path.extension().and_then(|extension| extension.to_str()),
                        Some("html" | "htm")
                    );
                    self.load(Some(path), text, html);
                }
                Err(error) => self.status.set(format!("打开失败：{error}")),
            }
        }
    }

    fn save(&self, save_as: bool) {
        let mut file = self.file.peek();
        let target = if save_as || file.path.is_none() {
            rfd::FileDialog::new()
                .set_file_name(if file.html {
                    "document.html"
                } else {
                    "document.md"
                })
                .save_file()
        } else {
            file.path.clone()
        };
        let Some(target) = target else { return };
        if file.path.as_ref() == Some(&target)
            && std::fs::read_to_string(&target).ok().as_deref() != Some(&file.saved)
        {
            self.status
                .set("文件已在外部修改。请另存为以保留双方内容。".into());
            return;
        }
        let text = self.text();
        let expected = (file.path.as_ref() == Some(&target)).then_some(file.saved.as_str());
        match jadix::utils::document_file::save_text_document(&target, &text, expected) {
            Ok(()) => {
                file.path = Some(target);
                file.saved = text;
                self.file.set(file);
                self.status.set("已保存".into());
            }
            Err(error) => self.status.set(format!("保存失败：{error}")),
        }
    }
}

fn action(id: &str, label: &str, callback: impl Fn() + 'static) -> Element {
    button(id)
        .variant("ghost")
        .text(label)
        .on_click(callback)
        .into()
}

fn build() -> Element {
    let workspace = use_signal("md-workspace", || {
        let settings = jadix::utils::editor_settings::EditorSettings::load();
        let workspace = Rc::new(Workspace {
            autosave: Signal::new(
                settings
                    .as_ref()
                    .map(|settings| settings.autosave.clone())
                    .unwrap_or_default(),
            ),
            editor: DocumentEditorState::new(include_str!("welcome.md")),
            html: Signal::new(include_str!("welcome.html").into()),
            html_mode: Signal::new(DocumentEditMode::LivePreview),
            surface: Signal::new((false, 0)),
            file: Signal::new(FileState {
                path: None,
                saved: include_str!("welcome.md").into(),
                html: false,
                generation: 0,
            }),
            status: Signal::new("点击正文写作；表格和代码按块编辑".into()),
        });
        if let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) {
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    let html = path.extension().is_some_and(|extension| {
                        extension.eq_ignore_ascii_case("html")
                            || extension.eq_ignore_ascii_case("htm")
                    });
                    workspace.load(Some(path), text, html);
                }
                Err(error) => workspace.status.set(format!("打开失败：{error}")),
            }
        }
        if let Err(error) = settings {
            workspace.status.set(format!("配置错误：{error}"));
        }
        workspace
    })
    .peek();
    jadix::core::reactivity::component_root(|| {
        let source = workspace.clone();
        let feedback = workspace.clone();
        jadix::utils::autosave::install_autosave(
            workspace.autosave,
            move || {
                let file = source.file.get();
                let snapshot = source.editor.signal().get();
                let html = source.html.get();
                Some(jadix::utils::autosave::SaveRequest {
                    key: file.generation,
                    path: file.path?,
                    contents: if file.html {
                        html
                    } else {
                        snapshot.document.source().text().to_string()
                    },
                    expected: file.saved,
                })
            },
            move |result| {
                let mut file = feedback.file.peek();
                if file.generation != result.request.key
                    || file.path.as_ref() != Some(&result.request.path)
                {
                    return;
                }
                match result.result {
                    Ok(()) => {
                        file.saved = result.request.contents;
                        feedback.file.set(file);
                        feedback.status.set("已自动保存".into());
                    }
                    Err(error) => feedback.status.set(format!("自动保存失败：{error}")),
                }
            },
        );
    });
    let query = use_signal("md-search", String::new);
    let mut toolbar = Element::new()
        .display_flex()
        .flex_row()
        .flex_wrap_kw("wrap")
        .items_center()
        .gap(4.0)
        .px(16.0)
        .py(8.0);
    let autosave = workspace.autosave;
    toolbar = toolbar.child(reactive_view("md-autosave", move || {
        let settings = autosave.get();
        let label = if settings.enabled {
            format!("自动保存 {}ms", settings.delay_ms)
        } else {
            "自动保存：关".into()
        };
        action("md-toggle-autosave", &label, move || {
            let mut next = autosave.peek();
            next.enabled = !next.enabled;
            autosave.set(next);
        })
    }));
    for (id, label, command) in [
        ("new", "新建", 0),
        ("open", "打开", 1),
        ("save", "保存", 2),
        ("save-as", "另存为", 3),
        ("html", "HTML 示例", 4),
    ] {
        let workspace = workspace.clone();
        toolbar = toolbar.child(action(id, label, move || match command {
            0 if workspace.may_replace() => workspace.load(None, String::new(), false),
            1 => workspace.open(),
            2 => workspace.save(false),
            3 => workspace.save(true),
            4 if workspace.may_replace() => {
                workspace.load(None, include_str!("welcome.html").into(), true)
            }
            _ => (),
        }));
    }
    for (id, label, mode) in [
        ("live", "实时", DocumentEditMode::LivePreview),
        ("source", "源码", DocumentEditMode::Source),
        ("preview", "阅读", DocumentEditMode::Preview),
    ] {
        let workspace = workspace.clone();
        toolbar = toolbar.child(action(id, label, move || {
            workspace.editor.set_mode(mode);
            workspace.html_mode.set(mode);
        }));
    }
    let format_workspace = workspace.clone();
    toolbar = toolbar.child(reactive_view("md-format-controls", move || {
        if format_workspace.surface.get().0 {
            return Element::new()
                .text("在正文工具栏设置格式")
                .text_size(12.0)
                .text_color("muted-foreground");
        }
        let mut controls = Element::new().display_flex().flex_row().gap(4.0);
        for (id, label, mark) in [
            ("bold", "粗体", DocumentMark::Bold),
            ("italic", "斜体", DocumentMark::Italic),
        ] {
            let workspace = format_workspace.clone();
            controls = controls.child(action(id, label, move || {
                let _ = workspace.editor.toggle_mark(mark);
            }));
        }
        let image_workspace = format_workspace.clone();
        controls = controls.child(action("md-insert-image", "插图", move || {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("图片", &["png", "jpg", "jpeg", "gif", "webp", "svg"])
                .pick_file()
            {
                let file = image_workspace.file.peek();
                let destination = file
                    .path
                    .as_ref()
                    .and_then(|file| file.parent())
                    .and_then(|root| path.strip_prefix(root).ok())
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/")
                    .replace('>', "%3E")
                    .replace('<', "%3C");
                let _ = image_workspace
                    .editor
                    .replace_selection(&format!("![图片](<{destination}>)"));
            }
        }));
        for (id, label, undo) in [("undo", "撤销", true), ("redo", "重做", false)] {
            let workspace = format_workspace.clone();
            controls = controls.child(action(id, label, move || {
                if undo {
                    workspace.editor.undo();
                } else {
                    workspace.editor.redo();
                }
            }));
        }
        controls
    }));
    let search_workspace = workspace.clone();
    let search = Element::new()
        .display_flex()
        .flex_row()
        .gap(8.0)
        .px(16.0)
        .py(6.0)
        .child(
            Element::from(
                input("md-query")
                    .placeholder("搜索文档…")
                    .value_signal(query),
            )
            .w(240.0),
        )
        .child(action("md-find", "查找下一个", move || {
            if search_workspace.file.peek().html {
                search_workspace
                    .status
                    .set("HTML 阅读与编辑请使用 Ctrl+F 查找".into());
                return;
            }
            let text = search_workspace.editor.document();
            let query = query.peek();
            if query.is_empty() {
                return;
            }
            let start = search_workspace.editor.selection().range().end;
            let found = text.source().text()[start..]
                .find(&query)
                .map(|offset| offset + start)
                .or_else(|| text.source().text()[..start].find(&query));
            if let Some(offset) = found {
                let _ = search_workspace.editor.set_selection(DocumentSelection {
                    anchor: offset,
                    caret: offset + query.len(),
                });
            } else {
                search_workspace.status.set("未找到".into());
            }
        }));
    let outline_workspace = workspace.clone();
    let outline = reactive_view("md-outline", move || {
        let document = outline_workspace.editor.signal().get().document;
        let mut list = Element::new()
            .display_flex()
            .flex_col()
            .gap(6.0)
            .p(20.0)
            .child(Element::new().text("文档大纲").font_weight(700));
        for block in document
            .blocks()
            .iter()
            .filter(|block| matches!(block.kind, DocumentBlockKind::Heading(_)))
            .take(200)
        {
            let workspace = outline_workspace.clone();
            let byte = block.source.start;
            list = list.child(action(
                &format!("heading-{}", block.id),
                &block.text(),
                move || {
                    let _ = workspace
                        .editor
                        .set_selection(DocumentSelection::caret(byte));
                },
            ));
        }
        list.w(220.0)
            .h("100%")
            .overflow("scroll")
            .bg("muted/30")
            .flex_shrink(0.0)
    })
    .w(220.0)
    .h("100%")
    .overflow("scroll")
    .bg("muted/30")
    .flex_shrink(0.0);
    let content_workspace = workspace.clone();
    let content = reactive_view("md-surface", move || {
        let _ = content_workspace.surface.get();
        let file = content_workspace.file.peek();
        let surface = if file.html {
            match content_workspace.html_mode.get() {
                DocumentEditMode::Source => Element::from(
                    text_editor(format!("md-html-source-{}", file.generation))
                        .value_signal(content_workspace.html)
                        .rows(30)
                        .line_numbers(true)
                        .language("html")
                        .wrap("none"),
                )
                .font_family("monospace"),
                DocumentEditMode::Preview => {
                    jadix_components::components::html_document::html_document_view_with_assets(
                        format!("md-html-view-{}", file.generation),
                        content_workspace.html.peek(),
                        file.path.as_ref().and_then(|path| path.parent()),
                    )
                }
                DocumentEditMode::LivePreview => {
                    jadix_components::components::html_document::html_document_editor_with_assets(
                        format!("md-html-{}", file.generation),
                        content_workspace.html,
                        file.path.as_ref().and_then(|path| path.parent()),
                    )
                }
            }
        } else {
            Element::new()
                .display_flex()
                .flex_col()
                .h("100%")
                .min_h(0.0)
                .w("100%")
                .p(28.0)
                .child(
                    Element::from(
                        document_editor(
                            format!("md-editor-{}", file.generation),
                            content_workspace.editor.clone(),
                        )
                        .asset_root(
                            file.path
                                .as_ref()
                                .and_then(|path| path.parent())
                                .map(std::path::Path::to_path_buf)
                                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default()),
                        )
                        .viewport_h(600.0),
                    )
                    .h("100%")
                    .min_h(0.0)
                    .flex_grow(1.0),
                )
        };
        surface.flex_grow(1.0).flex_basis(0.0).min_w(0.0).min_h(0.0)
    })
    .flex_grow(1.0)
    .flex_basis(0.0)
    .min_w(0.0)
    .min_h(0.0);
    let footer_workspace = workspace.clone();
    let footer = bind_text("md-status", move || {
        let file = footer_workspace.file.get();
        let snapshot = footer_workspace.editor.signal().get();
        let html = footer_workspace.html.get();
        let text = if file.html {
            html.as_str()
        } else {
            snapshot.document.source().text()
        };
        format!(
            "{}{}  ·  {} 字符  ·  {} 行  ·  {}",
            file.path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or("未命名".into()),
            if text != file.saved { " *" } else { "" },
            text.chars().count(),
            text.lines().count(),
            footer_workspace.status.get()
        )
    })
    .px(20.0)
    .py(8.0)
    .flex_shrink(0.0)
    .text_size(12.0)
    .text_color("muted-foreground");
    let mut root = Element::new()
        .class("flex flex-col w-full h-full bg-background text-foreground")
        .display_flex()
        .flex_col()
        .w("100%")
        .h("100%")
        .child(title_bar("Jadix Markdown"))
        .child(toolbar)
        .child(search)
        .child(
            Element::new()
                .display_flex()
                .flex_row()
                .w("100%")
                .flex_basis(0.0)
                .flex_grow(1.0)
                .min_h(0.0)
                .child(outline)
                .child(content),
        )
        .child(footer);
    root.set_attr("data-slot", "markdown-workspace");
    root
}

fn main() {
    set_theme_mode(ThemeMode::Light);
    app()
        .title("Jadix Markdown")
        .size(1240.0, 860.0)
        .min_size(1050.0, 700.0)
        .frameless()
        .resize_border(5.0)
        .mount(build);
}

#[cfg(test)]
mod tests {
    #[test]
    fn app_mount_preserves_the_workspace_layout_shell() {
        let (_owner, ()) = jadix::core::reactivity::create_root(|| {
            let packed = jadix::utils::take_packed(super::build());
            assert_eq!(packed.len(), 1);
            assert_eq!(
                packed[0].get_attr("data-slot").map(String::as_str),
                Some("markdown-workspace")
            );
            assert_eq!(packed[0].children.len(), 5);
        });
    }
}
