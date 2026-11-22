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
//! cmdk-like behavior: **type-to-filter**, ↑↓ highlight enabled items with wraparound,
//! Enter run; Esc closes overlay / Dialog. Filtering or disabling an active row
//! clears its highlight when no enabled replacement remains.
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
//!
//! Use [`command_query`] with the root's explicit stable id to read or change its
//! search text, including before mounting. It shares the Input's editing store:
//! `command_query("pal").set("settings")`. Reads subscribe to typed edits and
//! Escape/session resets; dismissible palettes retain their normal reset policy.
//! Builder `.query_signal(query)` binds that committed store to application state,
//! with blur/IME protection for subsequent external writes.
//! Builder `.filterable(false)` preserves externally ranked results, while
//! `.searchable(false)` replaces the search field with list keyboard/typeahead
//! navigation. `.loading_signal(busy)` exposes pending work without showing a
//! misleading empty result. These settings also apply to Builder `.child(...)`.

use crate::text_filter::TextFilter;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::OnceLock;

use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized};
use crate::authoring::widgets::scroll::reset_scroll_item_into_view;
use crate::components::base::SlotChrome;
use crate::components::dialog::{DialogContentOpts, dialog_content, dialog_scope, use_dialog};
use crate::components::input::{
    input, input_caret_signal_key, input_selection_signal_key, input_value_signal_key, lucide_named,
};
use crate::components::label::label;
use crate::components::scroll_area::{scroll_area, scroll_into_view};
use crate::components::separator::separator;
use crate::components::virtual_list::{VirtualListHandle, virtual_list};
use crate::core::reactivity::{
    Context, Signal, batch, create_context, create_memo, provide_context, use_context, use_signal,
    with_child_owner,
};
use crate::core::tree::{CursorKind, Element, Handler, LayoutStatus};
use crate::impl_simple_slot_builder;
use crate::layout::layout_tree::{ATTR_ESCAPE_OVERFLOW_CLIP, LayoutTree};
use crate::primitives::r#box::{hstack, vstack};
use crate::primitives::interactive::{interactive, mark_hover_paint_dirty};
use crate::primitives::text::text;
use crate::style::{TAILWIND_TEXT_SM_LEADING_SCALE, parse_color, parse_text_color};
use crate::tokens;
use crate::utils::activate_registry;
use crate::utils::modal_shell::ShellOverlayOpts;
use crate::utils::popup::{flow_hidden_portal_host, swallow_clicks};
use crate::utils::take_packed;
use crate::utils::text_field::input_focus_id;
use crate::utils::text_node::build_icon_text_node;
use crate::utils::viewport_modal::{
    ViewportModalLayerOpts, viewport_modal_is_mounted, viewport_modal_layer,
    viewport_modal_motion_busy,
};
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

/// Programmatic access to a Command's existing query, without a second store.
#[derive(Clone, Debug)]
pub struct CommandQuery {
    field_id: String,
}

/// Access the query for a Command root with this explicit, nonempty stable id.
///
/// Use within the same window's reactive context as the Command. Clones and
/// repeated calls with the same id address the same Input editing store.
pub fn command_query(id: impl AsRef<str>) -> CommandQuery {
    let id = id.as_ref();
    assert!(
        !id.is_empty(),
        "command_query requires an explicit Command id"
    );
    CommandQuery {
        field_id: format!("{id}__filter"),
    }
}

impl CommandQuery {
    /// Read the current query, subscribing the current reactive observer.
    pub fn get(&self) -> String {
        use_signal(&input_value_signal_key(&self.field_id), String::new).get()
    }

    /// Replace the query through the shared text-field commit path.
    pub fn set(&self, query: impl Into<String>) {
        crate::utils::text_field::commit_value(&self.field_id, query.into());
    }
}

fn command_context() -> Context<CommandCtx> {
    static CTX: OnceLock<Context<CommandCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<CommandCtx>)
}

fn command_filter_context() -> Context<Rc<RefCell<TextFilter>>> {
    thread_local! {
        static CTX: Context<Rc<RefCell<TextFilter>>> = create_context();
    }
    CTX.with(|ctx| *ctx)
}

#[derive(Clone)]
struct CommandBehavior {
    searchable: bool,
    filterable: bool,
    retain_active_by_value: bool,
    loading: Option<Signal<bool>>,
    query: Option<Signal<String>>,
    selection: Option<Signal<Vec<String>>>,
    confirm: Option<Rc<dyn Fn(Vec<String>)>>,
}

impl Default for CommandBehavior {
    fn default() -> Self {
        Self {
            searchable: true,
            filterable: true,
            retain_active_by_value: false,
            loading: None,
            query: None,
            selection: None,
            confirm: None,
        }
    }
}

fn command_behavior_context() -> Context<CommandBehavior> {
    thread_local! {
        static CTX: Context<CommandBehavior> = create_context();
    }
    CTX.with(|ctx| *ctx)
}

fn command_behavior() -> CommandBehavior {
    use_context(command_behavior_context()).unwrap_or_default()
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

fn command_item_matches_query(label: &str, value: &str, keywords: &[String], query: &str) -> bool {
    let mut filter = TextFilter::default();
    filter.begin(query);
    filter.matches(
        [label, value]
            .into_iter()
            .chain(keywords.iter().map(String::as_str)),
    )
}

fn has_shortcut_slot(el: &Element) -> bool {
    crate::core::tree::any_in_tree(el, |n| {
        n.attrs.get("data-slot").map(|s| s.as_str()) == Some("command-shortcut")
    })
}

fn clear_button_focus() {
    use_signal("__jadix_button_focus_id", String::new).set(String::new());
}

fn query_of(ctx: &CommandCtx) -> String {
    use_signal(input_value_signal_key(&ctx.filter_id).as_str(), String::new).peek()
}

fn reset_activates(id: &str) {
    activate_registry::reset(id);
}

fn take_activate(id: &str, ix: usize) -> Option<Rc<dyn Fn()>> {
    activate_registry::take(id, ix)
}

fn is_palette(surface: CommandSurface) -> bool {
    matches!(surface, CommandSurface::Overlay | CommandSurface::Dialog)
}

fn reset_command_session(id: &str) {
    reset_scroll_item_into_view(&format!("{id}:list"));
    command_pointer(id).set_silent(None);
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
    use_signal(format!("{id}__cmd_active").as_str(), || NO_ACTIVE_COMMAND).set(NO_ACTIVE_COMMAND);
}

const NO_ACTIVE_COMMAND: usize = usize::MAX;

fn command_pointer(id: &str) -> Signal<Option<[f32; 2]>> {
    use_signal(&format!("{id}__cmd_pointer"), || None)
}

/// Filter box only consumes list nav / run / dismiss. Backspace, Delete, arrows, Home/End, and chars go to text editing.
fn command_filter_consumes_key(key: &Key) -> bool {
    crate::utils::list_nav::is_list_nav_key(key)
        || matches!(key, Key::Named(NamedKey::PageUp | NamedKey::PageDown))
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
    let behavior = command_behavior();
    let filter_id = format!("{id}__filter");
    let active = use_signal(format!("{id}__cmd_active").as_str(), || NO_ACTIVE_COMMAND);
    // Retain the last real pointer sample through query and layout rebuilds.
    let _ = command_pointer(&id);
    let visible = use_signal(
        format!("{id}__cmd_visible").as_str(),
        Vec::<CommandVisibleItem>::new,
    );
    let empty_msg = use_signal(format!("{id}__cmd_empty").as_str(), || {
        crate::tr_default_zh("jadix.command.empty", "No results found.", "未找到结果。")
    });
    let query_sig = use_signal(input_value_signal_key(&filter_id).as_str(), String::new);
    let filter = use_signal(format!("{id}__cmd_text_filter").as_str(), || {
        Rc::new(RefCell::new(TextFilter::default()))
    })
    .peek();
    // Preserve identity before the Builder replaces the visible snapshot and registry.
    // A removed/disabled result clears the highlight rather than invoking its new neighbour.
    let previous_active = behavior
        .retain_active_by_value
        .then(|| {
            visible.with_untracked(|items| items.get(active.peek()).map(|item| item.value.clone()))
        })
        .flatten();
    let open_now = open.peek();
    let searchable = behavior.searchable;
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
            let query = query_sig.get();
            filter
                .borrow_mut()
                .begin(if behavior.filterable && behavior.searchable {
                    &query
                } else {
                    ""
                });
            provide_context(command_filter_context(), filter.clone());
            visible.set(Vec::new());
            use_signal(
                &format!("{id}__cmd_virtual_rows"),
                || None::<Rc<Vec<usize>>>,
            )
            .set_silent(None);
            reset_activates(&id);
            // Do not reset `active` here. `visible` was just cleared, so the old
            // `if active != 0 && visible.is_empty()` always fired and forced index 0
            // on every rebuild — including Dialog/Overlay close, while exit animation
            // still shows the list (first-item accent flash).
            // CommandList clamps `active` after items are collected.
            // Palette filter focus is applied in `sync_command_item_paint_subtree`
            // after enter motion is idle — not here, which runs at mount (first
            // enter frame) and would show the caret during zoom/fade.
            let children = take_packed(body());
            if let Some(value) = previous_active {
                let ctx = require_command("dynamic Command results");
                let next = visible.with_untracked(|items| {
                    items
                        .iter()
                        .position(|item| item.value == value && !item.disabled)
                });
                reveal_command_active(&ctx, next);
            }
            filter.borrow_mut().finish();
            command_root_shell(surface, children)
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
    if !searchable {
        painted.set_attr("jadix_command_focus", format!("{id}:listbox"));
    }
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
    el.set_attr("jadix_command_palette", "1");
    crate::authoring::presentation::attach(
        el,
        crate::authoring::presentation::Phase::State,
        |element, layout, _context| {
            let mut impact = LayoutStatus::Clean;
            sync_command_item_paint_node(element, layout, &mut impact);
            impact
        },
    );
    el.set_attr("jadix_command_id", id);
    el.set_attr(
        "jadix_command_surface",
        match surface {
            CommandSurface::Dialog => "dialog",
            CommandSurface::Overlay => "overlay",
            CommandSurface::Inline => "inline",
        },
    );
    el.set_attr("jadix_command_filter", filter_id);
    el.set_attr("jadix_command_open", if open { "1" } else { "0" });
}

fn command_palette_motion_busy(id: &str, surface: &str) -> bool {
    match surface {
        "dialog" | "overlay" => viewport_modal_motion_busy(id),
        _ => false,
    }
}

fn sync_command_palette_focus(el: &Element) {
    if el.attrs.get("jadix_command_palette").map(String::as_str) != Some("1") {
        return;
    }
    if el.attrs.get("jadix_command_open").map(String::as_str) != Some("1") {
        return;
    }
    let Some(id) = el.attrs.get("jadix_command_id") else {
        return;
    };
    let Some(filter_id) = el.attrs.get("jadix_command_filter") else {
        return;
    };
    let surface = el
        .attrs
        .get("jadix_command_surface")
        .map(String::as_str)
        .unwrap_or("inline");
    if command_palette_motion_busy(id, surface) {
        return;
    }
    let want = el
        .attrs
        .get("jadix_command_focus")
        .cloned()
        .unwrap_or_else(|| input_focus_id(filter_id));
    let focus = use_signal("__jadix_button_focus_id", String::new);
    if focus.peek() != want {
        focus.set(want);
    }
}

fn command_placeholder(placeholder: impl Into<String>) -> String {
    let placeholder = placeholder.into();
    if placeholder.is_empty() {
        crate::tr_default_zh(
            "jadix.command.placeholder",
            "Type a command or search...",
            "输入命令或搜索…",
        )
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
    // A 0×0 out-of-flow host still hoists underlay/stage (same as Select / Combobox) without occupying `gap`.
    flow_hidden_portal_host(
        Element::new()
            .pointer_events_box_none()
            .child(host)
            .child(reactive_placeholder_sized(reset_view, 0.0, 0.0).pointer_events_box_none()),
    )
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
    // Host is out of flow so a wrap HStack `gap` does not reserve an extra slot.
    flow_hidden_portal_host(
        Element::new()
            .pointer_events_box_none()
            .child(reactive_placeholder_sized(view_id, 0.0, 0.0).pointer_events_box_none()),
    )
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
    panel.set_attr("data-slot", "command-overlay-content");
    panel = crate::style::apply_stylesheet_rules(panel);
    panel.set_attr(ATTR_ESCAPE_OVERFLOW_CLIP, "1");
    panel.template_id = Some(format!("{overlay_id}__modal_zoom"));
    // Stage is already portaled; do not wrap `panel_block_clicks` (nested `jadix_portal` would hoist the panel out of the centered stage).
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
    pub keywords: Vec<String>,
    pub shortcut: Option<String>,
    pub disabled: bool,
    pub run: Rc<dyn Fn()>,
}

// Callback identity participates so replacing only a callback refreshes the registry.
impl PartialEq for CommandAction {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.label == other.label
            && self.keywords == other.keywords
            && self.shortcut == other.shortcut
            && self.disabled == other.disabled
            && Rc::ptr_eq(&self.run, &other.run)
    }
}
impl Eq for CommandAction {}

impl CommandAction {
    pub fn new(id: impl Into<String>, label: impl Into<String>, run: impl Fn() + 'static) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            keywords: Vec::new(),
            shortcut: None,
            disabled: false,
            run: Rc::new(run),
        }
    }

    pub fn shortcut(mut self, s: impl Into<String>) -> Self {
        self.shortcut = Some(s.into());
        self
    }

    /// Additional terms that should match this command in the search field.
    pub fn keywords<I, S>(mut self, keywords: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.keywords.extend(keywords.into_iter().map(Into::into));
        self
    }

    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
}

/// Builder group.
#[derive(Clone, PartialEq, Eq)]
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

#[derive(Clone, PartialEq, Eq)]
enum CommandEntry {
    Item(CommandAction),
    Group(CommandGroupData),
    Separator,
}

pub struct CommandBuilder {
    id: String,
    open: Option<Signal<bool>>,
    entries: Vec<CommandEntry>,
    actions: Option<Signal<Vec<CommandAction>>>,
    placeholder: String,
    surface: CommandSurface,
    behavior: CommandBehavior,
}

/// Command palette entry. With [`CommandBuilder::open_signal`], defaults to top-centered overlay; [`.dialog()`](CommandBuilder::dialog) uses Dialog.
pub fn command(id: impl Into<String>) -> CommandBuilder {
    CommandBuilder {
        id: id.into(),
        open: None,
        entries: Vec::new(),
        actions: None,
        placeholder: crate::tr_default_zh(
            "jadix.command.placeholder",
            "Type a command or search...",
            "输入命令或搜索…",
        ),
        surface: CommandSurface::Overlay,
        behavior: CommandBehavior::default(),
    }
}

impl CommandBuilder {
    /// Bind committed search text to application state, including typed edits and
    /// Escape/session resets. External writes wait while the field is focused or
    /// composing; local edits win. Also applies to `child` and searchless lists.
    pub fn query_signal(mut self, query: Signal<String>) -> Self {
        self.behavior.query = Some(query);
        self
    }

    /// Controlled multi-selection, keyed by stable, unique action IDs (or compound
    /// `CommandItem::value`). Selection survives filtering and virtual row unmounts.
    /// Clicking or activating toggles membership before running the item callback;
    /// the palette stays open. Without `on_confirm`, Enter also toggles the item.
    pub fn selection_signal(mut self, selection: Signal<Vec<String>>) -> Self {
        self.behavior.selection = Some(selection);
        self
    }

    /// With `selection_signal`, Enter confirms the current values, including an
    /// empty selection, before dismissing a dialog/overlay. Searchless lists also
    /// support Space to toggle the active item without confirming.
    pub fn on_confirm(mut self, confirm: impl Fn(Vec<String>) + 'static) -> Self {
        self.behavior.confirm = Some(Rc::new(confirm));
        self
    }

    /// Keep the supplied result order and membership (for external/async search).
    pub fn filterable(mut self, filterable: bool) -> Self {
        self.behavior.filterable = filterable;
        self
    }

    /// Without a search field, focus the list and use typeahead/keyboard navigation.
    pub fn searchable(mut self, searchable: bool) -> Self {
        self.behavior.searchable = searchable;
        self
    }

    /// Pending work is owned by the application; existing rows remain usable.
    pub fn loading_signal(mut self, loading: Signal<bool>) -> Self {
        self.behavior.loading = Some(loading);
        self
    }

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
    /// Controlled flat results, replacing static actions/groups. Pair with
    /// `filterable(false)` for externally ranked async search and `loading_signal`
    /// for pending work. Keep IDs unique and stable: reordering preserves the
    /// highlighted ID; removal or disabling clears it. Query and selection survive.
    /// Replacing a callback (even with identical metadata) updates activation.
    /// This applies to the Builder data path; `child` supplies its own results.
    pub fn actions_signal(mut self, actions: Signal<Vec<CommandAction>>) -> Self {
        self.actions = Some(actions);
        self.behavior.retain_active_by_value = true;
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
        assert!(
            self.behavior.confirm.is_none() || self.behavior.selection.is_some(),
            "Command::on_confirm requires selection_signal"
        );
        with_child_owner(|| {
            let id = crate::utils::resolve_instance_id(self.id, "cmd");
            if let Some(query) = self.behavior.query {
                crate::utils::text_field::bind_text_query_signal(&format!("{id}__filter"), query);
            }
            provide_context(command_behavior_context(), self.behavior);
            match (self.open, self.surface) {
                (Some(open), CommandSurface::Dialog) => {
                    command_dialog_scope(id, Some(open), self.placeholder, false, body)
                }
                (Some(open), CommandSurface::Overlay) => {
                    command_overlay_scope(id, Some(open), self.placeholder, body)
                }
                _ => command_scope(
                    id,
                    self.open,
                    self.placeholder,
                    CommandSurface::Inline,
                    body,
                ),
            }
        })
    }
}

// Rows reference immutable Builder data; filtering never clones every action or builds
// discarded Element trees. Item indices exclude headings and separators.
#[derive(Clone, Copy)]
enum BuilderRow {
    Item {
        entry: usize,
        child: Option<usize>,
        index: usize,
    },
    Heading(usize),
    Separator,
}

fn builder_action(entries: &[CommandEntry], entry: usize, child: Option<usize>) -> &CommandAction {
    match &entries[entry] {
        CommandEntry::Item(action) => action,
        CommandEntry::Group(group) => &group.items[child.unwrap()],
        CommandEntry::Separator => unreachable!(),
    }
}

fn builder_palette_body(placeholder: String, entries: Rc<Vec<CommandEntry>>) -> Element {
    let ctx = require_command("command Builder");
    let _ = CommandEmpty(CommandEmptyProps::default());
    let filter = use_context(command_filter_context()).expect("Command filter pass");
    let mut rows = Vec::new();
    let mut item_rows = Vec::new();
    let mut visible = Vec::new();
    let mut pending_separator = false;
    for (entry, value) in entries.iter().enumerate() {
        let actions = match value {
            CommandEntry::Item(action) => std::slice::from_ref(action),
            CommandEntry::Group(group) => group.items.as_slice(),
            CommandEntry::Separator => {
                pending_separator = !rows.is_empty();
                continue;
            }
        };
        let mut first = true;
        for (child, action) in actions.iter().enumerate() {
            let value = if action.id.is_empty() {
                &action.label
            } else {
                &action.id
            };
            if !filter.borrow_mut().matches(
                [action.label.as_str(), value.as_str()]
                    .into_iter()
                    .chain(action.keywords.iter().map(String::as_str)),
            ) {
                continue;
            }
            if first {
                if pending_separator {
                    rows.push(BuilderRow::Separator);
                }
                pending_separator = false;
                if let CommandEntry::Group(group) = &entries[entry] {
                    if !group.heading.is_empty() {
                        rows.push(BuilderRow::Heading(entry));
                    }
                }
                first = false;
            }
            let index = visible.len();
            item_rows.push(rows.len());
            rows.push(BuilderRow::Item {
                entry,
                child: matches!(entries[entry], CommandEntry::Group(_)).then_some(child),
                index,
            });
            visible.push(CommandVisibleItem {
                value: value.clone(),
                label: action.label.clone(),
                disabled: action.disabled,
            });
            activate_registry::push_enabled(
                &ctx.id,
                command_item_activation(value.clone(), action.run.clone()),
                !action.disabled,
            );
        }
    }
    ctx.visible.set_silent(visible);
    clamp_command_active(&ctx);
    let rows = Rc::new(rows);
    let count = rows.len();
    let item_rows = Rc::new(item_rows);
    // Page navigation needs logical rows, including offscreen items. Ordinary
    // compound lists continue using their actual measured item bounds.
    use_signal(&format!("{}__cmd_virtual_rows", ctx.id), || {
        None::<Rc<Vec<usize>>>
    })
    .set_silent((count >= 64).then(|| item_rows.clone()));
    let geometry = (count >= 64).then(VirtualListHandle::default);
    use_signal(&format!("{}__cmd_virtual_geometry", ctx.id), || {
        None::<VirtualListHandle>
    })
    .set_silent(geometry.clone());
    let render = {
        let rows = rows.clone();
        let ctx = ctx.clone();
        move |row: usize| {
            with_child_owner(|| {
                provide_context(command_context(), ctx.clone());
                match rows[row] {
                    BuilderRow::Item {
                        entry,
                        child,
                        index,
                    } => {
                        let item = builder_item(
                            builder_action(&entries, entry, child),
                            ctx.clone(),
                            index,
                        );
                        if child.is_some() {
                            vstack().w("100%").px("0.25rem").child(item).into()
                        } else {
                            item
                        }
                    }
                    BuilderRow::Heading(entry) => {
                        let CommandEntry::Group(group) = &entries[entry] else {
                            unreachable!()
                        };
                        let head: Element = label(format!("{}:grp:{}", ctx.id, group.heading))
                            .text(group.heading.clone())
                            .size(12.0)
                            .line_height(12.0)
                            .font_weight(500.0)
                            .color(parse_text_color("muted-foreground"))
                            .into();
                        head.w("100%").px("0.75rem").py("0.375rem")
                    }
                    BuilderRow::Separator => CommandSeparator(CommandSeparatorProps::default()),
                }
            })
        }
    };
    let list = if count >= 64 {
        // A highlight is persistent, but revealing it is a keyboard intent.
        // Consume that intent so wheel/drag scrolling can leave it offscreen.
        let reveal = use_signal(&format!("{}__cmd_virtual_focus", ctx.id), || {
            NO_ACTIVE_COMMAND
        });
        reveal.set_silent(ctx.active.peek());
        let mut list: Element = virtual_list(format!("{}:list", ctx.id))
            .handle(geometry.unwrap())
            .item_count(count)
            .item_height(32.0)
            .measure_rows()
            .height(300.0)
            .focused_index_fn(move || {
                let index = reveal.peek();
                reveal.set_silent(NO_ACTIVE_COMMAND);
                item_rows.get(index).copied()
            })
            .render_item(render)
            .into();
        list = list.max_h("18.75rem").min_h(0.0).flex_shrink(1.0);
        list.set_attr("data-slot", "command-list");
        list.set_attr("id", format!("{}:listbox", ctx.id));
        list = command_list_interaction(list.accesskit_role("listbox"), &ctx);
        reset_scroll_item_into_view(&format!("{}:list", ctx.id));
        list
    } else {
        let mut fragment = Element::new();
        fragment.set_attr("jadix_fragment", "1");
        CommandList(CommandListProps {
            children: Some(fragment.children((0..count).map(render).collect::<Vec<_>>())),
            base: None,
        })
    };
    let mut root = Element::new();
    root.set_attr("jadix_fragment", "1");
    root.children(vec![
        CommandInput(CommandInputProps {
            placeholder: Some(placeholder),
            ..Default::default()
        }),
        list,
    ])
}

fn builder_item(action: &CommandAction, ctx: CommandCtx, index: usize) -> Element {
    let mut kids = vec![Element::new().text(action.label.clone())];
    if let Some(shortcut) = &action.shortcut {
        kids.push(CommandShortcut(CommandShortcutProps {
            children: Some(Element::new().text(shortcut.clone())),
            base: None,
        }));
    }
    let value = if action.id.is_empty() {
        &action.label
    } else {
        &action.id
    };
    command_item_row(
        ctx,
        index,
        action.disabled,
        false,
        command_item_activation(value.clone(), action.run.clone()),
        kids,
    )
}

fn command_item_activation(value: String, action: Rc<dyn Fn()>) -> Rc<dyn Fn()> {
    let Some(selection) = command_behavior().selection else {
        return action;
    };
    Rc::new(move || {
        selection.set(super::toggle_group::multi_select_after_click(
            &selection.peek(),
            &value,
        ));
        mark_hover_paint_dirty();
        action();
    })
}

impl From<CommandBuilder> for Element {
    fn from(mut builder: CommandBuilder) -> Self {
        let entries = Rc::new(std::mem::take(&mut builder.entries));
        // Build immutable result data once per source change, not per query/key press.
        let dynamic = builder.actions.map(|actions| {
            create_memo(move || {
                Rc::new(actions.get().into_iter().map(CommandEntry::Item).collect())
            })
        });
        let placeholder = builder.placeholder.clone();
        builder.child(move || {
            builder_palette_body(
                placeholder.clone(),
                dynamic.map_or_else(|| entries.clone(), |items| items.get()),
            )
        })
    }
}

impl crate::core::tree::IntoElementList for CommandBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

// ---------------------------------------------------------------------------
// Compound parts
// ---------------------------------------------------------------------------

crate::__component_contract! {
CommandProps [];
/// RSX `<Command>`: with a composite subtree uses [`command_scope`]; otherwise the Builder path.
#[allow(non_snake_case)]
pub fn Command(
identity [] id: String,
plain [] open: Option<Signal<bool>>,
plain [] placeholder: Option<String>,
plain [] actions: Option<Vec<CommandAction>>,
plain [] children: Option<Element>,
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
}

fn reveal_command_active(ctx: &CommandCtx, next: Option<usize>) {
    let index = next.unwrap_or(NO_ACTIVE_COMMAND);
    set_command_active(ctx.active, index);
    use_signal(&format!("{}__cmd_virtual_focus", ctx.id), || {
        NO_ACTIVE_COMMAND
    })
    .set_silent(index);
    if let Some(index) = next {
        scroll_into_view(
            format!("{}:list", ctx.id),
            format!("{}:item:{index}", ctx.id),
            "nearest",
        );
    }
}

fn command_key_handler(
    ctx: CommandCtx,
    prev_key: Option<Rc<RefCell<dyn FnMut(winit::event::KeyEvent) -> bool>>>,
    searchable: bool,
) -> impl FnMut(winit::event::KeyEvent) -> bool {
    let cmd_id = ctx.id.clone();
    let open = ctx.open;
    let active = ctx.active;
    let dismissible = ctx.surface.dismissible();
    let behavior = command_behavior();
    let query = use_signal(&input_value_signal_key(&ctx.filter_id), String::new);
    let mut prefix = String::new();
    let mut last_typed = None::<std::time::Instant>;
    move |ev| {
        if ev.state != winit::event::ElementState::Pressed
            && (!searchable || matches!(ev.logical_key, Key::Named(NamedKey::Escape)))
        {
            return false;
        }
        if !searchable {
            if behavior.selection.is_some()
                && matches!(&ev.logical_key, Key::Character(text) if text.as_str() == " ")
            {
                if let Some(action) = take_activate(&cmd_id, active.peek()) {
                    batch(|| action());
                }
                prefix.clear();
                return true;
            }
            if matches!(ev.logical_key, Key::Named(NamedKey::Home | NamedKey::End)) {
                prefix.clear();
                reveal_command_active(
                    &ctx,
                    activate_registry::enabled_edge(
                        &cmd_id,
                        matches!(ev.logical_key, Key::Named(NamedKey::End)),
                    ),
                );
                return true;
            }
            if let Key::Character(_) = &ev.logical_key {
                if let Some(typed) = ev
                    .text_with_all_modifiers
                    .as_ref()
                    .or(ev.text.as_ref())
                    .filter(|text| !text.is_empty() && !text.chars().any(char::is_control))
                {
                    let now = std::time::Instant::now();
                    if last_typed.is_none_or(|last| now.duration_since(last).as_millis() > 600) {
                        prefix.clear();
                    }
                    let typed = typed.to_lowercase();
                    // Repeated single letters cycle matches instead of searching for "aaa".
                    if prefix != typed {
                        prefix.push_str(&typed);
                    }
                    last_typed = Some(now);
                    let next = ctx.visible.with_untracked(|items| {
                        let count = items.len();
                        let current = active.peek();
                        let start = usize::from(prefix.chars().count() == 1 && current < count);
                        let current = if current < count { current } else { 0 };
                        (start..count + start)
                            .map(|step| (current + step) % count)
                            .find(|&index| {
                                !items[index].disabled
                                    && items[index].label.to_lowercase().starts_with(&prefix)
                            })
                    });
                    if next.is_some() {
                        reveal_command_active(&ctx, next);
                    }
                    return true;
                }
            }
        }
        if !command_filter_consumes_key(&ev.logical_key) {
            if let Some(prev) = &prev_key {
                return prev.borrow_mut()(ev);
            }
            return false;
        }
        match &ev.logical_key {
            Key::Named(NamedKey::Escape) if searchable && !query.peek().is_empty() => {
                batch(|| reset_command_session(&cmd_id));
                return true;
            }
            Key::Named(NamedKey::Escape) if dismissible => {
                reset_scroll_item_into_view(&format!("{cmd_id}:list"));
                open.set(false);
                clear_button_focus();
                return true;
            }
            Key::Named(
                NamedKey::ArrowDown | NamedKey::ArrowUp | NamedKey::PageDown | NamedKey::PageUp,
            ) => {
                let down = matches!(
                    ev.logical_key,
                    Key::Named(NamedKey::ArrowDown | NamedKey::PageDown)
                );
                let next = if matches!(
                    ev.logical_key,
                    Key::Named(NamedKey::PageDown | NamedKey::PageUp)
                ) {
                    let height_key = crate::core::scroll_viewport::ScrollViewportSignalKeys::new(
                        &format!("{cmd_id}:list"),
                    )
                    .viewport_h;
                    let height = use_signal(&height_key, || 0.0_f32).peek();
                    command_page_enabled(&cmd_id, active.peek(), height, down)
                } else {
                    activate_registry::move_enabled(
                        &cmd_id,
                        active.peek(),
                        if down { 1 } else { -1 },
                    )
                };
                prefix.clear();
                reveal_command_active(&ctx, next);
                return true;
            }
            Key::Named(NamedKey::Enter) => {
                if let (Some(selection), Some(confirm)) = (behavior.selection, &behavior.confirm) {
                    batch(|| {
                        confirm(selection.peek());
                        if dismissible {
                            reset_scroll_item_into_view(&format!("{cmd_id}:list"));
                            open.set(false);
                            clear_button_focus();
                        }
                    });
                    return true;
                }
                let ix = active.peek();
                if ix < activate_registry::count(&cmd_id) {
                    if let Some(act) = take_activate(&cmd_id, ix) {
                        batch(|| {
                            act();
                            if dismissible && behavior.selection.is_none() {
                                reset_scroll_item_into_view(&format!("{cmd_id}:list"));
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
    }
}

crate::__component_contract! {
CommandInputProps [];
/// ShadCN / solid-ui `CommandInput`: bottom border + search icon + transparent input.
#[allow(non_snake_case)]
pub fn CommandInput(
plain [] placeholder: Option<String>,
plain [] children: Option<Element>,
) -> Element {
    let _ = children;
    let ctx = require_command("CommandInput");
    if !command_behavior().searchable {
        return Element::new().w(0.0).h(0.0);
    }
    let ph = placeholder
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| ctx.placeholder.clone());
    let filter_id = ctx.filter_id.clone();

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
    field = field.accesskit_role("combobox").accesskit_name(ph.as_str());
    field.set_attr("aria-haspopup", "listbox");
    field.set_attr("aria-autocomplete", "list");
    sync_command_input_semantics(&mut field, &ctx);
    let semantics = ctx.clone();
    crate::authoring::presentation::attach(
        &mut field,
        crate::authoring::presentation::Phase::State,
        move |element, _, _| sync_command_input_semantics(element, &semantics),
    );
    field.style.shadow_blur = 0.0;
    field.style.shadow_spread = 0.0;
    field.style.shadow_color = [0.0, 0.0, 0.0, 0.0];
    if let Some(sync) = field.input_paint_sync.as_mut() {
        sync.plain_chrome = true;
    }

    let prev_key = field.on_key_down.clone();
    field = field.on_key_down(command_key_handler(ctx.clone(), prev_key, true));

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
}

crate::__component_contract! {
CommandListProps [];
/// ShadCN / solid-ui `CommandList`：`max-h-[300px] overflow-y-auto overflow-x-hidden`。
#[allow(non_snake_case)]
pub fn CommandList(
plain [] children: Option<Element>,
) -> Element {
    let ctx = require_command("CommandList");
    let kids = children.map(take_packed).unwrap_or_default();
    // CommandEmpty is a registrar (copy for the no-results UI). It must not occupy
    // a flex slot in the list — a 0×0 stub still participates in column layout
    // once text/min-size resolve, which shoves groups below it downward.
    let kids: Vec<Element> = kids
        .into_iter()
        .filter(|k| k.attrs.get("jadix_command_empty_stub").map(String::as_str) != Some("1"))
        .collect();
    let mut col = vstack()
        .w("100%")
        .gap("0")
        .flex_grow(0.0)
        .flex_shrink(0.0)
        .children(kids);
    if ctx.visible.with_untracked(Vec::is_empty) {
        let loading = command_behavior().loading.is_some_and(|loading| loading.get());
        let message = if loading {
            crate::tr_default_zh("jadix.command.loading", "Loading…", "加载中…")
        } else {
            ctx.empty_msg.peek()
        };
        let mut empty = command_empty_ui(&message);
        if loading {
            empty = empty.accesskit_role("status").accesskit_name(&message);
            empty.set_attr("data-slot", "command-loading");
        }
        col = col.child(empty);
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
    clamp_command_active(&ctx);
    let scroll_id = format!("{}:list", ctx.id);
    // A rebuilt/filtered list can reuse an index for a different item. Keyboard
    // highlight updates only repaint, so their requests survive until layout.
    reset_scroll_item_into_view(&scroll_id);
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
    list.set_attr("id", format!("{}:listbox", ctx.id));
    command_list_interaction(list.accesskit_role("listbox"), &ctx)
}
}

fn command_list_interaction(mut list: Element, ctx: &CommandCtx) -> Element {
    let behavior = command_behavior();
    if behavior.selection.is_some() {
        list.set_attr("aria-multiselectable", "true");
    }
    let busy = behavior.loading.is_some_and(|loading| loading.get());
    list.set_attr("aria-busy", busy.to_string());
    list.set_attr(
        "accesskit_size_of_set",
        ctx.visible.with_untracked(Vec::len).to_string(),
    );
    if !behavior.searchable {
        list = list.focus_id(format!("{}:listbox", ctx.id)).focusable(true);
        list = list.accesskit_name(&ctx.placeholder);
        let previous = list.on_key_down.clone();
        list = list.on_key_down(command_key_handler(ctx.clone(), previous, false));
        sync_command_list_semantics(&mut list, ctx);
    }
    let semantics = ctx.clone();
    crate::authoring::presentation::attach(
        &mut list,
        crate::authoring::presentation::Phase::State,
        move |element, _, _| {
            let busy = behavior
                .loading
                .is_some_and(|loading| loading.peek())
                .to_string();
            let mut impact = LayoutStatus::Clean;
            if element.attrs.get("aria-busy") != Some(&busy) {
                element.set_attr("aria-busy", busy);
                impact = LayoutStatus::StyleChanged;
            }
            let size = semantics.visible.with_untracked(Vec::len).to_string();
            if element.attrs.get("accesskit_size_of_set") != Some(&size) {
                element.set_attr("accesskit_size_of_set", size);
                impact = LayoutStatus::StyleChanged;
            }
            if !behavior.searchable {
                impact = impact.merge(sync_command_list_semantics(element, &semantics));
            }
            impact
        },
    );
    if let Some(selection) = behavior.selection {
        // One retained binding schedules State paint for external writes without
        // subscribing the filtering/body effect or creating per-option effects.
        list = crate::app::reactive::bind_prop(
            list,
            format!("{}:selection-paint", ctx.id),
            move || selection.with(|_| ()),
            |_, ()| LayoutStatus::StyleChanged,
        );
    }
    list
}

fn command_active_option(ctx: &CommandCtx) -> Option<String> {
    let active = ctx.active.peek();
    ctx.open
        .peek()
        .then(|| {
            ctx.visible.with_untracked(|items| {
                items
                    .get(active)
                    .filter(|item| !item.disabled)
                    .map(|_| format!("{}:item:{active}", ctx.id))
            })
        })
        .flatten()
}

fn sync_command_list_semantics(list: &mut Element, ctx: &CommandCtx) -> LayoutStatus {
    let option = command_active_option(ctx);
    if list.attrs.get("aria-activedescendant") == option.as_ref() {
        return LayoutStatus::Clean;
    }
    if let Some(option) = option {
        list.set_attr("aria-activedescendant", option);
    } else {
        list.attrs.remove("aria-activedescendant");
    }
    LayoutStatus::StyleChanged
}

// Keyboard highlight uses silent Signals and a paint pass; accessibility must
// follow that same state without subscribing or rebuilding the input subtree.
fn sync_command_input_semantics(field: &mut Element, ctx: &CommandCtx) -> LayoutStatus {
    let expanded = ctx.open.peek();
    let option = command_active_option(ctx);
    let mut impact = LayoutStatus::Clean;
    for (key, value) in [
        ("aria-expanded", Some(expanded.to_string())),
        (
            "aria-controls",
            expanded.then(|| format!("{}:listbox", ctx.id)),
        ),
        ("aria-activedescendant", option),
    ] {
        if field.attrs.get(key) != value.as_ref() {
            if let Some(value) = value {
                field.set_attr(key, value);
            } else {
                field.attrs.remove(key);
            }
            impact = LayoutStatus::StyleChanged;
        }
    }
    impact
}

fn command_page_enabled(id: &str, current: usize, viewport: f32, down: bool) -> Option<usize> {
    let rows = use_signal(
        &format!("{id}__cmd_virtual_rows"),
        || None::<Rc<Vec<usize>>>,
    )
    .peek();
    let Some(rows) = rows else {
        return activate_registry::page_enabled(id, current, viewport, down);
    };
    let Some(&row) = rows.get(current) else {
        return activate_registry::enabled_edge(id, !down);
    };
    let geometry = use_signal(&format!("{id}__cmd_virtual_geometry"), || {
        None::<VirtualListHandle>
    })
    .peek();
    // Unmeasured rows retain VirtualList's estimate; measured rows and group
    // headings use the same geometry as scrolling and focus reveal.
    let position = |row| {
        geometry
            .as_ref()
            .and_then(|geometry| geometry.row_offset(row))
            .unwrap_or(row as f64 * 32.0)
    };
    let distance = if viewport.is_finite() {
        f64::from(viewport.max(1.0))
    } else {
        32.0
    };
    let target = if down {
        position(row) + distance
    } else {
        position(row) - distance
    };
    let mut best = None;
    let mut best_distance = f64::INFINITY;
    for step in 1..rows.len() {
        let index = if down {
            current
                .checked_add(step)
                .filter(|&index| index < rows.len())
        } else {
            current.checked_sub(step)
        };
        let Some(index) = index else { break };
        if activate_registry::take(id, index).is_none() {
            continue;
        }
        let offset = position(rows[index]);
        let distance = (offset - target).abs();
        if distance < best_distance {
            best = Some(index);
            best_distance = distance;
        }
        if (down && offset >= target) || (!down && offset <= target) {
            break;
        }
    }
    best.or_else(|| activate_registry::take(id, current).map(|_| current))
}

fn clamp_command_active(ctx: &CommandCtx) {
    let active = ctx.active.peek();
    if active != NO_ACTIVE_COMMAND {
        let next = ctx
            .visible
            .with_untracked(|visible| match visible.get(active) {
                Some(item) if !item.disabled => Some(active),
                Some(_) => None,
                None => visible.iter().rposition(|item| !item.disabled),
            });
        set_command_active(ctx.active, next.unwrap_or(NO_ACTIVE_COMMAND));
    }
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

crate::__component_contract! {
CommandEmptyProps [];
/// `CommandEmpty`: registers copy; actual paint is inserted by [`CommandList`] when nothing is visible (order-independent of RSX).
#[allow(non_snake_case)]
pub fn CommandEmpty(
plain [] children: Option<Element>,
) -> Element {
    let ctx = require_command("CommandEmpty");
    let msg = match &children {
        Some(c) => c
            .text
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| extract_text_leaf(c)),
        None => crate::tr_default_zh("jadix.command.empty", "No results found.", "未找到结果。"),
    };
    if ctx.empty_msg.peek() != msg {
        ctx.empty_msg.set(msg);
    }
    let mut stub = Element::new().w(0.0).h(0.0);
    stub.set_attr("data-slot", "command-empty");
    stub.set_attr("jadix_command_empty_stub", "1");
    stub
}
}

crate::__component_contract! {
CommandGroupProps [];
/// `CommandGroup`: `p-1` + heading (`text-xs text-muted-foreground`); hidden when filtering leaves no items.
#[allow(non_snake_case)]
pub fn CommandGroup(
plain [] heading: Option<String>,
plain [] children: Option<Element>,
) -> Element {
    let ctx = require_command("CommandGroup");
    let kids = children.map(take_packed).unwrap_or_default();
    let kids: Vec<Element> = kids
        .into_iter()
        .filter(|k| k.attrs.get("jadix_command_hidden").map(|s| s.as_str()) != Some("1"))
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
}

crate::__component_contract! {
CommandSeparatorProps [];
/// ShadCN / solid-ui `CommandSeparator`：`h-px bg-border`。
#[allow(non_snake_case)]
pub fn CommandSeparator(
plain [] children: Option<Element>,
) -> Element {
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
}

crate::__component_contract! {
CommandItemProps [];
/// ShadCN / solid-ui `CommandItem`: highlight `bg-accent`; optional check; hide check when a Shortcut is present.
#[allow(non_snake_case)]
pub fn CommandItem(
default [] value: String,
default [] disabled: bool,
default [] checked: bool,
plain [] keywords: Option<Vec<String>>,
default [mut] on_click: Handler,
plain [] children: Option<Element>,
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
    let keywords = keywords.unwrap_or_default();
    let behavior = command_behavior();
    let matches = if !behavior.searchable || !behavior.filterable {
        true
    } else if let Some(filter) = use_context(command_filter_context()).filter(|filter| filter.borrow().is_active()) {
        filter.borrow_mut().matches(
            [label_text.as_str(), value.as_str()].into_iter().chain(keywords.iter().map(String::as_str)),
        )
    } else {
        command_item_matches_query(&label_text, &value, &keywords, &query_of(&ctx))
    };
    if !matches {
        let mut hidden = Element::new().w(0.0).h(0.0);
        hidden.set_attr("jadix_command_hidden", "1");
        return hidden;
    }
    let on_click = Rc::new(RefCell::new(on_click));
    let activate = {
        let on_click = on_click.clone();
        Rc::new(move || on_click.borrow_mut().call(())) as Rc<dyn Fn()>
    };
    let activate = command_item_activation(value.clone(), activate);
    let ix = ctx.visible.update_in_place(|list| {
        let ix = list.len();
        list.push(CommandVisibleItem {
            value: value.clone(),
            label: label_text.clone(),
            disabled,
        });
        ix
    });
    activate_registry::push_enabled(&ctx.id, activate.clone(), !disabled);
    let kids = children.map(take_packed)
        .unwrap_or_else(|| vec![Element::new().text(label_text)]);
    command_item_row(ctx, ix, disabled, checked, activate, kids)
}
}

fn command_item_row(
    ctx: CommandCtx,
    ix: usize,
    disabled: bool,
    checked: bool,
    activate_click: Rc<dyn Fn()>,
    mut inner_kids: Vec<Element>,
) -> Element {
    let highlighted = ctx.active.peek() == ix;
    let open = ctx.open;
    let active = ctx.active;
    let selection = command_behavior().selection;
    let value = ctx.visible.with_untracked(|items| items[ix].value.clone());
    let selected =
        selection.is_some_and(|selection| selection.with_untracked(|items| items.contains(&value)));
    let dismissible = ctx.surface.dismissible() && selection.is_none();
    let show_check = selection.is_some() || (checked && !inner_kids.iter().any(has_shortcut_slot));
    if show_check {
        let g = crate::utils::lucide_glyph("check").unwrap_or_else(|| "✓".into());
        let mut check = build_icon_text_node(g)
            .text_size(14.0)
            .text_color(parse_text_color(if highlighted {
                "accent-foreground"
            } else {
                "popover-foreground"
            }))
            .w(14.0)
            .h(14.0)
            .flex_shrink(0.0)
            .opacity(if selection.is_none() || selected {
                1.0
            } else {
                0.0
            })
            .pointer_events_none();
        if selection.is_some() {
            check.set_attr("data-slot", "command-selection-check");
            check.set_attr("aria-hidden", "true");
            let value = value.clone();
            crate::authoring::presentation::attach(
                &mut check,
                crate::authoring::presentation::Phase::State,
                move |element, _, _| {
                    let opacity = if selection
                        .unwrap()
                        .with_untracked(|items| items.contains(&value))
                    {
                        1.0
                    } else {
                        0.0
                    };
                    if element.style.opacity == opacity {
                        LayoutStatus::Clean
                    } else {
                        element.style.opacity = opacity;
                        LayoutStatus::StyleChanged
                    }
                },
            );
        }
        inner_kids.push(check);
    }

    let item_fg = parse_text_color(if highlighted {
        "accent-foreground"
    } else {
        "popover-foreground"
    });
    let query = if command_behavior().searchable {
        query_of(&ctx)
    } else {
        String::new()
    };
    for k in &mut inner_kids {
        style_command_item_text(k, item_fg, &query);
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

    let scroll_id = format!("{}:list", ctx.id);
    let pointer = command_pointer(&ctx.id);
    let pointer_scroll_id = scroll_id.clone();
    let mut hit: Element = interactive(row)
        .on_mousemove(move |event| {
            let at = [event.x, event.y];
            if event.buttons != 0 || !at.iter().all(|v| v.is_finite()) {
                return;
            }
            if pointer.peek() != Some(at) {
                pointer.set_silent(Some(at));
                if !disabled {
                    reset_scroll_item_into_view(&pointer_scroll_id);
                    set_command_active(active, ix);
                }
            }
        })
        .on_mouseleave(move |event| {
            // Layout reconciliation can emit leave under a stationary cursor.
            // Leave must not consume the next row's move sample.
            let at = [event.x, event.y];
            if !disabled
                && event.buttons == 0
                && at.iter().all(|v| v.is_finite())
                && pointer.peek().is_some_and(|previous| previous != at)
                && active.peek() == ix
            {
                set_command_active(active, NO_ACTIVE_COMMAND);
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
                    reset_scroll_item_into_view(&scroll_id);
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
    hit = hit.accesskit_role("listboxoption");
    hit.set_attr("id", format!("{}:item:{ix}", ctx.id));
    hit.set_attr(
        "aria-selected",
        if selected || (selection.is_none() && highlighted && !disabled) {
            "true"
        } else {
            "false"
        },
    );
    hit.set_attr("aria-disabled", disabled.to_string());
    hit.set_attr("aria-posinset", (ix + 1).to_string());
    hit.set_attr(
        "aria-setsize",
        ctx.visible.with_untracked(Vec::len).to_string(),
    );
    hit.set_attr("data-slot", "command-item");
    hit.set_attr("jadix_command_id", ctx.id.clone());
    hit.set_attr("jadix_command_ix", ix.to_string());
    if selection.is_some() {
        hit.set_attr("jadix_command_multiselect", "1");
    }
    hit.set_attr("jadix_scroll_target", format!("{}:item:{ix}", ctx.id));
    crate::authoring::presentation::attach(
        &mut hit,
        crate::authoring::presentation::Phase::State,
        move |element, layout, _context| {
            let mut impact = LayoutStatus::Clean;
            sync_command_item_paint_node(element, layout, &mut impact);
            // Compound items register incrementally. State sees the completed
            // logical set; virtual rows use the same count without mounting it.
            let size = ctx.visible.with_untracked(Vec::len).to_string();
            if element.attrs.get("aria-setsize") != Some(&size) {
                element.set_attr("aria-setsize", size);
                impact = impact.merge(LayoutStatus::StyleChanged);
            }
            if let Some(selection) = selection {
                let selected = selection
                    .with_untracked(|items| items.contains(&value))
                    .to_string();
                if element.attrs.get("aria-selected") != Some(&selected) {
                    element.set_attr("aria-selected", selected);
                    impact = impact.merge(LayoutStatus::StyleChanged);
                }
            }
            impact
        },
    );
    hit = hit
        .text_size(13.0)
        .text_leading(TAILWIND_TEXT_SM_LEADING_SCALE);
    if disabled {
        hit.set_attr("jadix_command_disabled", "1");
    }
    hit
}

fn style_command_item_text(el: &mut Element, color: [u8; 4], query: &str) {
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
                // Preserve caller-provided rich text. Plain label leaves share
                // ComboBox's Unicode match painting, including virtual rows.
                if !query.is_empty() && el.text_spans.is_none() {
                    el.text_spans = Some(crate::style::TextSpan::highlight_matches(
                        el.text.as_deref().unwrap(),
                        query,
                        parse_text_color("match-foreground"),
                    ));
                }
                el.text_style.size = Some(13.0);
                // Caption size stays on the leaf. Line-height lives on the item
                // root (`text-sm`); release Label `leading-none` so cascade copies it.
                let is_label_leading_none = el.text_style.line_height_specified()
                    && el.text_style.line_height_px.is_none()
                    && el.text_style.line_height_scale == Some(1.0);
                if is_label_leading_none {
                    el.text_style.line_height_px = None;
                    el.text_style.line_height_scale = None;
                    el.text_style.set_line_height_specified(false);
                }
            }
            el.text_style.color = Some(color);
        }
        true
    });
}

/// Each frame writes highlight from `{id}__cmd_active` (`peek`, no root-effect subscription).
pub(crate) fn sync_command_item_paint_node(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    sync_command_palette_focus(el);
    if el.attrs.get("data-slot").map(String::as_str) == Some("command-item") {
        let disabled = el.attrs.get("jadix_command_disabled").map(String::as_str) == Some("1");
        let ix = el
            .attrs
            .get("jadix_command_ix")
            .and_then(|s| s.parse::<usize>().ok());
        let id = el.attrs.get("jadix_command_id").cloned();
        if let (Some(id), Some(ix)) = (id, ix) {
            activate_registry::update_bounds(&id, ix, el.layout_bounds);
            let active =
                use_signal(format!("{id}__cmd_active").as_str(), || NO_ACTIVE_COMMAND).peek();
            let on = !disabled && active == ix;
            let selected = if on { "true" } else { "false" };
            if !el.attrs.contains_key("jadix_command_multiselect")
                && el.attrs.get("aria-selected").map(String::as_str) != Some(selected)
            {
                el.set_attr("aria-selected", selected);
                *impact = impact.merge(LayoutStatus::StyleChanged);
            }
            apply_command_item_highlight(el, layout_tree, impact, on);
        }
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

crate::__component_contract! {
CommandShortcutProps [];
/// `CommandShortcut`：`ml-auto text-xs tracking-widest text-muted-foreground`。
#[allow(non_snake_case)]
pub fn CommandShortcut(
plain [] children: Option<Element>,
) -> Element {
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
    keywords: Vec<String>,
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn command_item(value: impl Into<String>) -> CommandItemBuilder {
    CommandItemBuilder {
        value: value.into(),
        disabled: false,
        checked: false,
        keywords: Vec::new(),
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
    pub fn keywords<I, S>(mut self, keywords: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.keywords.extend(keywords.into_iter().map(Into::into));
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
            keywords: Some(builder.keywords),
            on_click: Default::default(),
            children: crate::components::base::packed_children(builder.children),
            base: None,
        });
        builder.chrome.apply(el)
    }
}

impl crate::core::tree::IntoElementList for CommandItemBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/command.rs"]
mod tests;

#[cfg(all(test, feature = "product-tests"))]
pub(crate) fn sync_command_item_paint_subtree(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    crate::core::tree::walk_elements_mut(el, |element| {
        sync_command_item_paint_node(element, layout_tree, impact)
    });
}
