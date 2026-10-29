use crate::imports::*;
use crate::shared::{MyCard, MyCardProps};
use jadix::text::{DocumentEditMode, DocumentEditorState};
use jadix_document::{DiffDocument, DiffMode, TextDocument};

const MARKDOWN: &str = "# 原生文档工作台\n\n直接点击段落开始编辑，或切换到源码模式。\n\n## 丰富内容\n\n- **粗体**、*斜体*与 `inline code`\n- [Jadix](https://github.com/Zk3619/Jadix) 原生链接\n- 支持撤销、重做和选区编辑\n\n> 文档由语义块组成，在原生视口中渲染。\n\n| 能力 | 用途 |\n| --- | --- |\n| Markdown | 编辑与阅读 |\n| Diff | 版本比较 |\n\n```rust\nlet count = Signal::new(0);\n```\n";

const RUST_SOURCE: &str = "use jadix_components::prelude::*;\n\nfn counter() -> Element {\n    let count = use_signal(\"counter\", || 0);\n    let action: Element = button(\"increment\")\n        .text(\"增加\")\n        .on_click(move || count.update(|value| *value += 1))\n        .into();\n    ui! {\n        <VStack gap=\"1rem\">\n            <Label>{format!(\"计数：{}\", count.get())}</Label>\n            {action}\n        </VStack>\n    }\n}\n";

#[component]
pub(crate) fn DocumentEditingShowcase() -> Element {
    let editor = use_signal("gallery-document-state", || {
        DocumentEditorState::new(MARKDOWN)
    })
    .peek();
    let modes: Vec<Element> = [
        ("live", "实时预览", DocumentEditMode::LivePreview),
        ("source", "Markdown 源码", DocumentEditMode::Source),
        ("preview", "阅读", DocumentEditMode::Preview),
    ]
    .into_iter()
    .map(|(id, title, mode)| {
        let editor = editor.clone();
        button(format!("gallery-document-{id}"))
            .variant("outline")
            .text(title)
            .on_click(move || editor.set_mode(mode))
            .into()
    })
    .collect();
    let undo = editor.clone();
    let redo = editor.clone();
    let reset = editor.clone();
    let surface: Element = document_editor("gallery-document-editor", editor)
        .viewport_h(400.0)
        .into();
    ui! {
        <MyCard title="Markdown 编辑与阅读">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label color="muted-foreground">"切换编辑模式，输入文本并使用撤销 / 重做；内容保留在当前 Gallery 会话中。"</Label>
                <HStack wrap="wrap" gap="0.5rem">
                    {modes}
                    <Button id="gallery-document-undo" variant="outline" on_click={move || { undo.undo(); }}>"撤销"</Button>
                    <Button id="gallery-document-redo" variant="outline" on_click={move || { redo.redo(); }}>"重做"</Button>
                    <Button id="gallery-document-reset" variant="secondary" on_click={move || reset.set_document(MARKDOWN)}>"重置文档"</Button>
                </HStack>
                {surface}
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn DocumentScienceShowcase() -> Element {
    let formula: Element = math_view(r"\frac{-b \pm \sqrt{b^2-4ac}}{2a}")
        .display(true)
        .font_size(24.0)
        .into();
    let reaction: Element = chemistry_view("2H2 + O2 -> 2H2O").into();
    let document: Element = document_view("gallery-science-document")
        .document("## 科学文档\n\n公式与图表使用原生文档块渲染。\n\n$$\nE = mc^2\n$$\n\n```mermaid\nflowchart TD\n    Source --> Parse\n    Parse --> Layout\n    Layout --> Render\n```\n")
        .viewport_h(280.0).into();
    ui! {
        <MyCard title="公式、化学与图表">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label color="muted-foreground">"TeX 公式、化学方程式和 Mermaid 文档块；不支持的语法保留可见诊断。"</Label>
                {formula}
                {reaction}
                {document}
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn DocumentDiffShowcase() -> Element {
    let mode = use_signal("gallery-diff-mode", || DiffMode::Split);
    let original = TextDocument::new("fn greeting() {\n    println!(\"Hello\");\n}\n");
    let modified =
        TextDocument::new("fn greeting(name: &str) {\n    println!(\"Hello, {name}!\");\n}\n");
    let comparison = DiffDocument::new(original, modified);
    ui! {
        <MyCard title="代码差异比较">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <HStack wrap="wrap" gap="0.5rem">
                    <Button id="gallery-diff-split" variant="outline" on_click={move || mode.set(DiffMode::Split)}>"并排比较"</Button>
                    <Button id="gallery-diff-unified" variant="outline" on_click={move || mode.set(DiffMode::Unified)}>"统一差异"</Button>
                </HStack>
                <Match scope="gallery-document-diff-mode">
                    <When when={mode.get() == DiffMode::Split}>
                        {Element::from(diff_view("gallery-diff-surface", comparison.clone()).mode(DiffMode::Split).viewport_h(220.0))}
                    </When>
                    <When when={true}>
                        {Element::from(diff_view("gallery-diff-surface", comparison.clone()).mode(DiffMode::Unified).viewport_h(220.0))}
                    </When>
                </Match>
            </VStack>
        </MyCard>
    }
}

#[component]
pub(crate) fn CodeEditingShowcase() -> Element {
    let source = use_signal("gallery-code-source", || RUST_SOURCE.to_string());
    let editor: Element = code_editor("gallery-code-editor")
        .language("rust")
        .line_numbers(true)
        .rows(18)
        .value_signal(source)
        .into();
    ui! {
        <MyCard title="Rust 代码编辑器">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label color="muted-foreground">"体验语法高亮、行号、文本选区与撤销。示例本身混用 Builder 和 ui!；编辑器不执行输入的代码。"</Label>
                <HStack wrap="wrap" gap="0.75rem" items="center">
                    <Button id="gallery-code-reset" variant="outline" on_click={move || source.set(RUST_SOURCE.to_string())}>"恢复示例"</Button>
                    <Label id="gallery-code-length">{format!("{} 字节", source.get().len())}</Label>
                </HStack>
                {editor}
            </VStack>
        </MyCard>
    }
}
