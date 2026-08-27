//! # Command
//!
//! 对齐 shadcn/ui Base [`command.tsx`](https://github.com/shadcn-ui/ui) 与 solid-ui `command.tsx`：
//! 复合件 **Command / CommandInput / CommandList / CommandEmpty / CommandGroup /
//! CommandSeparator / CommandItem / CommandShortcut**。
//!
//! 三种表面：
//! - **行内** [`Command`]：嵌在 Card 等布局里。
//! - **Dialog** [`CommandDialog`]：组合 Dialog（居中模态，`show_close=false`），不改 `dialog.rs`。
//! - **纯浮层** [`command_overlay_scope`] / `<CommandOverlay>`：ShadCN CommandDialog 式视口 **top-1/3** 居中；
//!   舞台走 [`mount_viewport_portal_stage`](crate::utils::popup::mount_viewport_portal_stage)（非 Dialog 居中 chrome）。
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
use std::rc::Rc;
use std::sync::OnceLock;

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode, OverlayZ};
use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized};
use crate::components::dialog::{dialog_content, dialog_scope, use_dialog, DialogContentOpts};
use crate::components::input::{
    input, input_caret_signal_key, input_selection_signal_key, input_value_signal_key, lucide_named,
};
use crate::components::label::label;
use crate::components::scroll_area::scroll_area;
use crate::components::separator::separator;
use crate::core::reactivity::{
    batch, create_context, provide_context, use_context, use_signal, with_child_owner, Context,
    Signal,
};
use crate::core::tree::{CursorKind, Element, Handler, LayoutStatus};
use crate::layout::layout_tree::{LayoutTree, ATTR_ESCAPE_OVERFLOW_CLIP};
use crate::primitives::interactive::{interactive, mark_hover_paint_dirty};
use crate::primitives::r#box::{hstack, vstack};
use crate::primitives::text::text;
use crate::style::{parse_color, parse_text_color};
use crate::tokens;
use crate::utils::activate_registry;
use crate::utils::popup::{
    mount_modal_layer, mount_viewport_portal_stage, swallow_clicks, underlay_close_on_click,
};
use crate::utils::text_field::input_focus_id;
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

fn reset_activates(id: &str) {
    activate_registry::reset(id);
}

fn push_activate(id: &str, f: Rc<dyn Fn()>) {
    activate_registry::push(id, f);
}

fn take_activate(id: &str, ix: usize) -> Option<Rc<dyn Fn()>> {
    activate_registry::take(id, ix)
}

fn is_palette(surface: CommandSurface) -> bool {
    matches!(surface, CommandSurface::Overlay | CommandSurface::Dialog)
}

fn reset_command_session(id: &str) {
    let filter_id = format!("{id}__filter");
    use_signal(input_value_signal_key(&filter_id).as_str(), String::new).set(String::new());
    use_signal(
        input_caret_signal_key(&filter_id).as_str(),
        || None::<usize>,
    )
    .set(None);
    use_signal(input_selection_signal_key(&filter_id).as_str(), || {
        None::<(usize, usize)>
    })
    .set(None);
    use_signal(format!("{id}__cmd_active").as_str(), || 0usize).set(0);
}

/// 过滤框只拦截列表导航 / 执行 / 关闭。退格、Delete、左右方向、Home/End、字符一律交给文本编辑。
fn command_filter_consumes_key(key: &Key) -> bool {
    crate::utils::list_nav::is_list_nav_key(key)
}

fn set_command_active(active: Signal<usize>, next: usize) {
    if active.peek() != next {
        active.set_silent(next);
        mark_hover_paint_dirty();
    }
}

/// solid-ui / shadcn Command 根：`flex size-full flex-col overflow-hidden rounded-md bg-popover
/// text-popover-foreground`。
/// DialogContent 固定 `p-1.5rem`；面板内用负边距抵消，对齐 CommandDialog `p-0`（不改 dialog.rs）。
/// Overlay / Dialog 按内容撑开；勿 `overflow:hidden` + `min_h:0`（flex 会把固有高度压成 0）。
fn command_root_shell(surface: CommandSurface, kids: Vec<Element>) -> Element {
    let hug = is_palette(surface);
    let kids: Vec<Element> = if hug {
        kids.into_iter()
            .map(|k| k.flex_shrink(0.0).flex_grow(0.0))
            .collect()
    } else {
        kids
    };
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
    match surface {
        CommandSurface::Dialog => {
            // CommandDialog `hug_content` 面板已有 bg/圆角；勿再铺一层方形 popover 底。
        }
        CommandSurface::Inline => {
            root = root
                .bg(tokens::popover())
                .rounded("0.375rem")
                .border_style("solid")
                .border_width(1.0)
                .border_color(tokens::border());
        }
        CommandSurface::Overlay => {
            // Chrome (bg / radius / border) lives on `command_overlay_frame`. A second
            // full-bleed popover fill here is square and covers the panel's left radii.
        }
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
                filter_id: filter_id.clone(),
                surface,
            },
        );
        let _ = query_sig.get();
        visible.set(Vec::new());
        reset_activates(&id);
        if active.peek() != 0 && visible.peek().is_empty() {
            active.set_silent(0);
        }
        if is_palette(surface) && open.peek() {
            use_signal("__rui_button_focus_id", String::new).set(input_focus_id(&filter_id));
        }
        command_root_shell(surface, take_packed(body()))
    });
    // DialogContent 会给返回根写 `min_h:0` + `flex_grow:1`。固有高度放在内层，避免被压成 0。
    if surface == CommandSurface::Dialog {
        vstack().w("100%").flex_shrink(0.0).child(painted).into()
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
    let open = open.unwrap_or_else(|| use_signal(format!("{id}__open").as_str(), || false));
    let reset_view = format!("{id}__cmd_reset");
    reactive_anchor(reset_view.clone(), {
        let reset_id = id.clone();
        let open = open;
        move || {
            if !open.get() {
                reset_command_session(&reset_id);
            }
            Element::new()
        }
    });
    let host = dialog_scope(id.clone(), Some(open), move || {
        let open = use_dialog()
            .expect("command_dialog_scope: dialog_scope")
            .open;
        let id = id.clone();
        let placeholder = placeholder.clone();
        let body = body.clone();
        dialog_content(
            DialogContentOpts {
                show_close,
                hug_content: true,
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
        .child(reactive_placeholder_sized(reset_view, 0.0, 0.0).pointer_events_box_none())
}

/// VS Code 式命令面板：视口顶部居中，透明点击关闭层，不走 Dialog。
///
/// 宿主对齐 [`command_dialog_scope`] / Select：`0×0` 锚点，避免在 gallery 卡片
/// `VStack` / `ScrollArea` 里被 `100%×100%` 占位撑破或压成 0。
/// 舞台对齐已能显示的 Dialog：`inset(0)` 铺满 portal 层，再在其上顶部居中放面板。
/// 不要用「只有 left/right、高度 auto」的绝对定位 `hstack` 当提升根——在 auto 高父级里
/// Taffy 交叉轴 stretch 会把面板高度收成 0，gallery 里看起来就像没打开。
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
    reactive_anchor(view_id.clone(), {
        let id = id.clone();
        let body = body.clone();
        let placeholder = placeholder.clone();
        move || {
            let _ = open.get();
            let Some(oz) = portal_sync(&id, open, OverlayKind::Modal, OverlayMode::Stacked) else {
                reset_command_session(&id);
                return Element::new();
            };
            if !open.peek() {
                reset_command_session(&id);
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
        }
    });
    // 锚点必须是子节点：gallery 用 `let el = ui! { <CommandOverlay/> }` 时，顶层 `ui!`
    // 会 `template_id(file:line:column)` 盖掉根节点身份，`try_reconcile_subtree` 就找不到
    // `{id}__cmd_overlay`，打开后的舞台被丢掉。CommandDialog 的 0×0 外壳同理。
    Element::new()
        .w(0.0)
        .h(0.0)
        .pointer_events_box_none()
        .child(reactive_placeholder_sized(view_id, 0.0, 0.0).pointer_events_box_none())
}

/// 透明 underlay + 视口舞台（ShadCN `top-1/3` 水平居中面板）。提升的是铺满视口的舞台。
fn command_overlay_frame(open: Signal<bool>, oz: OverlayZ, inner: Element) -> Element {
    // ShadCN aria CommandDialog：`top-1/3 translate-y-0 overflow-hidden p-0`；
    // 宽度对齐 DialogContent `w-full max-w-lg`。solid-ui 为 `overflow-hidden p-0`。
    let mut panel: Element = vstack()
        .w("32rem")
        .min_w("16rem")
        .max_w("90dvw")
        .flex_grow(0.0)
        .flex_shrink(0.0)
        .child(inner)
        .into();
    panel = panel
        .bg(tokens::popover())
        .rounded("0.5rem")
        .border_style("solid")
        .border_width(1.0)
        .border_color(tokens::border())
        .shadow([0.0, 0.0, 0.0, 0.16], 16.0);
    // 勿 `overflow:hidden`：Taffy 会把该节点当成滚动盒，auto 高下列表主轴被压成 0
    //（只剩 flex_shrink:0 的搜索行）。圆角由面板自身 SDF 绘制，不依赖裁剪。
    panel.set_attr(ATTR_ESCAPE_OVERFLOW_CLIP, "1");
    // 舞台已 portal；勿再 `panel_block_clicks`（会嵌套 `rui_portal`，把面板 hoist 出居中舞台）。
    panel = swallow_clicks(panel);

    // 全宽行 + justify center：避免列上 `align-items:center` 把面板收成 min-content，
    // 或 stretch 成主栏全宽（左缘贴侧栏分割线，圆角被 overflow AABB 切成竖边）。
    let row: Element = hstack()
        .w("100%")
        .flex_shrink(0.0)
        .flex_grow(0.0)
        .justify("center")
        .items("start")
        .child(panel)
        .into();

    // `fixed`：视口包含块 + collect 清祖先 overflow clip（即使 hoist 尚未打上 hoisted 标记）。
    let mut stage: Element = vstack()
        .inset(0.0)
        .w("100%")
        .h("100%")
        .justify("start")
        .pt("33.333dvh")
        .child(row)
        .into();
    stage = mount_viewport_portal_stage(stage, oz.panel);

    mount_modal_layer(underlay_close_on_click(open, oz.underlay), stage)
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
        .h("100%")
        .text_size(14.0)
        .into();
    field = field
        .flex_grow(1.0)
        .min_w(0.0)
        .border_width(0.0)
        .rounded(0.0)
        .px(0.0)
        .py(0.0)
        .bg([0.0, 0.0, 0.0, 0.0]);
    field.style.shadow_blur = 0.0;
    field.style.shadow_spread = 0.0;
    field.style.shadow_color = [0.0, 0.0, 0.0, 0.0];
    if let Some(sync) = field.input_paint_sync.as_mut() {
        sync.plain_chrome = true;
    }

    let prev_key = field.on_key_down.clone();
    field = field.on_key_down(move |ev| {
        if !command_filter_consumes_key(&ev.logical_key) {
            if let Some(prev) = &prev_key {
                return prev.borrow_mut()(ev);
            }
            return false;
        }
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
            Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::ArrowUp) => {
                if let Some(next) = crate::utils::list_nav::list_nav_move_arrows(
                    &ev.logical_key,
                    n,
                    active.peek(),
                ) {
                    set_command_active(active, next);
                }
                return true;
            }
            Key::Named(NamedKey::Enter) => {
                let ix = active.peek();
                if let Some(item) = list.get(ix) {
                    if !item.disabled {
                        batch(|| {
                            if let Some(act) = take_activate(&cmd_id, ix) {
                                act();
                            }
                            if dismissible {
                                open.set(false);
                                clear_button_focus();
                            }
                        });
                    }
                    return true;
                }
            }
            _ => {}
        }
        false
    });

    // 搜索图标在圆角输入壳内部左侧；壳与面板之间留白，底部分割线不顶满宽。
    let mut shell: Element = hstack()
        .w("100%")
        .h("2.25rem")
        .px("0.75rem")
        .gap("0.5rem")
        .items_center()
        .child(icon)
        .child(field)
        .into();
    shell = shell
        .rounded("0.375rem")
        .bg(tokens::muted())
        .border_style("solid")
        .border_width(1.0)
        .border_color(tokens::border())
        .overflow("hidden");

    let hairline: Element = hstack().w("100%").px("0.75rem").child(separator()).into();

    vstack()
        .w("100%")
        .flex_shrink(0.0)
        .pt("0.5rem")
        .pb("0.5rem")
        .gap("0.5rem")
        .child(Element::from(hstack().w("100%").px("0.5rem").child(shell)))
        .child(hairline)
        .into()
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
        set_command_active(ctx.active, 0);
    } else if ctx.active.peek() >= n {
        set_command_active(ctx.active, n - 1);
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
    // Overlay / Dialog：按内容撑开。不要写死 `h:`（会矮于分组标题+项，Billing 会画出白底）。
    // 也不要 `overflow:auto`（auto 高父级里 Taffy 滚动盒高度为 0）。
    let mut list: Element = vstack()
        .w("100%")
        .flex_shrink(0.0)
        .flex_grow(0.0)
        .child(col)
        .into();
    list.set_attr("data-slot", "command-list");
    list
}

fn command_empty_ui(msg: &str) -> Element {
    // solid-ui CommandEmpty：`py-6 text-center text-sm`
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
            .font_weight(500.0)
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
    let mut el: Element = hstack()
        .w("100%")
        .px("0.75rem")
        .py("0.25rem")
        .child(separator())
        .into();
    el = el.pointer_events_none();
    el
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
    let value = if value.is_empty() {
        label_text.clone()
    } else {
        value
    };
    let query = query_of(&ctx);
    if !label_matches_query(&label_text, &query) && !label_matches_query(&value, &query) {
        let mut hidden = Element::new().w(0.0).h(0.0);
        hidden.set_attr("rui_command_hidden", "1");
        return hidden;
    }
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

    let mut inner_kids = children
        .map(take_packed)
        .unwrap_or_else(|| vec![Element::new().text(label_text.clone())]);
    let show_check = checked && !inner_kids.iter().any(has_shortcut_slot);
    if show_check {
        let g = crate::utils::lucide_glyph("check").unwrap_or_else(|| "✓".into());
        inner_kids.push(
            build_icon_text_node(g)
                .text_size(14.0)
                .text_color(parse_text_color(if highlighted {
                    "accent-foreground"
                } else {
                    "popover-foreground"
                }))
                .w(14.0)
                .h(14.0)
                .flex_shrink(0.0)
                .pointer_events_none(),
        );
    }

    let item_fg = parse_text_color(if highlighted {
        "accent-foreground"
    } else {
        "popover-foreground"
    });
    for k in &mut inner_kids {
        style_command_item_text(k, item_fg);
    }

    let mut row = hstack()
        .w("100%")
        .min_w(0.0)
        .items_center()
        .gap("0.5rem")
        .h("auto")
        .min_h("2rem")
        .flex_shrink(0.0)
        .rounded("0.375rem")
        .py("0.25rem")
        .px("0.5rem")
        .justify("start");
    for k in inner_kids {
        row = row.child(k);
    }
    let mut row: Element = row.into();
    row = row.rounded("0.375rem");
    if highlighted {
        row = row.bg(parse_color("accent")).text_color(item_fg);
    } else {
        row = row.bg([0.0, 0.0, 0.0, 0.0]).text_color(item_fg);
    }
    if disabled {
        row = row.opacity(0.5);
    }

    let mut hit: Element = interactive(row)
        .on_hover(move |h| {
            // 禁止 `Signal::set`：会在 hover 分发中途重建整棵 Command 树，
            // 导致高亮卡死、后续项收不到悬停、点击（Toast）失效。
            if h && !disabled {
                set_command_active(active, ix);
            }
        })
        .on_click(move || {
            if disabled {
                return;
            }
            set_command_active(active, ix);
            // 与 Dialog Confirm 相同：先跑 on_click（例如入队 Toast），再关浮层。
            batch(|| {
                activate_click();
                if dismissible {
                    open.set(false);
                    clear_button_focus();
                }
            });
        })
        .cursor_kind(if disabled {
            CursorKind::NotAllowed
        } else {
            CursorKind::Pointer
        })
        .into();
    hit.set_attr("data-slot", "command-item");
    hit.set_attr("rui_command_id", ctx.id.clone());
    hit.set_attr("rui_command_ix", ix.to_string());
    if disabled {
        hit.set_attr("rui_command_disabled", "1");
    }
    hit
}

fn style_command_item_text(el: &mut Element, color: [u8; 4]) {
    if el.attrs.get("data-slot").map(String::as_str) == Some("command-shortcut") {
        el.text_style.size = 12.0;
        el.text_style.line_height_px = Some(16.0);
        el.text_style.letter_spacing = 0.4;
        el.text_style.color = parse_text_color("muted-foreground");
        return;
    }
    if el.text.is_some() {
        let lucide = el.text_style.font_family.as_deref() == Some("lucide");
        if lucide {
            el.text_style.size = 14.0;
            el.text_style.line_height_px = Some(14.0);
        } else {
            el.text_style.size = 13.0;
            el.text_style.line_height_px = Some(16.0);
            el.text_style.line_height_scale = Some(1.0);
        }
        el.text_style.color = color;
    }
    for child in &mut el.children {
        style_command_item_text(child, color);
    }
}

/// 每帧按 `{id}__cmd_active` 写回高亮（`peek`，不订阅根 effect）。
pub(crate) fn sync_command_item_paint_subtree(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    if el.attrs.get("data-slot").map(String::as_str) == Some("command-item") {
        let disabled = el.attrs.get("rui_command_disabled").map(String::as_str) == Some("1");
        let ix = el
            .attrs
            .get("rui_command_ix")
            .and_then(|s| s.parse::<usize>().ok());
        let id = el.attrs.get("rui_command_id").cloned();
        if let (Some(id), Some(ix)) = (id, ix) {
            let active = use_signal(format!("{id}__cmd_active").as_str(), || 0usize).peek();
            let on = !disabled && active == ix;
            apply_command_item_highlight(el, layout_tree, impact, on);
        }
    }
    for child in &mut el.children {
        sync_command_item_paint_subtree(child, layout_tree, impact);
    }
}

fn apply_command_item_highlight(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
    on: bool,
) {
    let want_bg = if on {
        parse_color("accent")
    } else {
        [0.0, 0.0, 0.0, 0.0]
    };
    let want_fg = parse_text_color(if on {
        "accent-foreground"
    } else {
        "popover-foreground"
    });
    let bg_changed = el.style.bg_color != want_bg;
    if bg_changed {
        el.style.bg_color = want_bg;
        layout_tree.update_taffy_node_style(el);
        *impact = impact.merge(LayoutStatus::StyleChanged);
    }
    paint_command_item_fg(el, want_fg);
}

fn paint_command_item_fg(el: &mut Element, fg: [u8; 4]) {
    if el.attrs.get("data-slot").map(String::as_str) == Some("command-shortcut") {
        return;
    }
    if el.text.is_some() && el.text_style.font_family.as_deref() != Some("lucide") {
        el.text_style.color = fg;
    }
    for child in &mut el.children {
        paint_command_item_fg(child, fg);
    }
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
    let mut kbd: Element = label(format!("cmd-sc:{t}"))
        .text(t)
        .size(12.0)
        .line_height(16.0)
        .color(parse_text_color("muted-foreground"))
        .into();
    kbd = kbd.whitespace_nowrap().letter_spacing(0.4);
    let mut el: Element = hstack()
        .flex_shrink(0.0)
        .ml("auto")
        .items_center()
        .px("0.375rem")
        .py("0.125rem")
        .rounded("0.25rem")
        .bg(tokens::muted())
        .child(kbd)
        .into();
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

    #[test]
    fn command_action_holds_label() {
        let a = CommandAction::new("ping", "Ping host", || {}).shortcut("⌘P");
        assert_eq!(a.id, "ping");
        assert_eq!(a.label, "Ping host");
        assert_eq!(a.shortcut.as_deref(), Some("⌘P"));
    }

    fn find_portal_layer(root: &Element) -> &Element {
        root.children
            .iter()
            .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
            .expect("portal layer")
    }

    fn find_hoisted_panel<'a>(root: &'a Element, key: &str) -> &'a Element {
        find_portal_layer(root)
            .children
            .iter()
            .find(|c| {
                c.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1")
                    && c.attrs.get(ATTR_PORTAL_KEY).map(String::as_str) == Some(key)
            })
            .expect("hoisted command stage")
    }

    fn overlay_palette_panel(stage: &Element) -> &Element {
        let row = stage.children.first().expect("centering row");
        row.children.first().expect("palette panel")
    }

    #[test]
    fn command_overlay_frame_hoists_viewport_stage() {
        clear_overlay_stack();
        let (_owner, mut root) = create_root(|| {
            let open = Signal::new(true);
            let oz = OverlayZ {
                underlay: 1000,
                panel: 1001,
            };
            let inner = Element::new().w(200.0).h(48.0).bg([0.2, 0.2, 0.2, 1.0]);
            let frame = command_overlay_frame(open, oz, inner);
            Element::new().w(800.0).h(600.0).child(frame)
        });

        let lifted = hoist_portal_subtrees(&mut root);
        assert!(lifted >= 2, "underlay + stage must hoist; lifted={lifted}");

        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);

        let stage = find_hoisted_panel(&root, "panel:1001");
        assert!(
            stage.layout_bounds[2] > 700.0 && stage.layout_bounds[3] > 500.0,
            "command stage must fill the viewport after hoist, got {:?}",
            stage.layout_bounds
        );
        let panel = overlay_palette_panel(stage);
        assert!(
            panel.layout_bounds[3] > 40.0,
            "palette panel must have visible height, got {:?}",
            panel.layout_bounds
        );
        assert!(
            (panel.layout_bounds[2] - 512.0).abs() < 8.0 || panel.layout_bounds[2] > 400.0,
            "palette width is max-w-lg (32rem), got {}",
            panel.layout_bounds[2]
        );
        assert!(
            panel.layout_bounds[1] > 150.0 && panel.layout_bounds[1] < 250.0,
            "palette must sit at top-1/3 (ShadCN CommandDialog), got y={}",
            panel.layout_bounds[1]
        );
        clear_overlay_stack();
    }

    #[test]
    fn command_overlay_close_then_toast_does_not_leave_underlay() {
        use crate::components::toast::host::paint_toast_host;
        use crate::components::toast::toast_push;
        clear_overlay_stack();
        let (_owner, mut root) = create_root(|| {
            let open = Signal::new(true);
            let oz = OverlayZ {
                underlay: 1000,
                panel: 1001,
            };
            let frame = command_overlay_frame(open, oz, Element::new().w(200.0).h(48.0));
            Element::new()
                .w(800.0)
                .h(600.0)
                .child(frame)
                .child(paint_toast_host())
        });
        let _ = hoist_portal_subtrees(&mut root);
        assert!(
            find_portal_layer(&root).children.iter().any(|c| c
                .attrs
                .get(ATTR_PORTAL_KEY)
                .map(String::as_str)
                == Some("underlay:1000")),
            "precondition: command underlay is hoisted"
        );

        batch(|| {
            let _ = toast_push("Hello from Command", 2500);
        });
        // Overlay effect returns empty; toast host keeps a non-portal shell + child portal.
        let (_owner2, mut closed) = create_root(|| {
            Element::new()
                .w(800.0)
                .h(600.0)
                .child(Element::new().w(0.0).h(0.0))
                .child(paint_toast_host())
        });
        let _ = hoist_portal_subtrees(&mut closed);
        let layer = find_portal_layer(&closed);
        assert!(
            !layer.children.iter().any(|c| {
                c.attrs
                    .get(ATTR_PORTAL_KEY)
                    .map(|s| s.starts_with("underlay:"))
                    .unwrap_or(false)
            }),
            "closed command must not leave a full-screen underlay that swallows later toast clicks"
        );
        assert!(
            layer.children.iter().any(|c| {
                c.attrs.get(ATTR_PORTAL_KEY).map(String::as_str) == Some("toast-host")
                    && c.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1")
            }),
            "toast must sit on the portal layer after command close"
        );
        let _ = root;
        clear_overlay_stack();
    }

    fn gallery_overlay_body() -> Rc<dyn Fn() -> Element> {
        Rc::new(|| {
            let mk_item = |value: &str, label: &str| {
                CommandItem(CommandItemProps {
                    value: Some(value.into()),
                    disabled: None,
                    checked: None,
                    on_click: None,
                    children: Some(Element::new().text(label)),
                    base: None,
                })
            };
            let mut grp_kids = Element::new();
            grp_kids.set_attr("rui_fragment", "1");
            let group = CommandGroup(CommandGroupProps {
                heading: Some("Suggestions".into()),
                children: Some(grp_kids.children(vec![
                    mk_item("toast", "Show toast"),
                    mk_item("log", "Log ping"),
                ])),
                base: None,
            });
            let mut list_kids = Element::new();
            list_kids.set_attr("rui_fragment", "1");
            let mut root = Element::new();
            root.set_attr("rui_fragment", "1");
            root.children(vec![
                CommandInput(CommandInputProps {
                    placeholder: None,
                    children: None,
                    base: None,
                }),
                CommandList(CommandListProps {
                    children: Some(list_kids.children(vec![group])),
                    base: None,
                }),
            ])
        })
    }

    #[test]
    fn command_overlay_keeps_left_radius_and_list_items() {
        clear_overlay_stack();
        let (_owner, mut root) = create_root(|| {
            let open = Signal::new(true);
            let oz = OverlayZ {
                underlay: 1000,
                panel: 1001,
            };
            let inner = command_paint(
                "gal-cmd".into(),
                open,
                "Type a command or search...".into(),
                CommandSurface::Overlay,
                gallery_overlay_body(),
            );
            let frame = command_overlay_frame(open, oz, inner);
            let host = Element::new()
                .w(0.0)
                .h(0.0)
                .pointer_events_box_none()
                .child(frame);
            let side = Element::new().w(208.0).h("100%").bg([0.1, 0.1, 0.1, 1.0]);
            let body: Element = scroll_area("cmd-overlay-main")
                .overflow_y("auto")
                .flex_grow(1.0)
                .min_w(0.0)
                .h("100%")
                .child(host)
                .into();
            Element::new().w(800.0).h(600.0).child(
                Element::new()
                    .w("100%")
                    .h("100%")
                    .flex_row()
                    .child(side)
                    .child(body),
            )
        });

        let _ = hoist_portal_subtrees(&mut root);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);

        let stage = find_hoisted_panel(&root, "panel:1001");
        let panel = overlay_palette_panel(stage);
        assert!(
            panel.style.radii.iter().all(|r| *r >= 7.0),
            "overlay panel must keep all four corner radii, got {:?}",
            panel.style.radii
        );
        assert!(
            panel.layout_bounds[0] > 8.0,
            "panel must not be flush-clipped on the left (sidebar), got x={}",
            panel.layout_bounds[0]
        );
        let px = panel.layout_bounds[0];
        let pw = panel.layout_bounds[2];
        let overlay = &result.instances[result.overlay_split.instance_start..];
        let panel_inst = overlay
            .iter()
            .find(|i| {
                i.color[3] > 0.5
                    && (i.rect_pos[0] - px).abs() < 2.0
                    && (i.rect_size[0] - pw).abs() < 4.0
                    && i.radii.iter().any(|r| *r >= 7.0)
            })
            .expect("panel chrome in overlay paint suffix");
        let clip_cuts_left = panel_inst.clip_size[0] > 0.0
            && panel_inst.clip_size[1] > 0.0
            && panel_inst.clip_pos[0] > px + 1.0;
        assert!(
            !clip_cuts_left,
            "main ScrollArea must not scissor overlay left radii (sidebar edge); clip_pos={:?} clip_size={:?} panel_x={}",
            panel_inst.clip_pos,
            panel_inst.clip_size,
            px
        );
        let mut items = 0usize;
        walk_cmd(panel, &mut |el| {
            if el.attrs.get("data-slot").map(String::as_str) == Some("command-item") {
                items += 1;
                assert!(
                    el.layout_bounds[3] > 10.0,
                    "command item must have visible height, got {:?}",
                    el.layout_bounds
                );
            }
        });
        assert!(items >= 2, "overlay list items must paint, got {items}");
        assert!(
            panel.layout_bounds[3] > 80.0,
            "overlay must be taller than the search row, got h={}",
            panel.layout_bounds[3]
        );
        clear_overlay_stack();
    }

    fn find_tid(el: &Element, id: &str) -> bool {
        el.template_id.as_deref() == Some(id) || el.children.iter().any(|c| find_tid(c, id))
    }

    fn collect_tids(el: &Element) -> Vec<String> {
        let mut out = Vec::new();
        fn walk(el: &Element, out: &mut Vec<String>) {
            if let Some(t) = &el.template_id {
                out.push(t.clone());
            }
            for c in &el.children {
                walk(c, out);
            }
        }
        walk(el, &mut out);
        out
    }

    /// Gallery: `let cmd_el = ui! { <CommandOverlay/> }` stamps `file:line:column` on the
    /// **root**. The reactive anchor id must still be findable underneath.
    #[test]
    fn command_overlay_anchor_survives_ui_root_template_stamp() {
        clear_overlay_stack();
        crate::app::reactive::reset_registered_anchors();
        let (_owner, stamped) = create_root(|| {
            let open = use_signal("stamp_cmd_open", || false);
            let host = command_overlay_scope("gal-cmd", Some(open), "", || {
                Element::new().w(200.0).h(48.0)
            });
            host.template_id("examples/gallery/src/main.rs:2718:9")
        });
        assert_eq!(
            stamped.template_id.as_deref(),
            Some("examples/gallery/src/main.rs:2718:9"),
            "ui! stamps the wrapper"
        );
        assert!(
            find_tid(&stamped, "gal-cmd__cmd_overlay"),
            "overlay reactive placeholder must remain a descendant after ui! root stamp; tree tids: {:?}",
            collect_tids(&stamped)
        );
        clear_overlay_stack();
        crate::app::reactive::reset_registered_anchors();
    }

    /// Gallery shape: overlay host sits in an auto-height card inside a ScrollArea
    /// (percent height collapses). Auto-height command body must still paint.
    #[test]
    fn command_overlay_visible_in_gallery_scroll_card() {
        clear_overlay_stack();
        let (_owner, mut root) = create_root(|| {
            let open = Signal::new(true);
            let oz = OverlayZ {
                underlay: 1000,
                panel: 1001,
            };
            let inner: Element = vstack()
                .w("100%")
                .child(Element::new().w("100%").h(36.0).bg([0.2, 0.2, 0.2, 1.0]))
                .child(Element::new().w("100%").h(80.0).bg([0.3, 0.3, 0.3, 1.0]))
                .into();
            let frame = command_overlay_frame(open, oz, inner);
            let host = Element::new()
                .w(0.0)
                .h(0.0)
                .pointer_events_box_none()
                .child(frame);
            let card: Element = vstack()
                .w("100%")
                .child(Element::new().w("100%").h(40.0))
                .child(host)
                .into();
            Element::new().w(800.0).h(600.0).child(
                Element::new()
                    .w("100%")
                    .h("100%")
                    .overflow_y("auto")
                    .child(card),
            )
        });

        let lifted = hoist_portal_subtrees(&mut root);
        assert!(lifted >= 2, "underlay + stage must hoist; lifted={lifted}");

        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);

        let stage = find_hoisted_panel(&root, "panel:1001");
        assert!(
            stage.layout_bounds[2] > 700.0 && stage.layout_bounds[3] > 500.0,
            "gallery host is 0×0; stage must still fill the viewport, got {:?}",
            stage.layout_bounds
        );
        let panel = overlay_palette_panel(stage);
        assert!(
            panel.layout_bounds[3] > 100.0,
            "auto-height command body must not collapse in gallery card, got {:?}",
            panel.layout_bounds
        );
        assert!(
            panel.layout_bounds[2] > 200.0,
            "command panel must have visible width, got {:?}",
            panel.layout_bounds
        );
        clear_overlay_stack();
    }

    #[test]
    fn command_filter_lets_editing_keys_through() {
        assert!(!command_filter_consumes_key(&Key::Named(
            NamedKey::Backspace
        )));
        assert!(!command_filter_consumes_key(&Key::Named(NamedKey::Delete)));
        assert!(!command_filter_consumes_key(&Key::Named(
            NamedKey::ArrowLeft
        )));
        assert!(!command_filter_consumes_key(&Key::Named(NamedKey::Home)));
        assert!(!command_filter_consumes_key(&Key::Named(NamedKey::End)));
        assert!(!command_filter_consumes_key(&Key::Character("a".into())));
        assert!(command_filter_consumes_key(&Key::Named(
            NamedKey::ArrowDown
        )));
        assert!(command_filter_consumes_key(&Key::Named(NamedKey::Enter)));
        assert!(command_filter_consumes_key(&Key::Named(NamedKey::Escape)));
    }

    #[test]
    fn reset_command_session_clears_filter_and_active() {
        let _owner = create_root(|| {
            let id = "cmd-reset-t";
            let filter_id = format!("{id}__filter");
            use_signal(input_value_signal_key(&filter_id).as_str(), String::new)
                .set("hello".into());
            use_signal(format!("{id}__cmd_active").as_str(), || 0usize).set(3);
            reset_command_session(id);
            assert_eq!(
                use_signal(input_value_signal_key(&filter_id).as_str(), String::new).get(),
                ""
            );
            assert_eq!(
                use_signal(format!("{id}__cmd_active").as_str(), || 0usize).get(),
                0
            );
        });
    }

    fn walk_cmd<'a>(el: &'a Element, f: &mut impl FnMut(&'a Element)) {
        f(el);
        for c in &el.children {
            walk_cmd(c, f);
        }
    }

    fn find_cmd_item(root: &Element) -> &Element {
        let mut found = None;
        walk_cmd(root, &mut |el| {
            if el.attrs.get("data-slot").map(String::as_str) == Some("command-item") {
                found = Some(el);
            }
        });
        found.expect("command-item")
    }

    #[test]
    fn command_dialog_item_click_runs_on_click_then_closes() {
        use crate::core::reactivity::use_signal;
        use std::cell::Cell;
        let ran = Rc::new(Cell::new(false));
        let (_owner, (root, open)) = create_root({
            let ran = ran.clone();
            move || {
                let open = use_signal("cmd-cal-dlg-open", || true);
                let ran = ran.clone();
                let root = command_paint(
                    "cmd-cal-dlg".into(),
                    open,
                    "search".into(),
                    CommandSurface::Dialog,
                    Rc::new(move || {
                        let ran = ran.clone();
                        let item = CommandItem(CommandItemProps {
                            value: Some("calendar".into()),
                            disabled: None,
                            checked: None,
                            on_click: Some(Handler::new(move |_| ran.set(true))),
                            children: Some(Element::new().text("Calendar")),
                            base: None,
                        });
                        let mut grp_kids = Element::new();
                        grp_kids.set_attr("rui_fragment", "1");
                        let group = CommandGroup(CommandGroupProps {
                            heading: Some("Suggestions".into()),
                            children: Some(grp_kids.children(vec![item])),
                            base: None,
                        });
                        let mut list_kids = Element::new();
                        list_kids.set_attr("rui_fragment", "1");
                        let mut frag = Element::new();
                        frag.set_attr("rui_fragment", "1");
                        frag.children(vec![
                            CommandInput(CommandInputProps {
                                placeholder: None,
                                children: None,
                                base: None,
                            }),
                            CommandList(CommandListProps {
                                children: Some(list_kids.children(vec![group])),
                                base: None,
                            }),
                        ])
                    }),
                );
                (root, open)
            }
        });
        find_cmd_item(&root)
            .on_click
            .as_ref()
            .expect("item click")
            .borrow_mut()();
        assert!(ran.get(), "Calendar on_click must run before dismiss");
        assert!(
            !open.peek(),
            "dismissible CommandDialog closes after activate"
        );
    }

    #[test]
    fn command_palette_matches_shadcn_chrome() {
        let (_owner, root) = create_root(|| {
            command_paint(
                "cmd-style".into(),
                Signal::new(true),
                "Type a command or search...".into(),
                CommandSurface::Overlay,
                Rc::new(|| {
                    let mut item_kids = Element::new();
                    item_kids.set_attr("rui_fragment", "1");
                    let item = CommandItem(CommandItemProps {
                        value: Some("home".into()),
                        disabled: None,
                        checked: None,
                        on_click: None,
                        children: Some(item_kids.children(vec![
                            Element::new().text("Home"),
                            CommandShortcut(CommandShortcutProps {
                                children: Some(Element::new().text("⌘H")),
                                base: None,
                            }),
                        ])),
                        base: None,
                    });
                    let mut grp_kids = Element::new();
                    grp_kids.set_attr("rui_fragment", "1");
                    let group = CommandGroup(CommandGroupProps {
                        heading: Some("Navigation".into()),
                        children: Some(grp_kids.children(vec![item])),
                        base: None,
                    });
                    let mut list_kids = Element::new();
                    list_kids.set_attr("rui_fragment", "1");
                    let mut root = Element::new();
                    root.set_attr("rui_fragment", "1");
                    root.children(vec![
                        CommandInput(CommandInputProps {
                            placeholder: None,
                            children: None,
                            base: None,
                        }),
                        CommandList(CommandListProps {
                            children: Some(list_kids.children(vec![group])),
                            base: None,
                        }),
                    ])
                }),
            )
        });

        let mut plain = false;
        let mut item_radius = 0.0f32;
        let mut item_text = 0.0f32;
        let mut item_hover = false;
        let mut shortcut_tracking = 99.0f32;
        walk_cmd(&root, &mut |el| {
            if el.input_paint_sync.as_ref().is_some_and(|s| s.plain_chrome) {
                plain = true;
            }
            if el.attrs.get("data-slot").map(String::as_str) == Some("command-item") {
                item_radius = el.style.radii[0];
                item_hover = el.on_hover.is_some();
                walk_cmd(el, &mut |n| {
                    if n.text.as_deref() == Some("Home") {
                        item_text = n.text_style.size;
                    }
                    if n.text.as_deref() == Some("⌘H") {
                        shortcut_tracking = n.text_style.letter_spacing;
                    }
                });
            }
        });
        assert!(plain, "command filter must suppress the Input focus ring");
        assert!(
            item_radius >= 5.0,
            "command item highlight must be rounded-md, got {item_radius}"
        );
        assert!(item_hover, "command items must highlight on pointer hover");
        assert!(
            (item_text - 13.0).abs() < 0.01,
            "command item label must be 13px, got {item_text}"
        );
        assert!(
            shortcut_tracking < 1.0,
            "shortcut tracking must stay readable, got {shortcut_tracking}"
        );
    }

    #[test]
    fn command_item_hover_paint_moves_without_rebuild() {
        let (_owner, mut root) = create_root(|| {
            command_paint(
                "cmd-hover".into(),
                Signal::new(true),
                "Type".into(),
                CommandSurface::Inline,
                Rc::new(|| {
                    let a = CommandItem(CommandItemProps {
                        value: Some("a".into()),
                        disabled: None,
                        checked: None,
                        on_click: None,
                        children: Some(Element::new().text("Alpha")),
                        base: None,
                    });
                    let b = CommandItem(CommandItemProps {
                        value: Some("b".into()),
                        disabled: None,
                        checked: None,
                        on_click: None,
                        children: Some(Element::new().text("Beta")),
                        base: None,
                    });
                    let mut kids = Element::new();
                    kids.set_attr("rui_fragment", "1");
                    CommandList(CommandListProps {
                        children: Some(kids.children(vec![a, b])),
                        base: None,
                    })
                }),
            )
        });

        fn items(el: &Element) -> Vec<&Element> {
            let mut out = Vec::new();
            fn walk<'a>(el: &'a Element, out: &mut Vec<&'a Element>) {
                if el.attrs.get("data-slot").map(String::as_str) == Some("command-item") {
                    out.push(el);
                }
                for c in &el.children {
                    walk(c, out);
                }
            }
            walk(el, &mut out);
            out
        }

        assert_eq!(items(&root).len(), 2);
        use_signal("cmd-hover__cmd_active", || 0usize).set_silent(1);
        let mut tree = LayoutTree::new();
        let mut impact = crate::core::tree::LayoutStatus::Clean;
        sync_command_item_paint_subtree(&mut root, &mut tree, &mut impact);
        let painted = items(&root);
        let accent = parse_color("accent");
        assert_ne!(
            painted[0].style.bg_color, accent,
            "unhovered row must drop accent"
        );
        assert_eq!(
            painted[1].style.bg_color, accent,
            "active row must take accent via paint sync"
        );
    }
}
