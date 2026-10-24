use jadix::app::reactive::{bind_text, reactive_view};
use jadix::components::code_editor::code_editor;
use jadix::prelude::*;
use std::{path::PathBuf, rc::Rc};
mod ide_lsp;
mod lsp;

const ORIGINAL: &str = "fn greeting(name: &str) -> String {\n    format!(\"Hello, {}!\", name)\n}\n\nfn main() {\n    println!(\"{}\", greeting(\"world\"));\n}\n";
const MODIFIED: &str = "fn greeting(name: &str) -> String {\n    format!(\"Hello, {name}!\")\n}\n\nfn main() {\n    println!(\"{}\", greeting(\"Jadix\"));\n}\n";

#[derive(Clone)]
struct CodeFile {
    id: usize,
    path: Signal<Option<PathBuf>>,
    value: Signal<String>,
    saved: Signal<String>,
    baseline: Signal<String>,
}

impl PartialEq for CodeFile {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

struct Workspace {
    language: std::cell::RefCell<ide_lsp::Clients>,
    lsp_panel: Signal<String>,
    completions: Signal<Vec<ide_lsp::Completion>>,
    autosave: Signal<jadix::utils::editor_settings::AutosaveSettings>,
    settings: jadix::utils::editor_settings::EditorSettings,
    files: Signal<Vec<CodeFile>>,
    active: Signal<usize>,
    mode: Signal<u8>,
    diff: Signal<DiffDocument>,
    status: Signal<String>,
}

impl Workspace {
    fn current(&self) -> CodeFile {
        self.files.peek()[self.active.peek()].clone()
    }

    fn add(&self, path: Option<PathBuf>, text: String) {
        let mut files = self.files.peek();
        if let Some(index) = path.as_ref().and_then(|path| {
            files
                .iter()
                .position(|file| file.path.peek().as_ref() == Some(path))
        }) {
            self.active.set(index);
            self.mode.set(0);
            return;
        }
        let id = files.len();
        files.push(CodeFile {
            id,
            path: Signal::new(path),
            value: Signal::new(text.clone()),
            saved: Signal::new(text.clone()),
            baseline: Signal::new(text),
        });
        self.files.set(files);
        self.active.set(id);
        self.mode.set(0);
    }

    fn open(&self) {
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            match std::fs::read_to_string(&path) {
                Ok(text) => self.add(Some(path), text),
                Err(error) => self.status.set(format!("打开失败：{error}")),
            }
        }
    }

    fn save(&self, save_as: bool) {
        let file = self.current();
        let previous = file.path.peek();
        let path = if save_as || previous.is_none() {
            rfd::FileDialog::new()
                .set_file_name("untitled.rs")
                .save_file()
        } else {
            previous.clone()
        };
        let Some(path) = path else { return };
        if previous.as_ref() == Some(&path)
            && std::fs::read_to_string(&path).ok().as_deref() != Some(&file.saved.peek())
        {
            self.status
                .set("磁盘文件已变化；请另存为，避免覆盖外部修改。".into());
            return;
        }
        let text = file.value.peek();
        let saved = file.saved.peek();
        let expected = (previous.as_ref() == Some(&path)).then_some(saved.as_str());
        match jadix::utils::document_file::save_text_document(&path, &text, expected) {
            Ok(()) => {
                file.path.set(Some(path));
                file.saved.set(text);
                self.status.set("已保存；对照基线保持不变".into());
            }
            Err(error) => self.status.set(format!("保存失败：{error}")),
        }
    }

    fn compare(&self, mode: u8) {
        let file = self.current();
        self.diff
            .set(DiffDocument::new(file.baseline.peek(), file.value.peek()));
        self.mode.set(mode);
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
    let workspace = use_signal("ide-workspace", || {
        let (settings, configuration_error) =
            match jadix::utils::editor_settings::EditorSettings::load() {
                Ok(settings) => (settings, None),
                Err(error) => (Default::default(), Some(format!("配置加载失败：{error}"))),
            };
        let workspace = Rc::new(Workspace {
            language: Default::default(),
            lsp_panel: Signal::new("LSP：配置 language_servers 后打开文件即可连接".into()),
            completions: Signal::new(Vec::new()),
            autosave: Signal::new(settings.autosave.clone()),
            settings,
            files: Signal::new(vec![CodeFile {
                id: 0,
                path: Signal::new(None),
                value: Signal::new(MODIFIED.into()),
                saved: Signal::new(MODIFIED.into()),
                baseline: Signal::new(ORIGINAL.into()),
            }]),
            active: Signal::new(0),
            mode: Signal::new(0),
            diff: Signal::new(DiffDocument::new(ORIGINAL, MODIFIED)),
            status: Signal::new(
                configuration_error
                    .unwrap_or_else(|| "原生代码编辑 · Ctrl+Z / Ctrl+Y 撤销重做 · UTF-8".into()),
            ),
        });
        for path in std::env::args_os().skip(1).map(PathBuf::from) {
            match std::fs::read_to_string(&path) {
                Ok(text) => workspace.add(Some(path), text),
                Err(error) => workspace.status.set(format!("打开失败：{error}")),
            }
        }
        workspace
    })
    .peek();
    jadix::core::reactivity::component_root(|| {
        let failed_saves = Signal::new(Vec::<jadix::utils::autosave::SaveRequest>::new());
        let source = workspace.clone();
        let feedback = workspace.clone();
        jadix::utils::autosave::install_autosave(
            workspace.autosave,
            move || {
                let failed = failed_saves.get();
                source
                    .files
                    .get()
                    .into_iter()
                    .filter_map(|file| {
                        let path = file.path.get();
                        let contents = file.value.get();
                        let expected = file.saved.get();
                        path.filter(|_| contents != expected).map(|path| {
                            jadix::utils::autosave::SaveRequest {
                                key: file.id as u64,
                                path,
                                contents,
                                expected,
                            }
                        })
                    })
                    .find(|request| !failed.contains(request))
            },
            move |result| {
                if let Some(file) = feedback.files.peek().into_iter().find(|file| {
                    file.id as u64 == result.request.key
                        && file.path.peek().as_ref() == Some(&result.request.path)
                }) {
                    match result.result {
                        Ok(()) => {
                            file.saved.set(result.request.contents);
                            feedback.status.set("已自动保存；对照基线保持不变".into());
                        }
                        Err(error) => {
                            failed_saves.update(|failed| {
                                failed.retain(|request| request.key != result.request.key);
                                failed.push(result.request);
                            });
                            feedback.status.set(format!("自动保存失败：{error}"));
                        }
                    }
                }
            },
        );
        ide_lsp::install(&workspace);
    });
    let mut toolbar = Element::new()
        .display_flex()
        .flex_row()
        .flex_wrap_kw("wrap")
        .items_center()
        .gap(6.0)
        .px(16.0)
        .py(8.0);
    let autosave = workspace.autosave;
    toolbar = toolbar.child(reactive_view("ide-autosave", move || {
        let settings = autosave.get();
        let label = if settings.enabled {
            format!("自动保存 {}ms", settings.delay_ms)
        } else {
            "自动保存：关".into()
        };
        action("ide-toggle-autosave", &label, move || {
            let mut next = autosave.peek();
            next.enabled = !next.enabled;
            autosave.set(next);
        })
    }));
    for (id, label, command) in [
        ("ide-new", "新建", 0),
        ("ide-open", "打开文件", 1),
        ("ide-save", "保存", 2),
        ("ide-save-as", "另存为", 3),
        ("ide-edit", "编辑", 4),
        ("ide-split", "并排 Diff", 5),
        ("ide-unified", "统一 Diff", 6),
        ("ide-baseline", "载入对照", 7),
    ] {
        let workspace = workspace.clone();
        toolbar = toolbar.child(action(id, label, move || match command {
            0 => workspace.add(None, String::new()),
            1 => workspace.open(),
            2 => workspace.save(false),
            3 => workspace.save(true),
            4 => workspace.mode.set(0),
            5 => workspace.compare(1),
            6 => workspace.compare(2),
            _ => {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    match std::fs::read_to_string(path) {
                        Ok(text) => {
                            workspace.current().baseline.set(text);
                            workspace.compare(1);
                        }
                        Err(error) => workspace.status.set(format!("对照文件读取失败：{error}")),
                    }
                }
            }
        }));
    }
    for (label, method) in [
        ("补全", "textDocument/completion"),
        ("悬浮信息", "textDocument/hover"),
        ("定义", "textDocument/definition"),
        ("格式化", "textDocument/formatting"),
    ] {
        let target = workspace.clone();
        toolbar = toolbar.child(action(&format!("ide-lsp-{method}"), label, move || {
            ide_lsp::request(&target, method)
        }));
    }
    let tabs_workspace = workspace.clone();
    let tabs = reactive_view("ide-tabs", move || {
        let files = tabs_workspace.files.get();
        let active = tabs_workspace.active.get();
        let mut tabs = Element::new()
            .display_flex()
            .flex_row()
            .gap(4.0)
            .px(16.0)
            .py(6.0)
            .bg("muted/30");
        for (index, file) in files.iter().enumerate() {
            let workspace = tabs_workspace.clone();
            let file = file.clone();
            let label = file
                .path
                .get()
                .as_ref()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| format!("untitled-{}.rs", file.id + 1));
            tabs = tabs.child(Element::from(
                button(&format!("ide-tab-{}", file.id))
                    .variant(if index == active {
                        "secondary"
                    } else {
                        "ghost"
                    })
                    .text(label)
                    .on_click(move || {
                        workspace.active.set(index);
                        workspace.mode.set(0);
                    }),
            ));
        }
        tabs
    });
    let body_workspace = workspace.clone();
    let body = reactive_view("ide-surface", move || {
        let index = body_workspace.active.get();
        let file = body_workspace.files.get()[index].clone();
        let mode = body_workspace.mode.get();
        let surface = if mode == 0 {
            Element::new()
                .display_flex()
                .flex_col()
                .p(20.0)
                .gap(10.0)
                .child(
                    Element::new()
                        .text("SOURCE  ·  UTF-8  ·  无软换行")
                        .text_size(12.0)
                        .text_color("muted-foreground"),
                )
                .child(
                    Element::from(
                        code_editor(format!("jadix-ide://buffer/{}", file.id))
                            .language(
                                file.path
                                    .peek()
                                    .as_ref()
                                    .and_then(|path| path.extension())
                                    .and_then(|extension| extension.to_str())
                                    .unwrap_or("rs"),
                            )
                            .value_signal(file.value)
                            .rows(28)
                            .wrap("none"),
                    )
                    .font_family("monospace")
                    .w("100%"),
                )
        } else {
            let diff = body_workspace.diff.peek();
            Element::new()
                .display_flex()
                .flex_col()
                .p(20.0)
                .gap(10.0)
                .child(
                    Element::new()
                        .text("对照版本 → 当前缓冲区  ·  返回编辑后点击 Diff 重新计算")
                        .text_size(12.0)
                        .text_color("muted-foreground"),
                )
                .child(Element::from(
                    diff_view(format!("ide-diff-{mode}-{}", file.id), diff)
                        .mode(if mode == 1 {
                            DiffMode::Split
                        } else {
                            DiffMode::Unified
                        })
                        .viewport_h(610.0),
                ))
        };
        surface
            .flex_grow(1.0)
            .flex_basis(0.0)
            .h("100%")
            .min_h(0.0)
            .min_w(0.0)
    })
    .flex_grow(1.0)
    .flex_basis(0.0)
    .min_h(0.0)
    .min_w(0.0);
    let panel_workspace = workspace.clone();
    let language_panel = reactive_view("ide-language-panel", move || {
        let completions = panel_workspace.completions.get();
        let mut panel = Element::new()
            .display_flex()
            .flex_col()
            .h(if completions.is_empty() { 48.0 } else { 96.0 })
            .flex_shrink(0.0)
            .overflow("auto")
            .px(20.0)
            .py(8.0)
            .bg("muted")
            .child(
                Element::new()
                    .text(panel_workspace.lsp_panel.get())
                    .text_size(12.0)
                    .white_space("pre-wrap"),
            );
        for (index, completion) in completions.into_iter().enumerate() {
            let target = panel_workspace.clone();
            let label = completion.label.clone();
            panel = panel.child(action(
                &format!("ide-completion-{index}"),
                &label,
                move || ide_lsp::complete(&target, &completion),
            ));
        }
        panel
    })
    .flex_shrink(0.0);
    let footer_workspace = workspace.clone();
    let footer = bind_text("ide-status", move || {
        let files = footer_workspace.files.get();
        let file = &files[footer_workspace.active.get()];
        let value = file.value.get();
        format!(
            "{}{}  ·  {} 行  ·  {} 字节  ·  {}",
            file.path
                .get()
                .map(|path| path.display().to_string())
                .unwrap_or("未命名".into()),
            if value != file.saved.get() { " *" } else { "" },
            value.lines().count(),
            value.len(),
            footer_workspace.status.get()
        )
    })
    .px(20.0)
    .py(8.0)
    .text_size(12.0)
    .text_color("muted-foreground");
    let mut root = Element::new()
        .class("flex flex-col w-full h-full bg-background text-foreground")
        .display_flex()
        .flex_col()
        .w("100%")
        .h("100%")
        .child(title_bar("Jadix IDE"))
        .child(toolbar)
        .child(tabs)
        .child(body)
        .child(language_panel)
        .child(footer);
    root.set_attr("data-slot", "ide-workspace");
    root
}

fn main() {
    app()
        .title("Jadix IDE")
        .size(1280.0, 850.0)
        .min_size(1000.0, 700.0)
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
                Some("ide-workspace")
            );
            assert_eq!(packed[0].children.len(), 6);
        });
    }
}
