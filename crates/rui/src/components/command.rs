//! # Command
//!
//! 对齐 shadcn/ui Base [`command.tsx`](https://github.com/shadcn-ui/ui) 与 solid-ui `command.tsx`：
//! 复合件 **Command / CommandInput / CommandList / CommandEmpty / CommandGroup /
//! CommandSeparator / CommandItem / CommandShortcut**。
//!
//! 三种表面：
//! - **行内** [`Command`]：嵌在 Card 等布局里。
//! - **Dialog** [`CommandDialog`]：组合 Dialog（居中模态，`show_close=false`），不改 `dialog.rs`。
//! - **纯浮层** [`command_overlay_scope`] / `<CommandOverlay>`：VS Code 式视口**顶部居中**，
//!   透明点击关闭层 + 模态栈面板（无 Dialog 壳；点击空白 / Esc 关闭）。
//!
//! 行为对照 cmdk：**类型即搜**，↑↓ 高亮，Enter 执行；浮层 / Dialog 下 Esc 关闭。
//!
//! ## 用法（复合 / 类前端）
//!
//! ```ignore
//! // 行内（Card 内嵌）
//! ui! {
//!     <Command id="cmd">
//!         <CommandInput placeholder="Type a command or search..." />
//!         <CommandList>
//!             <CommandEmpty>"No results found."</CommandEmpty>
//!             <CommandGroup heading="Suggestions">
//!                 <CommandItem value="cal">"Calendar"</CommandItem>
//!             </CommandGroup>
//!             <CommandSeparator />
//!             <CommandGroup heading="Settings">
//!                 <CommandItem value="profile">
//!                     "Profile"
//!                     <CommandShortcut>"⌘P"</CommandShortcut>
//!                 </CommandItem>
//!             </CommandGroup>
//!         </CommandList>
//!     </Command>
//! }
//!
//! // 命令面板：VS Code 式顶部居中浮层
//! ui! {
//!     <CommandOverlay id="pal" open={open}>
//!         <CommandInput placeholder="Type a command or search..." />
//!         <CommandList>
//!             <CommandEmpty>"No results found."</CommandEmpty>
//!             <CommandItem value="ping" on_click={move || { /* … */ }}>"Ping"</CommandItem>
//!         </CommandList>
//!     </CommandOverlay>
//! }
//!
//! // 命令面板：复用 Dialog（居中模态）
//! ui! {
//!     <CommandDialog id="pal-dlg" open={open_dlg}>
//!         <CommandInput placeholder="Type a command or search..." />
//!         <CommandList>
//!             <CommandEmpty>"No results found."</CommandEmpty>
//!             <CommandItem value="ping" on_click={move || { /* … */ }}>"Ping"</CommandItem>
//!         </CommandList>
//!     </CommandDialog>
//! }
//! ```
//!
//! ## 用法（Builder）
//!
//! ```ignore
//! command("pal")
//!     .open_signal(open) // 默认顶部居中浮层
//!     .action(CommandAction::new("ping", "Ping", || log::info!("ping")))
//! // 居中 Dialog：`.dialog()`
//!     .action(CommandAction::new("ping", "Ping", || log::info!("ping")))
//! ```

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode, OverlayZ};
use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized, reactive_view};
use crate::components::button::button;
use crate::components::dialog::{dialog_content, dialog_scope, use_dialog, DialogContentOpts};
use crate::components::input::{input, input_value_signal_key, lucide_named};
use crate::components::label::label;
use crate::components::scroll_area::scroll_area;
use crate::components::separator::separator;
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, with_child_owner, Context, Signal,
};
use crate::core::tree::{Element, Handler};
use crate::layout::layout_tree::ATTR_PORTAL_KEY;
use crate::primitives::r#box::{hstack, vstack};
use crate::primitives::text::text;
use crate::style::{parse_color, parse_text_color};
use crate::tokens;
use crate::utils::popup::{
    panel_block_clicks, portal_panel, swallow_clicks, underlay_close_on_click,
};
use crate::utils::roving::{roving_edge, roving_move};
use crate::utils::text_node::build_icon_text_node;
use rui_macros::component;
use winit::keyboard::{Key, NamedKey};

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

/// 键盘 / 空状态用的可见项快照。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandVisibleItem {
    pub value: String,
    pub label: String,
    pub disabled: bool,
}

/// Command 展示表面。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CommandSurface {
    /// 嵌在现有布局中（无 portal）。
    Inline,
    /// 复用 Dialog：视口居中模态。
    Dialog,
    /// VS Code 式：视口顶部居中纯浮层。
    #[default]
    Overlay,
}

impl CommandSurface {
    fn dismissible(self) -> bool {
        !matches!(self, Self::Inline)
    }
}

/// Command 根上下文（复合子件共享）。
#[derive(Clone)]
pub struct CommandCtx {
    pub id: String,
    pub open: Signal<bool>,
    pub active: Signal<usize>,
    pub visible: Signal<Vec<CommandVisibleItem>>,
    pub empty_msg: Signal<String>,
    pub placeholder: String,
    pub filter_id: String,
    pub surface: CommandSurface,
}

fn command_context() -> Context<CommandCtx> {
    static CTX: OnceLock<Context<CommandCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<CommandCtx>)
}

/// 读取最近的 [`Command`] 上下文。
pub fn use_command() -> Option<CommandCtx> {
    use_context(command_context())
}

fn require_command(part: &str) -> CommandCtx {
    use_command().unwrap_or_else(|| {
        panic!("{part} must be used within <Command> / <CommandDialog> / <CommandOverlay> / command_scope");
    })
}

fn take_packed(mut el: Element) -> Vec<Element> {
    if el.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1") {
        std::mem::take(&mut el.children)
    } else if !el.children.is_empty()
        && el.text.is_none()
        && el.on_click.is_none()
        && el.focus_id.is_none()
    {
        std::mem::take(&mut el.children)
    } else {
        vec![el]
    }
}

fn extract_text_leaf(el: &Element) -> String {
    if let Some(t) = &el.text {
        return t.clone();
    }
    for c in &el.children {
        let t = extract_text_leaf(c);
        if !t.is_empty() {
            return t;
        }
    }
    String::new()
}

fn label_matches_query(label: &str, query: &str) -> bool {
    let q = query.to_ascii_lowercase();
    q.is_empty() || label.to_ascii_lowercase().contains(&q)
}

fn has_shortcut_slot(el: &Element) -> bool {
    if el.attrs.get("data-slot").map(|s| s.as_str()) == Some("command-shortcut") {
        return true;
    }
    el.children.iter().any(has_shortcut_slot)
}

fn clear_button_focus() {
    use_signal("__rui_button_focus_id", String::new).set(String::new());
}

fn query_of(ctx: &CommandCtx) -> String {
    use_signal(input_value_signal_key(&ctx.filter_id).as_str(), String::new).peek()
}

thread_local! {
    static COMMAND_ACTIVATES: RefCell<HashMap<String, Vec<Rc<dyn Fn()>>>> = RefCell::new(HashMap::new());
}

fn reset_activates(id: &str) {
    COMMAND_ACTIVATES.with(|m| {
        m.borrow_mut().insert(id.to_string(), Vec::new());
    });
}

fn push_activate(id: &str, f: Rc<dyn Fn()>) {
    COMMAND_ACTIVATES.with(|m| {
        m.borrow_mut().entry(id.to_string()).or_default().push(f);
    });
}

fn take_activate(id: &str, ix: usize) -> Option<Rc<dyn Fn()>> {
    COMMAND_ACTIVATES.with(|m| {
        m.borrow()
            .get(id)
            .and_then(|v| v.get(ix).cloned())
    })
}

/// solid-ui / shadcn：`flex size-full flex-col overflow-hidden rounded-md bg-popover`。
/// DialogContent 固定 `p-1.5rem`；面板内用负边距抵消，对齐 `p-0`（不改 dialog.rs）。
/// Overlay / Dialog 按内容撑开；勿 `overflow:hidden` + `min_h:0`（flex 会把固有高度压成 0）。
/// DialogContent 会再写 `min_h:0` / `flex_grow:1`，所以这里用 `flex_shrink:0` 保住输入行高度。
fn command_root_shell(surface: CommandSurface, kids: Vec<Element>) -> Element {
    let hug = matches!(
        surface,
        CommandSurface::Overlay | CommandSurface::Dialog
    );
    let mut inner = vstack().w("100%").gap("0").children(kids);
    if !hug {
        inner = inner.min_h("0").flex_grow(1.0);
    }
    let inner: Element = inner.into();
    let mut root = vstack().w("100%").child(inner);
    if hug {
        root = root.flex_shrink(0.0);
    } else {
        root = root.min_h("0").flex_grow(1.0).overflow("hidden");
    }
    let mut root: Element = root.into();
    root = root.bg(tokens::popover());
    match surface {
        CommandSurface::Dialog => {
            root = root.mx("-1.5rem").my("-1.5rem");
        }
        CommandSurface::Inline => {
            root = root
                .rounded("0.375rem")
                .border_style("solid")
                .border_width(1.0)
                .border_color(tokens::border());
        }
        CommandSurface::Overlay => {}
    }
    root
}

// ---------------------------------------------------------------------------
// Scopes
// ---------------------------------------------------------------------------

/// 提供上下文并按当前 query 重建壳（不注册 `reactive_view`）。
/// Overlay / Dialog 的外层 effect 必须调用本函数，以便订阅 query 且避免嵌套 100% 占位塌高。
fn command_paint(
    id: String,
    open: Signal<bool>,
    placeholder: String,
    surface: CommandSurface,
    body: Rc<dyn Fn() -> Element>,
) -> Element {
    let filter_id = format!("{id}__filter");
    let active = use_signal(format!("{id}__cmd_active").as_str(), || 0usize);
    let visible = use_signal(
        format!("{id}__cmd_visible").as_str(),
        Vec::<CommandVisibleItem>::new,
    );
    let empty_msg = use_signal(format!("{id}__cmd_empty").as_str(), || {
        "No results found.".to_string()
    });
    let query_sig = use_signal(input_value_signal_key(&filter_id).as_str(), String::new);
    let painted = with_child_owner(move || {
        provide_context(
            command_context(),
            CommandCtx {
                id: id.clone(),
                open,
                active,
                visible,
                empty_msg,
                placeholder,
                filter_id,
                surface,
            },
        );
        let _ = query_sig.get();
        visible.set(Vec::new());
        reset_activates(&id);
        if active.peek() != 0 && visible.peek().is_empty() {
            active.set(0);
        }
        command_root_shell(surface, take_packed(body()))
    });
    // DialogContent 会给返回根写 `min_h:0` + `flex_grow:1`。固有高度放在内层，避免被压成 0。
    if surface == CommandSurface::Dialog {
        vstack()
            .w("100%")
            .flex_shrink(0.0)
            .child(painted)
            .into()
    } else {
        painted
    }
}

fn command_placeholder(placeholder: impl Into<String>) -> String {
    let placeholder = placeholder.into();
    if placeholder.is_empty() {
        "Type a command or search...".into()
    } else {
        placeholder
    }
}

/// 提供 Command 上下文并构建子树（`ui!` `<Command>` / Builder 共用）。
///
/// 已在 Command 上下文内时（例如 [`command_dialog_scope`] 内再包 `<Command>`）只套视觉壳。
pub fn command_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    placeholder: impl Into<String>,
    surface: CommandSurface,
    body: impl Fn() -> Element + 'static,
) -> Element {
    if use_command().is_some() {
        return command_root_shell(CommandSurface::Inline, take_packed(body()));
    }

    let id = crate::utils::resolve_instance_id(id, "command");
    let placeholder = command_placeholder(placeholder);
    let open = open.unwrap_or_else(|| use_signal(format!("{id}__cmd_open").as_str(), || false));
    let body: Rc<dyn Fn() -> Element> = Rc::new(body);
    let view_id = format!("{id}__cmd_view");
    reactive_anchor(view_id.clone(), {
        let id = id.clone();
        let body = body.clone();
        let placeholder = placeholder.clone();
        move || command_paint(id.clone(), open, placeholder.clone(), surface, body.clone())
    });
    // 100%×100% 会在 auto 高度父级（Dialog / Overlay）里塌成 0。
    reactive_placeholder_sized(view_id, "100%", "auto")
}

/// ShadCN / solid-ui `CommandDialog`：`Dialog` + `DialogContent(show_close=false)` + Command 壳。
pub fn command_dialog_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    placeholder: impl Into<String>,
    show_close: bool,
    body: impl Fn() -> Element + 'static,
) -> Element {
    let id = crate::utils::resolve_instance_id(id, "command");
    let placeholder = command_placeholder(placeholder);
    let body: Rc<dyn Fn() -> Element> = Rc::new(body);
    let host = dialog_scope(id.clone(), open, move || {
        let open = use_dialog()
            .expect("command_dialog_scope: dialog_scope")
            .open;
        let id = id.clone();
        let placeholder = placeholder.clone();
        let body = body.clone();
        dialog_content(
            DialogContentOpts {
                show_close,
                ..Default::default()
            },
            move || {
                command_paint(
                    id.clone(),
                    open,
                    placeholder.clone(),
                    CommandSurface::Dialog,
                    body.clone(),
                )
            },
        )
    });
    // DialogContent 默认 100%×100% 占位；若留在 HStack/卡片流里会撑破布局。
    // 0×0 锚点仍能 hoist 出 underlay/stage（与 Select / Combobox 相同）。
    Element::new()
        .w(0.0)
        .h(0.0)
        .pointer_events_box_none()
        .child(host)
}

/// VS Code 式命令面板：视口顶部居中，透明点击关闭层，不走 Dialog。
///
/// 打开链路对齐已能显示的 Sheet：`reactive_view` + `100%×100%` 根，
/// 再 hoist underlay 与带明确左右边的顶栏。不要用 Select 式 `0×0` 锚点当宿主——
/// 舞台只有 `inset(0)` 时提升后宽高仍是 0，面板等于没画出来。
pub fn command_overlay_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    placeholder: impl Into<String>,
    body: impl Fn() -> Element + 'static,
) -> Element {
    let id = crate::utils::resolve_instance_id(id, "command");
    let placeholder = command_placeholder(placeholder);
    let open = open.unwrap_or_else(|| use_signal(format!("{id}__cmd_open").as_str(), || false));
    let body: Rc<dyn Fn() -> Element> = Rc::new(body);
    let view_id = format!("{id}__cmd_overlay");
    reactive_view(view_id, move || {
        let _ = open.get();
        let Some(oz) = portal_sync(&id, open, OverlayKind::Modal, OverlayMode::Stacked) else {
            return Element::new();
        };
        if !open.peek() {
            return Element::new();
        }
        let inner = command_paint(
            id.clone(),
            open,
            placeholder.clone(),
            CommandSurface::Overlay,
            body.clone(),
        );
        command_overlay_frame(open, oz, inner)
    })
}

/// 顶部居中条 + 透明 underlay。提升节点本身带 `left/right/width`，与 Sheet 面板相同。
fn command_overlay_frame(open: Signal<bool>, oz: OverlayZ, inner: Element) -> Element {
    let mut panel: Element = vstack()
        .w("40rem")
        .max_w("90dvw")
        .flex_shrink(0.0)
        .child(inner)
        .into();
    panel = panel
        .bg(tokens::popover())
        .rounded("0.5rem")
        .border_style("solid")
        .border_width(1.0)
        .border_color(tokens::border())
        .shadow([0.0, 0.0, 0.0, 0.4], 28.0)
        .overflow("hidden");
    panel = swallow_clicks(panel);

    let mut bar: Element = hstack()
        .absolute(true)
        .top("0.75rem")
        .left(0.0)
        .right(0.0)
        .w("100%")
        .justify_center()
        .child(panel)
        .into();
    bar.set_attr(ATTR_PORTAL_KEY, format!("panel:{}", oz.panel));

    let mut root: Element = hstack()
        .w("100%")
        .h("100%")
        .child(underlay_close_on_click(open, oz.underlay))
        .into();
    root = root.child(portal_panel(panel_block_clicks(bar, oz.panel), oz.panel));
    root
}

// ---------------------------------------------------------------------------
// Data + Builder
// ---------------------------------------------------------------------------

/// 一条可过滤命令。
#[derive(Clone)]
pub struct CommandAction {
    pub id: String,
    pub label: String,
    pub shortcut: Option<String>,
    pub disabled: bool,
    pub run: Rc<dyn Fn()>,
}

impl CommandAction {
    pub fn new(id: impl Into<String>, label: impl Into<String>, run: impl Fn() + 'static) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            shortcut: None,
            disabled: false,
            run: Rc::new(run),
        }
    }

    pub fn shortcut(mut self, s: impl Into<String>) -> Self {
        self.shortcut = Some(s.into());
        self
    }

    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
}

/// Builder 分组。
#[derive(Clone)]
pub struct CommandGroupData {
    pub heading: String,
    pub items: Vec<CommandAction>,
}

impl CommandGroupData {
    pub fn new(heading: impl Into<String>) -> Self {
        Self {
            heading: heading.into(),
            items: Vec::new(),
        }
    }

    pub fn item(mut self, item: CommandAction) -> Self {
        self.items.push(item);
        self
    }
}

enum CommandEntry {
    Item(CommandAction),
    Group(CommandGroupData),
    Separator,
}

pub struct CommandBuilder {
    id: String,
    open: Option<Signal<bool>>,
    entries: Vec<CommandEntry>,
    placeholder: String,
    surface: CommandSurface,
}

/// 命令面板入口。传 [`CommandBuilder::open_signal`] 时默认顶部居中浮层；[`.dialog()`](CommandBuilder::dialog) 走 Dialog。
pub fn command(id: impl Into<String>) -> CommandBuilder {
    CommandBuilder {
        id: id.into(),
        open: None,
        entries: Vec::new(),
        placeholder: "Type a command or search...".into(),
        surface: CommandSurface::Overlay,
    }
}

impl CommandBuilder {
    pub fn open_signal(mut self, s: Signal<bool>) -> Self {
        self.open = Some(s);
        self
    }
    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = s.into();
        self
    }
    pub fn actions(mut self, actions: Vec<CommandAction>) -> Self {
        self.entries
            .extend(actions.into_iter().map(CommandEntry::Item));
        self
    }
    pub fn action(mut self, action: CommandAction) -> Self {
        self.entries.push(CommandEntry::Item(action));
        self
    }
    pub fn group(mut self, group: CommandGroupData) -> Self {
        self.entries.push(CommandEntry::Group(group));
        self
    }
    pub fn separator(mut self) -> Self {
        self.entries.push(CommandEntry::Separator);
        self
    }
    /// 视口顶部居中纯浮层（默认）。
    pub fn overlay(mut self) -> Self {
        self.surface = CommandSurface::Overlay;
        self
    }
    /// 复用 Dialog 的居中模态。
    pub fn dialog(mut self) -> Self {
        self.surface = CommandSurface::Dialog;
        self
    }
}

fn builder_palette_body(placeholder: String, entries: Vec<CommandEntry>) -> Element {
    let mut list_kids: Vec<Element> = Vec::new();
    list_kids.push(CommandEmpty(CommandEmptyProps {
        children: Some(Element::new().text("No results found.")),
        base: None,
    }));
    for e in entries {
        match e {
            CommandEntry::Item(a) => list_kids.push(builder_item(a)),
            CommandEntry::Group(g) => {
                let mut kids: Vec<Element> = Vec::new();
                for a in g.items {
                    kids.push(builder_item(a));
                }
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                list_kids.push(CommandGroup(CommandGroupProps {
                    heading: Some(g.heading),
                    children: Some(frag.children(kids)),
                    base: None,
                }));
            }
            CommandEntry::Separator => list_kids.push(CommandSeparator(CommandSeparatorProps {
                children: None,
                base: None,
            })),
        }
    }
    let mut list_frag = Element::new();
    list_frag.set_attr("rui_fragment", "1");
    let mut root = Element::new();
    root.set_attr("rui_fragment", "1");
    root.children(vec![
        CommandInput(CommandInputProps {
            placeholder: Some(placeholder),
            children: None,
            base: None,
        }),
        CommandList(CommandListProps {
            children: Some(list_frag.children(list_kids)),
            base: None,
        }),
    ])
}

fn builder_item(a: CommandAction) -> Element {
    let mut kids = vec![Element::new().text(a.label.clone())];
    if let Some(s) = a.shortcut.clone() {
        kids.push(CommandShortcut(CommandShortcutProps {
            children: Some(Element::new().text(s)),
            base: None,
        }));
    }
    let mut frag = Element::new();
    frag.set_attr("rui_fragment", "1");
    let run = a.run.clone();
    CommandItem(CommandItemProps {
        value: Some(a.id),
        disabled: Some(a.disabled),
        checked: None,
        on_click: Some(Handler::new(move |_| (run)())),
        children: Some(frag.children(kids)),
        base: None,
    })
}

impl From<CommandBuilder> for Element {
    fn from(builder: CommandBuilder) -> Self {
        let CommandBuilder {
            id,
            open,
            entries,
            placeholder,
            surface,
        } = builder;
        let entries = Rc::new(entries);
        let placeholder_c = placeholder.clone();
        let body = {
            let entries = entries.clone();
            move || builder_palette_body(placeholder_c.clone(), clone_entries(&entries))
        };
        if open.is_some() {
            match surface {
                CommandSurface::Dialog => command_dialog_scope(id, open, placeholder, false, body),
                CommandSurface::Overlay | CommandSurface::Inline => {
                    command_overlay_scope(id, open, placeholder, body)
                }
            }
        } else {
            command_scope(id, open, placeholder, CommandSurface::Inline, body)
        }
    }
}

fn clone_entries(entries: &Rc<Vec<CommandEntry>>) -> Vec<CommandEntry> {
    entries
        .iter()
        .map(|e| match e {
            CommandEntry::Item(a) => CommandEntry::Item(a.clone()),
            CommandEntry::Group(g) => CommandEntry::Group(g.clone()),
            CommandEntry::Separator => CommandEntry::Separator,
        })
        .collect()
}

impl crate::core::tree::IntoElementList for CommandBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

// ---------------------------------------------------------------------------
// Compound parts
// ---------------------------------------------------------------------------

/// RSX `<Command>`：有复合子树时走 [`command_scope`]；否则走 Builder。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Command(
    id: String,
    open: Option<Signal<bool>>,
    placeholder: Option<String>,
    actions: Option<Vec<CommandAction>>,
    children: Option<Element>,
) -> Element {
    if let Some(c) = children {
        // `ui!` 复合路径由宏走 [`command_scope`]（每次按 query 重建）。
        let cell = RefCell::new(Some(c));
        return command_scope(
            id,
            open,
            placeholder.unwrap_or_default(),
            CommandSurface::Inline,
            move || cell.borrow_mut().take().unwrap_or_else(Element::new),
        );
    }
    let mut b = command(&id);
    if let Some(s) = open {
        b = b.open_signal(s);
    }
    if let Some(p) = placeholder {
        b = b.placeholder(p);
    }
    if let Some(a) = actions {
        b = b.actions(a);
    }
    b.into()
}

/// ShadCN / solid-ui `CommandInput`：底边框 + 搜索图标 + 透明输入。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn CommandInput(placeholder: Option<String>, children: Option<Element>) -> Element {
    let _ = children;
    let ctx = require_command("CommandInput");
    let ph = placeholder
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| ctx.placeholder.clone());
    let filter_id = ctx.filter_id.clone();
    let cmd_id = ctx.id.clone();
    let open = ctx.open;
    let active = ctx.active;
    let visible = ctx.visible;
    let dismissible = ctx.surface.dismissible();

    let icon = build_icon_text_node(lucide_named("search"))
        .text_size(16.0)
        .text_color(parse_text_color("muted-foreground"))
        .w(16.0)
        .h(16.0)
        .flex_shrink(0.0)
        .opacity(0.5)
        .pointer_events_none();

    let mut field: Element = input(&filter_id)
        .placeholder(ph.as_str())
        .w("100%")
        .h("2.5rem")
        .into();
    field = field
        .flex_grow(1.0)
        .min_w(0.0)
        .border_width(0.0)
        .rounded(0.0)
        .px(0.0);

    field = field.on_key_down(move |ev| {
        let list = visible.peek();
        let n = list.len();
        match &ev.logical_key {
            Key::Named(NamedKey::Escape) => {
                if dismissible {
                    open.set(false);
                    clear_button_focus();
                    return true;
                }
            }
            Key::Named(NamedKey::ArrowDown) => {
                if let Some(next) = roving_move(n, active.peek(), 1) {
                    active.set(next);
                }
                return true;
            }
            Key::Named(NamedKey::ArrowUp) => {
                if let Some(next) = roving_move(n, active.peek(), -1) {
                    active.set(next);
                }
                return true;
            }
            Key::Named(NamedKey::Home) => {
                if let Some(i) = roving_edge(n, false) {
                    active.set(i);
                }
                return true;
            }
            Key::Named(NamedKey::End) => {
                if let Some(i) = roving_edge(n, true) {
                    active.set(i);
                }
                return true;
            }
            Key::Named(NamedKey::Enter) => {
                let ix = active.peek();
                if let Some(item) = list.get(ix) {
                    if !item.disabled {
                        if let Some(act) = take_activate(&cmd_id, ix) {
                            act();
                        }
                        if dismissible {
                            open.set(false);
                            clear_button_focus();
                        }
                    }
                    return true;
                }
            }
            _ => {}
        }
        false
    });

    let mut wrap: Element = hstack()
        .w("100%")
        .h("2.25rem")
        .px("0.75rem")
        .gap("0.5rem")
        .items_center()
        .flex_shrink(0.0)
        .child(icon)
        .child(field)
        .into();
    wrap = wrap.border_bottom(tokens::border(), 1.0);
    wrap
}

/// ShadCN / solid-ui `CommandList`：`max-h-[300px] overflow-y-auto overflow-x-hidden`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn CommandList(children: Option<Element>) -> Element {
    let ctx = require_command("CommandList");
    let kids = children.map(take_packed).unwrap_or_default();
    let mut col = vstack().w("100%").gap("0").children(kids);
    if ctx.visible.peek().is_empty() {
        col = col.child(command_empty_ui(&ctx.empty_msg.peek()));
    }
    let n = ctx.visible.peek().len();
    if n == 0 {
        ctx.active.set(0);
    } else if ctx.active.peek() >= n {
        ctx.active.set(n - 1);
    }
    let scroll_id = format!("{}:list", ctx.id);
    // Overlay / Dialog 父级高度为 auto：`overflow: auto` 在 Taffy 里会变成 0 高滚动盒。
    // 这两处只用 `max-h` 按内容撑开；行内才用 ScrollArea 填满父级。
    if ctx.surface == CommandSurface::Inline {
        return scroll_area(scroll_id.as_str())
            .overflow_y("auto")
            .overflow_x("hidden")
            .w("100%")
            .max_h("18.75rem")
            .min_h("0")
            .flex_grow(1.0)
            .child(col)
            .into();
    }
    vstack()
        .w("100%")
        .max_h("18.75rem")
        .flex_shrink(0.0)
        .child(col)
        .into()
}

fn command_empty_ui(msg: &str) -> Element {
    let t: Element = text(msg)
        .size(14.0)
        .color(parse_text_color("muted-foreground"))
        .into();
    vstack()
        .w("100%")
        .py("1.5rem")
        .items("center")
        .child(t)
        .into()
}

/// `CommandEmpty`：登记文案；真正绘制由 [`CommandList`] 在无可见项时插入（不依赖 RSX 顺序）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn CommandEmpty(children: Option<Element>) -> Element {
    let ctx = require_command("CommandEmpty");
    let msg = match &children {
        Some(c) => c
            .text
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| extract_text_leaf(c)),
        None => "No results found.".into(),
    };
    if ctx.empty_msg.peek() != msg {
        ctx.empty_msg.set(msg);
    }
    Element::new().w(0.0).h(0.0)
}

/// `CommandGroup`：`p-1` + heading（`text-xs text-muted-foreground`）；过滤后无项则隐藏。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn CommandGroup(heading: Option<String>, children: Option<Element>) -> Element {
    let ctx = require_command("CommandGroup");
    let kids = children.map(take_packed).unwrap_or_default();
    let kids: Vec<Element> = kids
        .into_iter()
        .filter(|k| k.attrs.get("rui_command_hidden").map(|s| s.as_str()) != Some("1"))
        .collect();
    if kids.is_empty() {
        return Element::new().w(0.0).h(0.0);
    }
    let mut col = vstack().w("100%").gap("0").p("0.25rem");
    if let Some(h) = heading.filter(|s| !s.is_empty()) {
        let mut head: Element = label(format!("{}:grp:{}", ctx.id, h).as_str())
            .text(h)
            .size(12.0)
            .line_height(12.0)
            .color(parse_text_color("muted-foreground"))
            .into();
        head = head.w("100%").px("0.5rem").py("0.375rem");
        col = col.child(head);
    }
    col.children(kids).into()
}

/// ShadCN / solid-ui `CommandSeparator`：`h-px bg-border`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn CommandSeparator(children: Option<Element>) -> Element {
    let _ = children;
    let _ = require_command("CommandSeparator");
    Element::from(separator())
        .w("auto")
        .mx("-0.25rem")
        .my("0.25rem")
        .pointer_events_none()
}

/// ShadCN / solid-ui `CommandItem`：高亮 `bg-accent`；可选 check；有 Shortcut 时不显示 check。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn CommandItem(
    value: String,
    disabled: bool,
    checked: bool,
    #[prop(default = "Handler::default()")] mut on_click: Handler,
    children: Option<Element>,
) -> Element {
    let ctx = require_command("CommandItem");
    let label_text = match &children {
        Some(c) => c
            .text
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| extract_text_leaf(c)),
        None => value.clone(),
    };
    let query = query_of(&ctx);
    if !label_matches_query(&label_text, &query) {
        let mut hidden = Element::new().w(0.0).h(0.0);
        hidden.set_attr("rui_command_hidden", "1");
        return hidden;
    }

    let value = if value.is_empty() {
        label_text.clone()
    } else {
        value
    };
    let on_click = Rc::new(RefCell::new(on_click));
    let activate = {
        let on_click = on_click.clone();
        Rc::new(move || on_click.borrow_mut().call(())) as Rc<dyn Fn()>
    };
    let activate_click = activate.clone();
    let ix = {
        let mut list = ctx.visible.peek();
        let ix = list.len();
        list.push(CommandVisibleItem {
            value: value.clone(),
            label: label_text.clone(),
            disabled,
        });
        ctx.visible.set(list);
        push_activate(&ctx.id, activate);
        ix
    };

    let highlighted = ctx.active.peek() == ix;
    let open = ctx.open;
    let active = ctx.active;
    let dismissible = ctx.surface.dismissible();
    let bid = format!("{}:item:{}", ctx.id, value);

    let mut inner_kids = children.map(take_packed).unwrap_or_else(|| {
        vec![Element::new().text(label_text.clone())]
    });
    let show_check = checked && !inner_kids.iter().any(has_shortcut_slot);
    if show_check {
        let g = crate::utils::lucide_glyph("check").unwrap_or_else(|| "✓".into());
        inner_kids.push(
            build_icon_text_node(g)
                .text_size(16.0)
                .text_color(parse_text_color(if highlighted {
                    "accent-foreground"
                } else {
                    "foreground"
                }))
                .w(16.0)
                .h(16.0)
                .flex_shrink(0.0)
                .pointer_events_none(),
        );
    }

    let mut row_inner = hstack()
        .w("100%")
        .min_w(0.0)
        .items_center()
        .gap("0.5rem");
    for k in inner_kids {
        row_inner = row_inner.child(k);
    }

    let mut row: Element = button(&bid)
        .variant("ghost")
        .size("sm")
        .w("100%")
        .disabled(disabled)
        .on_click(move || {
            if disabled {
                return;
            }
            active.set(ix);
            activate_click();
            if dismissible {
                open.set(false);
                clear_button_focus();
            }
        })
        .child(row_inner)
        .into();

    row = row
        .w("100%")
        .min_w(0.0)
        .h("auto")
        .min_h("auto")
        .rounded("0.125rem")
        .gap("0.5rem")
        .py("0.375rem")
        .px("0.5rem")
        .justify_start();
    if highlighted {
        row = row.bg(parse_color("accent"));
    }
    if disabled {
        row = row.opacity(0.5);
    }
    row
}

/// `CommandShortcut`：`ml-auto text-xs tracking-widest text-muted-foreground`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn CommandShortcut(children: Option<Element>) -> Element {
    let _ = require_command("CommandShortcut");
    let t = match &children {
        Some(c) => c
            .text
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| extract_text_leaf(c)),
        None => String::new(),
    };
    let mut el: Element = label(format!("cmd-sc:{t}"))
        .text(t)
        .size(12.0)
        .color(parse_text_color("muted-foreground"))
        .into();
    el = el.flex_shrink(0.0).ml("auto");
    el.set_attr("data-slot", "command-shortcut");
    el
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::portal_overlay::clear_overlay_stack;
    use crate::core::reactivity::create_root;
    use crate::layout::layout_tree::{
        hoist_portal_subtrees, LayoutTree, ATTR_HOISTED_PORTAL, ATTR_PORTAL_KEY, PORTAL_LAYER_ID,
    };
    use taffy::style::Dimension;

    #[test]
    fn command_action_holds_label() {
        let a = CommandAction::new("ping", "Ping host", || {}).shortcut("⌘P");
        assert_eq!(a.id, "ping");
        assert_eq!(a.label, "Ping host");
        assert_eq!(a.shortcut.as_deref(), Some("⌘P"));
    }

    #[test]
    fn command_overlay_frame_hoists_viewport_width_bar() {
        clear_overlay_stack();
        let (_owner, mut root) = create_root(|| {
            let open = Signal::new(true);
            let oz = OverlayZ {
                underlay: 1000,
                panel: 1001,
            };
            let inner = Element::new().w(200.0).h(48.0).bg([0.2, 0.2, 0.2, 1.0]);
            let frame = command_overlay_frame(open, oz, inner);
            assert!(
                matches!(frame.style.size.width, Dimension::Percent(p) if (p - 1.0).abs() < 0.01),
                "overlay root must be 100% wide (Sheet contract), not 0; got {:?}",
                frame.style.size.width
            );
            assert!(
                matches!(frame.style.size.height, Dimension::Percent(p) if (p - 1.0).abs() < 0.01),
                "overlay root must be 100% tall, not 0; got {:?}",
                frame.style.size.height
            );
            Element::new().w(800.0).h(600.0).child(frame)
        });

        let lifted = hoist_portal_subtrees(&mut root);
        assert!(
            lifted >= 2,
            "underlay + top bar must hoist; lifted={lifted}"
        );

        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);

        let layer = root
            .children
            .iter()
            .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
            .expect("portal layer");
        let bar = layer
            .children
            .iter()
            .find(|c| {
                c.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1")
                    && c.attrs.get(ATTR_PORTAL_KEY).map(String::as_str) == Some("panel:1001")
            })
            .expect("hoisted command bar");
        assert!(
            bar.layout_bounds[2] > 700.0,
            "command bar must span the viewport after hoist, got {:?}",
            bar.layout_bounds
        );
        assert!(
            bar.layout_bounds[3] > 8.0,
            "command bar must have visible height after hoist, got {:?}",
            bar.layout_bounds
        );
        clear_overlay_stack();
    }
}
