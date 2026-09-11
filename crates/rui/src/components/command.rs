//! # Command
//!
//! Aligns with shadcn/ui Base [`command.tsx`](https://github.com/shadcn-ui/ui) and solid-ui `command.tsx`:
//! Composite **Command / CommandInput / CommandList / CommandEmpty / CommandGroup /
//! CommandSeparator / CommandItem / CommandShortcut**.
//!
//! Three surfaces:
//! - **Inline** [`Command`]: embed in Card or other layout.
//! - **Dialog** [`CommandDialog`]: compose Dialog (centered modal, `show_close=false`). Hug shell is `p-0` (no footer); CommandList absorbs leftover height under `max_h: 85dvh`.
//! - **Pure overlay** [`command_overlay_scope`] / `<CommandOverlay>`: viewport-centered palette (ShadCN `CommandDialog` chrome, no `Dialog` wrapper).
//!   stage via [`mount_viewport_portal_stage`](crate::utils::popup::mount_viewport_portal_stage) (not Dialog centered chrome).
//!
//! cmdk-like behavior: **type-to-filter**, ↑↓ highlight, Enter run; Esc closes overlay / Dialog.
//!
//! ## Usage (composite / front-end style)
//!
//! ```ignore
//! // Inline (nested in Card)
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
//! // Command palette: VS Code-style top-centered overlay
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
//! // Command palette: reuse Dialog (centered modal)
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
//! ## Usage (Builder)
//!
//! ```ignore
//! command("pal")
//!     .open_signal(open) // default: top-centered overlay
//!     .action(CommandAction::new("ping", "Ping", || log::info!("ping")))
//! // Centered Dialog: `.dialog()`
//!     .action(CommandAction::new("ping", "Ping", || log::info!("ping")))
//! ```

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::OnceLock;

use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized};
use crate::components::base::SlotChrome;
use crate::components::dialog::{dialog_content, dialog_scope, use_dialog, DialogContentOpts};
use crate::components::input::{
    input, input_caret_signal_key, input_selection_signal_key, input_value_signal_key, lucide_named,
};
use crate::utils::modal_shell::ShellOverlayOpts;
use crate::utils::viewport_modal::{
    viewport_modal_is_mounted, viewport_modal_layer, viewport_modal_motion_busy,
    ViewportModalLayerOpts,
};
use crate::components::label::label;
use crate::components::scroll_area::scroll_area;
use crate::components::separator::separator;
use crate::core::reactivity::{
    batch, create_context, provide_context, use_context, use_signal, with_child_owner, Context,
    Signal,
};
use crate::core::tree::{CursorKind, Element, Handler, LayoutStatus};
use crate::impl_simple_slot_builder;
use crate::layout::layout_tree::{LayoutTree, ATTR_ESCAPE_OVERFLOW_CLIP};
use crate::primitives::interactive::{interactive, mark_hover_paint_dirty};
use crate::primitives::r#box::{hstack, vstack};
use crate::primitives::text::text;
use crate::style::{parse_color, parse_text_color};
use crate::tokens;
use crate::utils::activate_registry;
use crate::utils::popup::swallow_clicks;
use crate::utils::take_packed;
use crate::utils::text_field::input_focus_id;
use crate::utils::text_node::build_icon_text_node;
use rui_macros::component;
use winit::keyboard::{Key, NamedKey};

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

/// Snapshot of visible items for keyboard / empty state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandVisibleItem {
    pub value: String,
    pub label: String,
    pub disabled: bool,
}

/// Command display surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CommandSurface {
    /// Embedded in existing layout (no portal).
    Inline,
    /// Reuse Dialog: viewport-centered modal.
    Dialog,
    /// VS Code-style: top-centered pure overlay.
    #[default]
    Overlay,
}

impl CommandSurface {
    fn dismissible(self) -> bool {
        !matches!(self, Self::Inline)
    }
}

/// Command root context (shared by composite children).
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

/// Read the nearest [`Command`] context.
pub fn use_command() -> Option<CommandCtx> {
    use_context(command_context())
}

fn require_command(part: &str) -> CommandCtx {
    use_command().unwrap_or_else(|| {
        panic!("{part} must be used within <Command> / <CommandDialog> / <CommandOverlay> / command_scope");
    })
}
fn extract_text_leaf(el: &Element) -> String {
    crate::core::tree::first_text_leaf(el)
}

fn label_matches_query(label: &str, query: &str) -> bool {
    let q = query.to_ascii_lowercase();
    q.is_empty() || label.to_ascii_lowercase().contains(&q)
}

fn has_shortcut_slot(el: &Element) -> bool {
    crate::core::tree::any_in_tree(el, |n| {
        n.attrs.get("data-slot").map(|s| s.as_str()) == Some("command-shortcut")
    })
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

/// Filter box only consumes list nav / run / dismiss. Backspace, Delete, arrows, Home/End, and chars go to text editing.
fn command_filter_consumes_key(key: &Key) -> bool {
    crate::utils::list_nav::is_list_nav_key(key)
}

fn set_command_active(active: Signal<usize>, next: usize) {
    if active.peek() != next {
        active.set_silent(next);
        mark_hover_paint_dirty();
    }
}

/// solid-ui / shadcn Command root: `flex size-full flex-col overflow-hidden rounded-md bg-popover
/// text-popover-foreground`.
/// DialogContent `hug_content` shell is already `p-0` — do not negate phantom `p-1.5rem` (that bleeds past the card).
/// Overlay / Dialog size to content; avoid `overflow:hidden` + `min_h:0` (flex would crush intrinsic height to 0).
fn command_root_shell(surface: CommandSurface, kids: Vec<Element>) -> Element {
    let hug = is_palette(surface);
    let dialog = surface == CommandSurface::Dialog;
    let kids: Vec<Element> = if hug && !dialog {
        kids.into_iter()
            .map(|k| k.flex_shrink(0.0).flex_grow(0.0))
            .collect()
    } else {
        kids
    };
    let mut inner = vstack().w("100%").gap("0").children(kids);
    if !hug || dialog {
        inner = inner.min_h("0");
        if !hug {
            inner = inner.flex_grow(1.0);
        }
    }
    let inner: Element = inner.into();
    let mut root = vstack().w("100%").child(inner);
    if dialog {
        // Card `max_h: 85dvh` must shrink the list, not clip it. Input stays flex_shrink:0.
        root = root.min_h("0").flex_shrink(1.0);
    } else if hug {
        root = root.flex_shrink(0.0);
    } else {
        root = root.min_h("0").flex_grow(1.0).overflow("hidden");
    }
    let mut root: Element = root.into();
    match surface {
        CommandSurface::Dialog => {
            // Chrome (bg / radius / border) lives on `dialog_shell_frame` hug card.
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

/// Provide context and rebuild the shell from the current query (no `reactive_view` registration).
/// Outer Overlay / Dialog effects must call this so they subscribe to query and avoid nested 100% placeholder height collapse.
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
    let open_now = open.peek();
    let painted = with_child_owner({
        let id = id.clone();
        let filter_id = filter_id.clone();
        move || {
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
            // Do not reset `active` here. `visible` was just cleared, so the old
            // `if active != 0 && visible.is_empty()` always fired and forced index 0
            // on every rebuild — including Dialog/Overlay close, while exit animation
            // still shows the list (first-item accent flash).
            // CommandList clamps `active` after items are collected.
            // Palette filter focus is applied in `sync_command_item_paint_subtree`
            // after enter motion is idle — not here, which runs at mount (first
            // enter frame) and would show the caret during zoom/fade.
            command_root_shell(surface, take_packed(body()))
        }
    });
    // DialogContent sets `min_h:0` + `flex_grow:1` on the returned root. Keep intrinsic height on the inner shell so it is not crushed to 0.
    let mut painted = if surface == CommandSurface::Dialog {
        vstack()
            .w("100%")
            .min_h(0.0)
            .flex_shrink(1.0)
            .child(painted)
            .into()
    } else {
        painted
    };
    stamp_command_palette_attrs(&mut painted, &id, surface, open_now, &filter_id);
    painted
}

fn stamp_command_palette_attrs(
    el: &mut Element,
    id: &str,
    surface: CommandSurface,
    open: bool,
    filter_id: &str,
) {
    if !is_palette(surface) {
        return;
    }
    el.set_attr("rui_command_palette", "1");
    el.set_attr("rui_command_id", id);
    el.set_attr(
        "rui_command_surface",
        match surface {
            CommandSurface::Dialog => "dialog",
            CommandSurface::Overlay => "overlay",
            CommandSurface::Inline => "inline",
        },
    );
    el.set_attr("rui_command_filter", filter_id);
    el.set_attr("rui_command_open", if open { "1" } else { "0" });
}

fn command_palette_motion_busy(id: &str, surface: &str) -> bool {
    match surface {
        "dialog" | "overlay" => viewport_modal_motion_busy(id),
        _ => false,
    }
}

fn sync_command_palette_focus(el: &Element) {
    if el.attrs.get("rui_command_palette").map(String::as_str) != Some("1") {
        return;
    }
    if el.attrs.get("rui_command_open").map(String::as_str) != Some("1") {
        return;
    }
    let Some(id) = el.attrs.get("rui_command_id") else {
        return;
    };
    let Some(filter_id) = el.attrs.get("rui_command_filter") else {
        return;
    };
    let surface = el
        .attrs
        .get("rui_command_surface")
        .map(String::as_str)
        .unwrap_or("inline");
    if command_palette_motion_busy(id, surface) {
        return;
    }
    let want = input_focus_id(filter_id);
    let focus = use_signal("__rui_button_focus_id", String::new);
    if focus.peek() != want {
        focus.set(want);
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

/// Provide Command context and build the subtree (shared by `ui!` `<Command>` / Builder).
///
/// When already inside a Command context (e.g. `<Command>` nested in [`command_dialog_scope`]), only wrap the visual shell.
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
    // 100%×100% collapses to 0 inside auto-height parents (Dialog / Overlay).
    reactive_placeholder_sized(view_id, "100%", "auto")
}

/// ShadCN / solid-ui `CommandDialog`: `Dialog` + `DialogContent(show_close=false)` + Command shell.
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
            // Match CommandOverlay: keep highlight stable during exit animation.
            // Resetting `active` to 0 while the dialog is still mounted forces the
            // first item accent via `sync_command_item_paint_subtree` (visible flicker).
            if !viewport_modal_is_mounted(&reset_id, open) {
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
    // DialogContent defaults to 100%×100% placeholder; leaving it in an HStack/card flow blows out layout.
    // A 0×0 anchor still hoists underlay/stage (same as Select / Combobox).
    Element::new()
        .w(0.0)
        .h(0.0)
        .pointer_events_box_none()
        .child(host)
        .child(reactive_placeholder_sized(reset_view, 0.0, 0.0).pointer_events_box_none())
}

/// VS Code-style command palette: viewport-centered panel, transparent dismiss layer, no Dialog.
///
/// Host matches [`command_dialog_scope`] / Select: `0×0` anchor so gallery cards
/// `VStack` / `ScrollArea` are not blown out or crushed by a `100%×100%` placeholder.
/// Stage matches Dialog: `inset(0)` fills the portal layer, then centers the panel.
/// Do not use an absolutely positioned `hstack` with only left/right and auto height as the hoist root — inside an auto-height parent
/// Taffy cross-axis stretch collapses panel height to 0 and the gallery looks like it never opened.
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
            let overlay_opts =
                ShellOverlayOpts::from_content_parts(None, Some(0.0), true, Some(0.0));
            viewport_modal_layer(
                &id,
                open,
                &overlay_opts,
                ViewportModalLayerOpts::dialog_fade(),
                |phase| {
                    if phase.is_none() {
                        reset_command_session(&id);
                        return None;
                    }
                    let inner = command_paint(
                        id.clone(),
                        open,
                        placeholder.clone(),
                        CommandSurface::Overlay,
                        body.clone(),
                    );
                    Some(command_overlay_panel(inner, &id))
                },
            )
        }
    });
    // Anchor must be a child: when gallery does `let el = ui! { <CommandOverlay/> }`, the outer `ui!`
    // stamps `template_id(file:line:column)` over the root identity so `try_reconcile_subtree` cannot find
    // `{id}__cmd_overlay` and the opened stage is dropped. Same for CommandDialog's 0×0 shell.
    Element::new()
        .w(0.0)
        .h(0.0)
        .pointer_events_box_none()
        .child(reactive_placeholder_sized(view_id, 0.0, 0.0).pointer_events_box_none())
}

/// ShadCN CommandDialog chrome (transparent scrim is applied by the viewport shell).
fn command_overlay_panel(inner: Element, overlay_id: &str) -> Element {
    // ShadCN aria CommandDialog: `top-1/3 translate-y-0 overflow-hidden p-0`;
    // width matches DialogContent `w-full max-w-lg`. solid-ui uses `overflow-hidden p-0`.
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
        .shadow(tokens::shadow_rgba(tokens::Elevation::Xl), 16.0)
        .pb("0.5rem");
    // Avoid `overflow:hidden`: Taffy treats the node as a scroll box and crushes the list main axis to 0 under auto height
    //(only the flex_shrink:0 search row remains). Corner radius is painted by the panel SDF, not by clipping.
    panel.set_attr(ATTR_ESCAPE_OVERFLOW_CLIP, "1");
    panel.template_id = Some(format!("{overlay_id}__modal_zoom"));
    // Stage is already portaled; do not wrap `panel_block_clicks` (nested `rui_portal` would hoist the panel out of the centered stage).
    swallow_clicks(panel)
}

// ---------------------------------------------------------------------------
// Data + Builder
// ---------------------------------------------------------------------------

/// One filterable command.
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

/// Builder group.
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

/// Command palette entry. With [`CommandBuilder::open_signal`], defaults to top-centered overlay; [`.dialog()`](CommandBuilder::dialog) uses Dialog.
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
    /// Top-centered pure overlay (default).
    pub fn overlay(mut self) -> Self {
        self.surface = CommandSurface::Overlay;
        self
    }
    /// Reuse Dialog as a centered modal.
    pub fn dialog(mut self) -> Self {
        self.surface = CommandSurface::Dialog;
        self
    }

    /// Embed in existing layout (no portal).
    pub fn inline(mut self) -> Self {
        self.surface = CommandSurface::Inline;
        self
    }

    /// Build Input / List / Item subtrees inside the context.
    pub fn child(self, body: impl Fn() -> Element + 'static) -> Element {
        command_scope(self.id, self.open, self.placeholder, self.surface, body)
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

/// RSX `<Command>`: with a composite subtree uses [`command_scope`]; otherwise the Builder path.
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
        // `ui!` composite path is lowered by the macro to [`command_scope`] (rebuilds each query).
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

/// ShadCN / solid-ui `CommandInput`: bottom border + search icon + transparent input.
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
                if let Some(next) =
                    crate::utils::list_nav::list_nav_move_arrows(&ev.logical_key, n, active.peek())
                {
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

    // Search icon sits inside the rounded input shell on the left; leave gap between shell and panel so the bottom divider is not full-bleed.
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
    // CommandEmpty is a registrar (copy for the no-results UI). It must not occupy
    // a flex slot in the list — a 0×0 stub still participates in column layout
    // once text/min-size resolve, which shoves groups below it downward.
    let kids: Vec<Element> = kids
        .into_iter()
        .filter(|k| k.attrs.get("rui_command_empty_stub").map(String::as_str) != Some("1"))
        .collect();
    let mut col = vstack()
        .w("100%")
        .gap("0")
        .flex_grow(0.0)
        .flex_shrink(0.0)
        .children(kids);
    if ctx.visible.peek().is_empty() {
        col = col.child(command_empty_ui(&ctx.empty_msg.peek()));
    }
    // Content-shell padding-bottom. Column `pb` is invisible to `content_h` (extent
    // is descendant boxes, not own padding), so this is a real child.
    col = col.child(
        Element::new()
            .w("100%")
            .h("0.25rem")
            .flex_shrink(0.0)
            .pointer_events_none(),
    );
    let n = ctx.visible.peek().len();
    if n == 0 {
        set_command_active(ctx.active, 0);
    } else if ctx.active.peek() >= n {
        set_command_active(ctx.active, n - 1);
    }
    let scroll_id = format!("{}:list", ctx.id);
    // ShadCN `max-h-[300px] overflow-y-auto` on every surface. `max_h` on ScrollArea gives a
    // definite scroll-box height under auto-height Dialog / Overlay parents (plain `overflow:auto`
    // on a vstack collapses to 0 in Taffy).
    let mut sa = scroll_area(scroll_id.as_str())
        .overflow_y("auto")
        .overflow_x("hidden")
        .w("100%")
        .max_h("18.75rem");
    if ctx.surface == CommandSurface::Inline {
        sa = sa.min_h("0").flex_grow(1.0);
    } else if ctx.surface == CommandSurface::Dialog {
        // Dialog hug panel is `max_h: 85dvh`. Input is flex_shrink 0; this list is
        // the only body: leftover height, capped at 18.75rem, never taller than the card.
        // Hover mode stays the ScrollArea default (`scrollbar`): item hover must not
        // latch overlay-bar visibility (`area` never auto-hides while the pointer is
        // on a CommandItem, which fills the list).
        sa = sa.h("auto").min_h(0.0).flex_grow(1.0).flex_shrink(1.0);
    } else {
        sa = sa.h("auto").flex_shrink(0.0);
    }
    let mut list: Element = sa.child(col).into();
    list.set_attr("data-slot", "command-list");
    list
}

fn command_empty_ui(msg: &str) -> Element {
    // solid-ui CommandEmpty: `py-6 text-center text-sm`
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

/// `CommandEmpty`: registers copy; actual paint is inserted by [`CommandList`] when nothing is visible (order-independent of RSX).
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
    let mut stub = Element::new().w(0.0).h(0.0);
    stub.set_attr("data-slot", "command-empty");
    stub.set_attr("rui_command_empty_stub", "1");
    stub
}

/// `CommandGroup`: `p-1` + heading (`text-xs text-muted-foreground`); hidden when filtering leaves no items.
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
    let mut col = vstack().w("100%").gap("0").p("0.25rem").flex_shrink(0.0);
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
        .flex_shrink(0.0)
        .px("0.75rem")
        .py("0.25rem")
        .child(separator())
        .into();
    el = el.pointer_events_none();
    el
}

/// ShadCN / solid-ui `CommandItem`: highlight `bg-accent`; optional check; hide check when a Shortcut is present.
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
            // Do not `Signal::set`: that rebuilds the whole Command tree mid-hover dispatch,
            // freezing highlight, dropping later hover targets, and breaking click (Toast).
            if h && !disabled {
                set_command_active(active, ix);
            }
        })
        .on_click(move || {
            if disabled {
                return;
            }
            set_command_active(active, ix);
            // Same as Dialog Confirm: run on_click first (e.g. enqueue Toast), then close the overlay.
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
    crate::core::tree::walk_elements_mut_pruned(el, |el| {
        if el.attrs.get("data-slot").map(String::as_str) == Some("command-shortcut") {
            el.text_style.size = Some(12.0);
            el.text_style.line_height_px = Some(16.0);
            el.text_style.letter_spacing = 0.4;
            el.text_style.letter_spacing_specified = true;
            el.text_style.color = Some(parse_text_color("muted-foreground"));
            return false;
        }
        if el.text.is_some() {
            let lucide = el.text_style.font_family.as_deref() == Some("lucide");
            if lucide {
                el.text_style.size = Some(14.0);
                el.text_style.line_height_px = Some(14.0);
            } else {
                el.text_style.size = Some(13.0);
                el.text_style.line_height_px = Some(16.0);
                el.text_style.line_height_scale = Some(1.0);
            }
            el.text_style.color = Some(color);
        }
        true
    });
}

/// Each frame writes highlight from `{id}__cmd_active` (`peek`, no root-effect subscription).
pub(crate) fn sync_command_item_paint_subtree(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    crate::core::tree::walk_elements_mut(el, |el| {
        sync_command_palette_focus(el);
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
    });
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
    crate::core::tree::walk_elements_mut_pruned(el, |el| {
        if el.attrs.get("data-slot").map(String::as_str) == Some("command-shortcut") {
            return false;
        }
        if el.text.is_some() && el.text_style.font_family.as_deref() != Some("lucide") {
            el.text_style.color = Some(fg);
        }
        true
    });
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

impl_simple_slot_builder!(
    CommandInputBuilder,
    command_input,
    CommandInput,
    CommandInputProps
);
impl_simple_slot_builder!(
    CommandListBuilder,
    command_list,
    CommandList,
    CommandListProps
);
impl_simple_slot_builder!(
    CommandEmptyBuilder,
    command_empty,
    CommandEmpty,
    CommandEmptyProps
);
impl_simple_slot_builder!(
    CommandSeparatorBuilder,
    command_separator,
    CommandSeparator,
    CommandSeparatorProps
);
impl_simple_slot_builder!(
    CommandShortcutBuilder,
    command_shortcut,
    CommandShortcut,
    CommandShortcutProps
);

pub struct CommandGroupBuilder {
    heading: Option<String>,
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn command_group() -> CommandGroupBuilder {
    CommandGroupBuilder {
        heading: None,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl CommandGroupBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn heading(mut self, h: impl Into<String>) -> Self {
        self.heading = Some(h.into());
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children
            .extend(crate::utils::take_packed(child.into()));
        self
    }
}

impl From<CommandGroupBuilder> for Element {
    fn from(builder: CommandGroupBuilder) -> Self {
        let el = CommandGroup(CommandGroupProps {
            heading: builder.heading,
            children: crate::components::base::packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

impl crate::core::tree::IntoElementList for CommandGroupBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

pub struct CommandItemBuilder {
    value: String,
    disabled: bool,
    checked: bool,
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn command_item(value: impl Into<String>) -> CommandItemBuilder {
    CommandItemBuilder {
        value: value.into(),
        disabled: false,
        checked: false,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl CommandItemBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }
    pub fn checked(mut self, v: bool) -> Self {
        self.checked = v;
        self
    }
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children
            .extend(crate::utils::take_packed(child.into()));
        self
    }
    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.children.push(Element::new().text(content.into()));
        self
    }
}

impl From<CommandItemBuilder> for Element {
    fn from(builder: CommandItemBuilder) -> Self {
        let el = CommandItem(CommandItemProps {
            value: Some(builder.value),
            disabled: Some(builder.disabled),
            checked: Some(builder.checked),
            on_click: Default::default(),
            children: crate::components::base::packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

impl crate::core::tree::IntoElementList for CommandItemBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
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

    #[test]
    fn builder_item_style() {
        let (_o, el): (_, Element) = create_root(|| {
            command_paint(
                "cmd-chrome".into(),
                Signal::new(true),
                "search".into(),
                CommandSurface::Inline,
                Rc::new(|| {
                    command_list()
                        .child(
                            command_item("cal")
                                .style(|b| b.text_size(20.0))
                                .text("Calendar"),
                        )
                        .into()
                }),
            )
        });
        fn find_size(el: &Element, want: f32) -> bool {
            el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
                || (el.text_style.size_resolved() - want).abs() < 0.01
                || el.children.iter().any(|c| find_size(c, want))
        }
        assert!(
            find_size(&el, 20.0),
            "command_item .style(|b| b.text_size(20)) must land on the item tree"
        );
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
        stage.children.first().expect("palette panel")
    }

    fn command_overlay_frame_open(overlay_id: &str, inner: Element) -> Element {
        let open = Signal::new(true);
        let overlay_opts =
            ShellOverlayOpts::from_content_parts(None, Some(0.0), true, Some(0.0));
        viewport_modal_layer(
            overlay_id,
            open,
            &overlay_opts,
            ViewportModalLayerOpts::dialog_fade(),
            |_| Some(command_overlay_panel(inner, overlay_id)),
        )
    }

    #[test]
    fn command_overlay_list_inset_from_panel_edges() {
        clear_overlay_stack();
        let (_owner, mut root) = create_root(|| {
            let inner = command_paint(
                "cmd-pad-reg".into(),
                Signal::new(true),
                "Type a command or search...".into(),
                CommandSurface::Overlay,
                gallery_overlay_body(),
            );
            let frame = command_overlay_frame_open("cmd-pad-reg-t", inner);
            Element::new().w(800.0).h(600.0).child(frame)
        });
        let _ = hoist_portal_subtrees(&mut root);
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
        let stage = find_hoisted_panel(&root, "panel:1001");
        let panel = overlay_palette_panel(stage);
        let [px, _py, pw, _ph] = panel.layout_bounds;
        let item = find_cmd_item(panel);
        let item_inset_l = item.layout_bounds[0] - px;
        let item_inset_r = (px + pw) - (item.layout_bounds[0] + item.layout_bounds[2]);
        clear_overlay_stack();
        assert!(
            item_inset_l >= 3.0,
            "command-item should be inset from panel left by CommandGroup p-1 (>=4px), got {item_inset_l}"
        );
        assert!(
            item_inset_r >= 3.0,
            "command-item should be inset from panel right by CommandGroup p-1 (>=4px), got {item_inset_r}"
        );
    }

    #[test]
    fn command_overlay_frame_hoists_viewport_stage() {
        clear_overlay_stack();
        let (_owner, mut root) = create_root(|| {
            let inner = Element::new().w(200.0).h(48.0).bg([0.2, 0.2, 0.2, 1.0]);
            let frame = command_overlay_frame_open("cmd-overlay-frame-t", inner);
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
        let stage_h = stage.layout_bounds[3];
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
        let panel_y = panel.layout_bounds[1];
        let panel_h = panel.layout_bounds[3];
        let centered_y = (stage_h - panel_h) * 0.5;
        assert!(
            (panel_y - centered_y).abs() < 12.0,
            "palette must be vertically centered in viewport, got y={panel_y} want≈{centered_y}"
        );
        clear_overlay_stack();
    }

    #[test]
    fn command_overlay_empty_sides_hit_underlay() {
        use crate::app::pointer_events::hit_path;

        clear_overlay_stack();
        let (_owner, mut root) = create_root(|| {
            let inner = Element::new().w(200.0).h(48.0).bg([0.2, 0.2, 0.2, 1.0]);
            let frame = command_overlay_frame_open("cmd-overlay-sides-t", inner);
            Element::new().w(800.0).h(600.0).child(frame)
        });

        let _ = hoist_portal_subtrees(&mut root);
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);

        let stage = find_hoisted_panel(&root, "panel:1001");
        let panel = overlay_palette_panel(stage);
        let [px, py, pw, ph] = panel.layout_bounds;
        assert!(
            px > 8.0,
            "palette must not span the viewport; left={px} w={pw}"
        );
        let left_x = (px - 24.0).max(4.0);
        let right_x = (px + pw + 24.0).min(796.0);
        let mid_y = py + ph * 0.5;
        for (name, x) in [("left", left_x), ("right", right_x)] {
            let path = hit_path(&root, x, mid_y);
            assert!(
                path.iter().any(|e| {
                    e.portal_key
                        .as_deref()
                        .is_some_and(|k| k.starts_with("underlay:"))
                }),
                "{name} of command palette must hit underlay so outside click closes; path keys={:?}",
                path.iter()
                    .filter_map(|e| e.portal_key.as_deref())
                    .collect::<Vec<_>>()
            );
        }
        clear_overlay_stack();
    }

    #[test]
    fn command_overlay_close_then_toast_does_not_leave_underlay() {
        use crate::components::toast::host::paint_toast_host;
        use crate::components::toast::toast_push;
        clear_overlay_stack();
        let (_owner, mut root) = create_root(|| {
            let frame = command_overlay_frame_open(
                "cmd-overlay-toast-t",
                Element::new().w(200.0).h(48.0),
            );
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
            let inner = command_paint(
                "gal-cmd".into(),
                Signal::new(true),
                "Type a command or search...".into(),
                CommandSurface::Overlay,
                gallery_overlay_body(),
            );
            let frame = command_overlay_frame_open("cmd-overlay-radius-t", inner);
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
            panel_inst.clip_pos, panel_inst.clip_size, px
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
            let inner: Element = vstack()
                .w("100%")
                .child(Element::new().w("100%").h(36.0).bg([0.2, 0.2, 0.2, 1.0]))
                .child(Element::new().w("100%").h(80.0).bg([0.3, 0.3, 0.3, 1.0]))
                .into();
            let frame = command_overlay_frame_open("cmd-overlay-gallery-t", inner);
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

    #[test]
    fn command_dialog_session_reset_waits_for_unmount() {
        use crate::utils::presence::PresencePhase;
        use crate::utils::viewport_modal::{
            viewport_modal_force_unmount, viewport_modal_is_mounted, viewport_modal_phase,
        };

        clear_overlay_stack();
        let _owner = create_root(|| {
            let id = "cmd-reset-wait";
            let filter_id = format!("{id}__filter");
            let open = use_signal(format!("{id}__open").as_str(), || true);
            let active = use_signal(format!("{id}__cmd_active").as_str(), || 0usize);
            use_signal(input_value_signal_key(&filter_id).as_str(), String::new)
                .set("query".into());
            active.set(2);

            assert_eq!(
                viewport_modal_phase(id, open),
                Some(PresencePhase::Present)
            );
            assert!(viewport_modal_is_mounted(id, open));

            open.set(false);
            assert_eq!(
                viewport_modal_phase(id, open),
                Some(PresencePhase::Exiting)
            );
            assert!(
                viewport_modal_is_mounted(id, open),
                "dialog must stay mounted during exit animation"
            );
            assert_eq!(active.get(), 2, "active must not reset while still mounted");
            assert_eq!(
                use_signal(input_value_signal_key(&filter_id).as_str(), String::new).get(),
                "query",
                "filter must not clear while still mounted"
            );

            viewport_modal_force_unmount(id);
            reset_command_session(id);
            assert_eq!(active.get(), 0);
            assert_eq!(
                use_signal(input_value_signal_key(&filter_id).as_str(), String::new).get(),
                ""
            );
        });
        clear_overlay_stack();
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

    fn three_item_command_body() -> Rc<dyn Fn() -> Element> {
        Rc::new(|| {
            let mk = |value: &str, label: &str| {
                CommandItem(CommandItemProps {
                    value: Some(value.into()),
                    disabled: None,
                    checked: None,
                    on_click: None,
                    children: Some(Element::new().text(label)),
                    base: None,
                })
            };
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
                    children: Some(list_kids.children(vec![
                        mk("a", "Alpha"),
                        mk("b", "Beta"),
                        mk("c", "Gamma"),
                    ])),
                    base: None,
                }),
            ])
        })
    }

    fn collect_cmd_items(root: &Element) -> Vec<&Element> {
        let mut out = Vec::new();
        walk_cmd(root, &mut |el| {
            if el.attrs.get("data-slot").map(String::as_str) == Some("command-item") {
                out.push(el);
            }
        });
        out
    }

    #[test]
    fn command_paint_rebuild_does_not_force_first_item_active() {
        let body = three_item_command_body();
        let (_owner, root) = create_root({
            let body = body.clone();
            move || {
                let open = Signal::new(true);
                let id = "cmd-keep-ix";
                let _ = command_paint(
                    id.into(),
                    open,
                    "q".into(),
                    CommandSurface::Dialog,
                    body.clone(),
                );
                use_signal(format!("{id}__cmd_active").as_str(), || 0usize).set_silent(2);
                command_paint(id.into(), open, "q".into(), CommandSurface::Dialog, body)
            }
        });
        assert_eq!(
            use_signal("cmd-keep-ix__cmd_active", || 0usize).peek(),
            2,
            "close/rebuild must keep the hovered index; resetting to 0 paints the first row accent"
        );
        let items = collect_cmd_items(&root);
        assert_eq!(items.len(), 3);
        let accent = parse_color("accent");
        assert_ne!(
            items[0].style.bg_color, accent,
            "first item must not take hover accent after rebuild"
        );
        assert_eq!(
            items[2].style.bg_color, accent,
            "active index 2 must keep accent through rebuild"
        );
    }

    #[test]
    fn command_dialog_list_scrollbar_hover_is_not_area() {
        use crate::core::scroll_viewport::scroll_hover_mode_signal_key;
        let (_owner, _) = create_root(|| {
            let _ = command_paint(
                "cmd-hover-mode".into(),
                Signal::new(true),
                "q".into(),
                CommandSurface::Dialog,
                three_item_command_body(),
            );
            let mode = use_signal(&scroll_hover_mode_signal_key("cmd-hover-mode:list"), || {
                "missing".to_string()
            })
            .peek();
            assert_eq!(
                mode, "scrollbar",
                "CommandDialog list must not use hover_mode=area (item hover would latch the bar)"
            );
        });
    }

    #[test]
    fn command_dialog_does_not_focus_filter_during_enter() {
        use crate::utils::viewport_modal::{
            viewport_modal_clear_pending_enter, viewport_modal_motion_busy, viewport_modal_phase,
        };
        clear_overlay_stack();
        let (_owner, _) = create_root(|| {
            let id = "cmd-focus-enter";
            let open = Signal::new(true);
            let _ = viewport_modal_phase(id, open);
            assert!(
                viewport_modal_motion_busy(id),
                "opening a dialog must arm enter motion"
            );
            use_signal("__rui_button_focus_id", String::new).set(String::new());
            let mut root = command_paint(
                id.into(),
                open,
                "q".into(),
                CommandSurface::Dialog,
                three_item_command_body(),
            );
            let want = input_focus_id(&format!("{id}__filter"));
            assert_ne!(
                use_signal("__rui_button_focus_id", String::new).peek(),
                want,
                "command_paint must not focus the filter on the enter frame"
            );
            let mut tree = LayoutTree::new();
            let mut impact = crate::core::tree::LayoutStatus::Clean;
            sync_command_item_paint_subtree(&mut root, &mut tree, &mut impact);
            assert_ne!(
                use_signal("__rui_button_focus_id", String::new).peek(),
                want,
                "paint sync must not focus the filter while enter is busy"
            );
            viewport_modal_clear_pending_enter(id);
            sync_command_item_paint_subtree(&mut root, &mut tree, &mut impact);
            assert_eq!(
                use_signal("__rui_button_focus_id", String::new).peek(),
                want,
                "filter focuses only after enter motion is idle"
            );
        });
        clear_overlay_stack();
    }

    #[test]
    fn command_overlay_does_not_focus_filter_during_enter() {
        use crate::utils::viewport_modal::{
            viewport_modal_clear_pending_enter, viewport_modal_motion_busy, viewport_modal_phase,
        };
        clear_overlay_stack();
        let (_owner, _) = create_root(|| {
            let id = "cmd-focus-overlay";
            let open = Signal::new(true);
            let _ = viewport_modal_phase(id, open);
            assert!(
                viewport_modal_motion_busy(id),
                "opening an overlay must arm enter motion"
            );
            use_signal("__rui_button_focus_id", String::new).set(String::new());
            let mut root = command_paint(
                id.into(),
                open,
                "q".into(),
                CommandSurface::Overlay,
                three_item_command_body(),
            );
            let want = input_focus_id(&format!("{id}__filter"));
            assert_ne!(
                use_signal("__rui_button_focus_id", String::new).peek(),
                want,
                "command_paint must not focus the overlay filter on the enter frame"
            );
            let mut tree = LayoutTree::new();
            let mut impact = crate::core::tree::LayoutStatus::Clean;
            sync_command_item_paint_subtree(&mut root, &mut tree, &mut impact);
            assert_ne!(
                use_signal("__rui_button_focus_id", String::new).peek(),
                want,
                "paint sync must not focus the overlay filter while enter is busy"
            );
            viewport_modal_clear_pending_enter(id);
            sync_command_item_paint_subtree(&mut root, &mut tree, &mut impact);
            assert_eq!(
                use_signal("__rui_button_focus_id", String::new).peek(),
                want,
                "overlay filter focuses only after enter motion is idle"
            );
        });
        clear_overlay_stack();
    }

    #[test]
    fn command_dialog_list_scroll_caps_height() {
        let mk_item = |label: &str| {
            CommandItem(CommandItemProps {
                value: Some(label.into()),
                disabled: None,
                checked: None,
                on_click: None,
                children: Some(Element::new().text(label)),
                base: None,
            })
        };
        let (_owner, root) = create_root(|| {
            command_paint(
                "cmd-dlg-scroll".into(),
                Signal::new(true),
                "search".into(),
                CommandSurface::Dialog,
                Rc::new(move || {
                    let mut items = Vec::new();
                    for i in 0..12 {
                        items.push(mk_item(&format!("Item {i}")));
                    }
                    let mut grp_kids = Element::new();
                    grp_kids.set_attr("rui_fragment", "1");
                    let group = CommandGroup(CommandGroupProps {
                        heading: Some("Suggestions".into()),
                        children: Some(grp_kids.children(items)),
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
            )
        });
        let mut host = Element::new().w(800.0).h(600.0).child(root);
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut host, 800.0, 600.0, [-1.0, -1.0]);
        let mut list_h = 0.0f32;
        walk_cmd(&host, &mut |el| {
            if el.attrs.get("data-slot").map(String::as_str) == Some("command-list") {
                list_h = el.layout_bounds[3];
            }
        });
        assert!(
            list_h > 0.0,
            "command-list must layout with non-zero height"
        );
        assert!(
            list_h <= 305.0,
            "command-list must cap at max-h 18.75rem (300px) and scroll, got h={list_h}"
        );
    }

    fn gallery_like_command_body() -> Rc<dyn Fn() -> Element> {
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
            let group = |heading: &str, items: Vec<Element>| {
                let mut grp_kids = Element::new();
                grp_kids.set_attr("rui_fragment", "1");
                CommandGroup(CommandGroupProps {
                    heading: Some(heading.into()),
                    children: Some(grp_kids.children(items)),
                    base: None,
                })
            };
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
                    children: Some(list_kids.children(vec![
                        group(
                            "Suggestions",
                            vec![
                                mk_item("toast", "Show toast"),
                                mk_item("calendar", "Calendar"),
                                mk_item("emoji", "Search Emoji"),
                                mk_item("calc", "Calculator"),
                            ],
                        ),
                        CommandSeparator(CommandSeparatorProps {
                            children: None,
                            base: None,
                        }),
                        group(
                            "Settings",
                            vec![
                                mk_item("profile", "Profile"),
                                mk_item("billing", "Billing"),
                                mk_item("settings", "Settings"),
                            ],
                        ),
                        CommandSeparator(CommandSeparatorProps {
                            children: None,
                            base: None,
                        }),
                        group(
                            "File",
                            vec![
                                mk_item("new-file", "New File"),
                                mk_item("open", "Open..."),
                                mk_item("save", "Save"),
                            ],
                        ),
                    ])),
                    base: None,
                }),
            ])
        })
    }

    /// Gallery CommandDialog body: CommandEmpty registrar + shortcut pills on Settings/File.
    fn gallery_command_dialog_body() -> Rc<dyn Fn() -> Element> {
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
            let mk_shortcut_item = |value: &str, label: &str, keys: &str| {
                let mut kids = Element::new();
                kids.set_attr("rui_fragment", "1");
                CommandItem(CommandItemProps {
                    value: Some(value.into()),
                    disabled: None,
                    checked: None,
                    on_click: None,
                    children: Some(kids.children(vec![
                        Element::new().text(label),
                        CommandShortcut(CommandShortcutProps {
                            children: Some(Element::new().text(keys)),
                            base: None,
                        }),
                    ])),
                    base: None,
                })
            };
            let group = |heading: &str, items: Vec<Element>| {
                let mut grp_kids = Element::new();
                grp_kids.set_attr("rui_fragment", "1");
                CommandGroup(CommandGroupProps {
                    heading: Some(heading.into()),
                    children: Some(grp_kids.children(items)),
                    base: None,
                })
            };
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
                    children: Some(list_kids.children(vec![
                        CommandEmpty(CommandEmptyProps {
                            children: Some(Element::new().text("No results found.")),
                            base: None,
                        }),
                        group(
                            "Suggestions",
                            vec![
                                mk_item("toast", "Show toast"),
                                mk_item("calendar", "Calendar"),
                                mk_item("emoji", "Search Emoji"),
                                mk_item("calc", "Calculator"),
                            ],
                        ),
                        CommandSeparator(CommandSeparatorProps {
                            children: None,
                            base: None,
                        }),
                        group(
                            "Settings",
                            vec![
                                mk_shortcut_item("profile", "Profile", "⌘P"),
                                mk_shortcut_item("billing", "Billing", "⌘B"),
                                mk_shortcut_item("settings", "Settings", "⌘S"),
                            ],
                        ),
                        CommandSeparator(CommandSeparatorProps {
                            children: None,
                            base: None,
                        }),
                        group(
                            "File",
                            vec![
                                mk_shortcut_item("new-file", "New File", "⌘N"),
                                mk_shortcut_item("open", "Open...", "⌘O"),
                                mk_shortcut_item("save", "Save", "⌘S"),
                            ],
                        ),
                    ])),
                    base: None,
                }),
            ])
        })
    }

    fn layout_command_dialog_host(modal_id: &str, body: Rc<dyn Fn() -> Element>) -> Element {
        use crate::components::base::SlotChrome;
        use crate::components::dialog::dialog_shell_frame;
        use crate::utils::modal_shell::{
            default_modal_overlay_color, ShellOverlayOpts,
        };

        let open = Signal::new(true);
        let overlay_opts = ShellOverlayOpts {
            color: default_modal_overlay_color(None, Some(0.5)),
            close_on_click: true,
            backdrop_blur: 0.0,
        };
        let inner = command_paint(
            modal_id.into(),
            open,
            "search".into(),
            CommandSurface::Dialog,
            body,
        );
        let shell = dialog_shell_frame(
            modal_id,
            open,
            inner,
            &overlay_opts,
            None,
            true,
            None,
            None,
            None,
            None,
            SlotChrome::default(),
        );
        Element::new().w("100%").h("100%").child(shell)
    }

    fn item_bounds_by_value(root: &Element, value: &str) -> Option<[f32; 4]> {
        let mut found = None;
        walk_cmd(root, &mut |el| {
            if el.attrs.get("data-slot").map(String::as_str) != Some("command-item") {
                return;
            }
            // CommandItem doesn't stamp `value` on the node; match by first text leaf.
            if crate::core::tree::first_text_leaf(el) == value {
                found = Some(el.layout_bounds);
            }
        });
        found
    }

    #[test]
    fn command_dialog_gallery_settings_items_stable_across_layouts() {
        clear_overlay_stack();
        let modal_id = "cmd-dlg-settings-jump";
        let (_owner, mut host) =
            create_root(|| layout_command_dialog_host(modal_id, gallery_command_dialog_body()));
        assert!(hoist_portal_subtrees(&mut host) >= 2);

        let mut empty_stub_h = None;
        walk_cmd(&host, &mut |el| {
            if el.attrs.get("rui_command_empty_stub").map(String::as_str) == Some("1") {
                empty_stub_h = Some(el.layout_bounds[3]);
            }
        });
        assert!(
            empty_stub_h.is_none(),
            "CommandEmpty registrar must not stay in the list layout (would push groups down)"
        );

        let layout_once = |host: &mut Element| {
            let mut tree = LayoutTree::new();
            let _ = tree.build_and_layout(host, 800.0, 600.0, [-1.0, -1.0]);
            (
                item_bounds_by_value(host, "Profile").expect("Profile"),
                item_bounds_by_value(host, "Billing").expect("Billing"),
                item_bounds_by_value(host, "Settings").expect("Settings item"),
                item_bounds_by_value(host, "New File").expect("New File"),
            )
        };

        let first = layout_once(&mut host);
        let second = layout_once(&mut host);
        for (name, a, b) in [
            ("Profile", first.0, second.0),
            ("Billing", first.1, second.1),
            ("Settings", first.2, second.2),
            ("New File", first.3, second.3),
        ] {
            assert!(
                (a[1] - b[1]).abs() < 0.5,
                "{name} y jumped across layouts: {a:?} -> {b:?}"
            );
        }
        let profile_gap = first.1[1] - (first.0[1] + first.0[3]);
        let billing_to_settings = first.2[1] - (first.1[1] + first.1[3]);
        assert!(
            (profile_gap - billing_to_settings).abs() < 1.0,
            "Settings-group rows must share the same inter-item gap; got profile→billing={profile_gap} billing→settings={billing_to_settings}"
        );
        let settings_to_file = first.3[1] - (first.2[1] + first.2[3]);
        assert!(
            settings_to_file > 0.0,
            "File group must sit below Settings, gap={settings_to_file}"
        );
        clear_overlay_stack();
    }

    /// Enter-settle must not move CommandDialog rows in layout or Coverage Y.
    /// Remaining screen-space travel while fade is already visible is the
    /// `__modal_zoom` layer scaling about the card center (lower rows move down).
    #[test]
    fn command_dialog_item_y_stable_across_enter_settle() {
        use crate::utils::modal_motion::{
            flush_modal_shell_motion_subtree, tick_hoisted_modal_css,
        };
        use crate::utils::viewport_modal::viewport_modal_motion_busy;
        use crate::utils::text_field::input_focus_id;

        crate::style::utility::set_frame_dt(1.0 / 60.0);
        clear_overlay_stack();
        let modal_id = "cmd-dlg-enter-y";
        let (_owner, mut host) =
            create_root(|| layout_command_dialog_host(modal_id, gallery_command_dialog_body()));
        assert!(hoist_portal_subtrees(&mut host) >= 2);

        let item_ys = |host: &Element| {
            (
                item_bounds_by_value(host, "Show toast").expect("Show toast")[1],
                item_bounds_by_value(host, "Profile").expect("Profile")[1],
            )
        };

        let mut last_comp_ys = None;
        let mut last_blit_profile_y = None;
        let mut last_comp_live_also = false;
        let mut settle_ys = None;

        for frame in 0..24 {
            let _ = flush_modal_shell_motion_subtree(&mut host);
            let _ = tick_hoisted_modal_css(&mut host);
            let busy = viewport_modal_motion_busy(modal_id);
            let _ = crate::renderer::take_capture_jobs();
            let mut tree = LayoutTree::new();
            let result = tree.build_and_layout(&mut host, 800.0, 600.0, [-1.0, -1.0]);
            let caps = crate::renderer::take_capture_jobs();
            let cap = caps.iter().find(|c| c.key.starts_with("rui_zoom_cap:"));
            let ys = item_ys(&host);
            if let Some(cap) = cap {
                last_comp_ys = Some(ys);
                last_comp_live_also = result.text_nodes.iter().any(|t| t.text == "Profile");
                if let (Some(ty), Some(blit)) = (
                    cap.text_nodes
                        .iter()
                        .find(|t| t.text == "Profile")
                        .map(|t| t.y),
                    result
                        .textured_draws
                        .iter()
                        .find(|d| d.texture_key.starts_with("rui_zoom_cap:")),
                ) {
                    let b = cap.bounds;
                    last_blit_profile_y = Some(blit.rect[1] + (ty - b[1]) * (blit.rect[3] / b[3]));
                }
            }
            if !busy {
                settle_ys = Some(ys);
                let live_y = result
                    .text_nodes
                    .iter()
                    .find(|t| t.text == "Profile")
                    .map(|t| t.y);
                let last_c = last_comp_ys.expect("enter must capture __modal_zoom");
                assert!(
                    !last_comp_live_also,
                    "1:1 blit must not also emit live Coverage (row pop)"
                );
                assert!(
                    (last_c.0 - ys.0).abs() < 0.5 && (last_c.1 - ys.1).abs() < 0.5,
                    "item layout y jumped compositor→settle: {last_c:?} -> {ys:?}"
                );
                if let (Some(blit_y), Some(live_y)) = (last_blit_profile_y, live_y) {
                    assert!(
                        (blit_y - live_y).abs() < 1.0,
                        "Profile blit-mapped y {blit_y} != Coverage {live_y}"
                    );
                }
                break;
            }
            assert!(frame < 23, "enter must settle within 24 frames");
        }

        let settle = settle_ys.expect("enter must settle");
        let mut tree = LayoutTree::new();
        let mut impact = crate::core::tree::LayoutStatus::Clean;
        sync_command_item_paint_subtree(&mut host, &mut tree, &mut impact);
        let _ = tree.build_and_layout(&mut host, 800.0, 600.0, [-1.0, -1.0]);
        let focused = item_ys(&host);
        assert!(
            (settle.0 - focused.0).abs() < 0.5 && (settle.1 - focused.1).abs() < 0.5,
            "item layout y jumped settle→filter-focus: {settle:?} -> {focused:?}"
        );
        assert_eq!(
            use_signal("__rui_button_focus_id", String::new).peek(),
            input_focus_id(&format!("{modal_id}__filter")),
            "filter focuses only after enter motion is idle"
        );
        clear_overlay_stack();
    }

    #[test]
    fn command_dialog_list_max_scroll_reveals_last_item() {
        use crate::components::base::SlotChrome;
        use crate::components::dialog::dialog_shell_frame;
        use crate::utils::modal_shell::{
            default_modal_overlay_color, ShellOverlayOpts,
        };

        clear_overlay_stack();
        let modal_id = "cmd-dlg-scroll-end";
        let (_owner, mut host) = create_root(|| {
            let open = Signal::new(true);
            let overlay_opts = ShellOverlayOpts {
                color: default_modal_overlay_color(None, Some(0.5)),
                close_on_click: true,
                backdrop_blur: 0.0,
            };
            let inner = command_paint(
                modal_id.into(),
                open,
                "search".into(),
                CommandSurface::Dialog,
                gallery_like_command_body(),
            );
            let shell = dialog_shell_frame(
                modal_id,
                open,
                inner,
                &overlay_opts,
                None,
                true,
                None,
                None,
                None,
                None,
                SlotChrome::default(),
            );
            Element::new().w("100%").h("100%").child(shell)
        });
        assert!(hoist_portal_subtrees(&mut host) >= 2);

        for (vw, vh) in [(800.0, 600.0), (400.0, 400.0)] {
            let mut tree = LayoutTree::new();
            let result = tree.build_and_layout(&mut host, vw, vh, [-1.0, -1.0]);
            let zoom_id = format!("{modal_id}__modal_zoom");
            let mut card_bounds = [0.0f32; 4];
            let mut last_item: Option<[f32; 4]> = None;
            let mut list_bounds = [0.0f32; 4];
            let mut scrollbar_bounds: Option<[f32; 4]> = None;
            let track_fid = format!("scrollbar-track:{modal_id}:list:y");
            walk_cmd(&host, &mut |el| {
                if el.template_id.as_deref() == Some(zoom_id.as_str()) {
                    card_bounds = el.layout_bounds;
                }
                if el.attrs.get("data-slot").map(String::as_str) == Some("command-list") {
                    list_bounds = el.layout_bounds;
                }
                if el.attrs.get("data-slot").map(String::as_str) == Some("command-item") {
                    last_item = Some(el.layout_bounds);
                }
                if el.focus_id.as_deref() == Some(track_fid.as_str()) {
                    scrollbar_bounds = Some(el.layout_bounds);
                }
            });
            let item = last_item.expect("command-item");
            let item_bottom = item[1] + item[3];
            let list_bottom = list_bounds[1] + list_bounds[3];
            let card_bottom = card_bounds[1] + card_bounds[3];
            let header_h = (list_bounds[1] - card_bounds[1]).max(0.0);
            let rest = format!("{modal_id}:list");
            let (_id, vp, max_scroll, content) = result
                .scroll_metrics
                .iter()
                .find(|(id, ..)| id == &rest)
                .unwrap_or_else(|| {
                    panic!(
                        "missing scroll metrics for {rest} at {vw}x{vh}; have {:?}",
                        result
                            .scroll_metrics
                            .iter()
                            .map(|(id, ..)| id.as_str())
                            .collect::<Vec<_>>()
                    )
                });
            let content_bottom = list_bounds[1] + content[1];
            let visible_bottom_at_end = list_bottom + max_scroll[1];
            let pad_after_last = content_bottom - item_bottom;
            let painted_item_bottom = item_bottom - max_scroll[1];
            let max_card = vh * 0.85;
            assert!(
                card_bounds[3] <= max_card + 1.0,
                "card must honor max_h 85dvh at {vw}x{vh}: card_h={} cap={max_card}",
                card_bounds[3]
            );
            assert!(
                list_bounds[3] > 8.0,
                "list must keep a usable viewport (not crushed) at {vw}x{vh}, list_h={}",
                list_bounds[3]
            );
            assert!(
                list_bottom <= card_bottom + 1.0,
                "list must stay inside the card (header+list must fit max_h): list_bottom={list_bottom} card_bottom={card_bottom} header_h={header_h} list_h={} card_h={} {vw}x{vh}",
                list_bounds[3],
                card_bounds[3]
            );
            if let Some(sb) = scrollbar_bounds {
                let sb_bottom = sb[1] + sb[3];
                assert!(
                    sb_bottom <= card_bottom + 1.0,
                    "scrollbar must stay inside the card at {vw}x{vh}: sb_bottom={sb_bottom} card_bottom={card_bottom} sb={sb:?}"
                );
            }
            assert!(
                content[1] > vp[3] + 1.0,
                "list content must overflow viewport so it can scroll at {vw}x{vh} (content_h={} vp_h={})",
                content[1],
                vp[3]
            );
            assert!(
                item_bottom <= content_bottom + 1.0,
                "content_h must cover last item at {vw}x{vh}: item_bottom={item_bottom} content_bottom={content_bottom} content_h={} list_y={} item={:?}",
                content[1],
                list_bounds[1],
                item
            );
            assert!(
                pad_after_last >= 3.0,
                "content_h must include CommandList trailing inset after last item at {vw}x{vh}, got pad_after_last={pad_after_last}"
            );
            assert!(
                item_bottom <= visible_bottom_at_end + 1.0,
                "last command-item must be fully visible at max scroll ({vw}x{vh}): item_bottom={item_bottom} visible_end={visible_bottom_at_end} max_scroll_y={} list_bottom={list_bottom} item_h={}",
                max_scroll[1],
                item[3]
            );
            assert!(
                painted_item_bottom <= card_bottom + 1.0,
                "at max scroll last item must sit inside the card clip ({vw}x{vh}): painted_bottom={painted_item_bottom} card_bottom={card_bottom} list_bottom={list_bottom} max_scroll_y={} item={:?}",
                max_scroll[1],
                item
            );
        }
        clear_overlay_stack();
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
                        item_text = n.text_style.size_resolved();
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
