//! # Timeline
//!
//! Vertical event timeline aligned with solid-ui
//! [`timeline.tsx`](../../../../Reference/solid-ui-main/apps/docs/src/registry/ui/timeline.tsx)
//! (ShadCN-style chrome; **not** an official shadcn/ui registry primitive).
//!
//! ```text
//! Timeline                            // ul · padding for the rail
//! └── TimelineItem                    // li · rail + title + description
//!     ├── TimelineItemBullet          // disc on the line (optional Lucide)
//!     ├── TimelineItemTitle           // text-base font-semibold
//!     └── TimelineItemDescription?    // text-sm text-muted-foreground
//! ```
//!
//! solid-ui `activeItem` (0-based, `-1` = nothing active):
//! - bullet `i` is primary when `activeItem >= i`
//! - connector under item `i` is primary when `activeItem >= i + 1`
//!
//! `<Timeline>` expands to [`timeline_scope`]. Builder [`timeline`] shares that
//! path (`.items()` / `.item()` sugar or `.child(|| …)`).
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags (camelCase: activeItem, activeItemSignal, defaultActive,
//! // bulletSize, lineSize)
//! ui! {
//!     <Timeline activeItem={step} bulletSize={32} lineSize={2}>
//!         <TimelineItem>
//!             <TimelineItemBullet>
//!                 <Icon name="shopping-cart" />
//!             </TimelineItemBullet>
//!             <TimelineItemTitle>"Order placed"</TimelineItemTitle>
//!             <TimelineItemDescription>"We received your order."</TimelineItemDescription>
//!         </TimelineItem>
//!         <TimelineItem bullet="package">
//!             <TimelineItemTitle>"Packed"</TimelineItemTitle>
//!             <TimelineItemDescription>"Left the warehouse."</TimelineItemDescription>
//!         </TimelineItem>
//!         <TimelineItem>
//!             <TimelineItemTitle>"Delivered"</TimelineItemTitle>
//!         </TimelineItem>
//!     </Timeline>
//! }
//!
//! // B · Builder (snake_case only)
//! Element::from(
//!     timeline()
//!         .active_item_signal(step)
//!         .bullet_size(32.0)
//!         .line_size(2.0)
//!         .item(("Order placed", "We received your order."))
//!         .item(
//!             TimelineEntry::from(("Packed", "Left the warehouse.")).bullet("package"),
//!         )
//!         .item("Delivered"),
//! )
//! ```
//!
//! ## Notes
//! - **Identity (WP-2.8)**: `id` optional; empty → [`resolve_instance_id`].
//! - Disc: `rounded-full` · `bg-background` · border width = `line_size`
//!   (`border` idle, `primary` when the bullet is active).
//! - Title: `text-base` / `font-semibold` / `leading-none`. Description:
//!   `text-sm` / `text-muted-foreground`. Item `pb-8` except the last.
//! - Custom bullet glyphs are Lucide **names** (RSX `<Icon name>` or Builder
//!   `.bullet("check")`). Arbitrary extra nodes in the disc are not a product path.

use std::cell::RefCell;
use std::sync::OnceLock;

use crate::app::reactive::reactive_view;
use crate::components::base::{
    push_pending_item_chrome, take_pending_item_chrome, BaseComponentBuilder, SlotAuthor,
    SlotChrome,
};
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, with_child_owner, Context, Signal,
};
use crate::core::tree::{parse_color, parse_text_color, Element, IntoF32Raw};
use crate::impl_slot_chrome_methods;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::style::{apply_semantic_border, with_class_list, SemanticBorder};
use crate::utils::icon_provider::{build_icon_named, with_icon_size};
use crate::utils::resolve_instance_id;
use crate::utils::take_packed;
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use crate::utils::ATTR_ICON_NAME;

const ATTR_SLOT: &str = "data-slot";
const ATTR_STATE: &str = "data-state";
const ATTR_INDEX: &str = "data-index";
const ATTR_ACTIVE: &str = "data-active";

const SLOT_ROOT: &str = "timeline";
const SLOT_ITEM: &str = "timeline-item";
const SLOT_BULLET: &str = "timeline-item-bullet";
const SLOT_TITLE: &str = "timeline-item-title";
const SLOT_DESCRIPTION: &str = "timeline-item-description";
const SLOT_SEPARATOR: &str = "timeline-item-separator";

/// Default disc diameter, with room for a legible Lucide glyph and inset.
const DEFAULT_BULLET_PX: f32 = 32.0;
/// solid-ui default `lineSize`.
const DEFAULT_LINE_PX: f32 = 2.0;
/// Icon inside the disc as a fraction of its diameter (≈21px at default).
const BULLET_GLYPH_RATIO: f32 = 0.65;
const BULLET_GLYPH_MIN_PX: f32 = 10.0;
/// `pb-8` between items.
const ITEM_PB: &str = "2rem";
/// Keep the larger disc close enough to its event copy to read as one row.
const BODY_GAP: &str = "0.75rem";
const TITLE_SIZE_PX: f32 = 16.0;
const TITLE_WEIGHT: u16 = 600;
const DESC_SIZE_PX: f32 = 14.0;

// ---------------------------------------------------------------------------
// Entry / context
// ---------------------------------------------------------------------------

/// One timeline row for Builder `.item()` / `.items()` sugar.
/// Default appearance; per-row class/style → [`timeline_entry`].
#[derive(Clone, Debug, Default)]
pub struct TimelineEntry {
    pub title: String,
    pub description: String,
    /// Optional Lucide icon name inside the disc.
    pub bullet: Option<String>,
    pub chrome: SlotChrome,
}

/// Sugar entry: `timeline().items([timeline_entry("Shipped").class("ring-2")])`.
pub fn timeline_entry(title: impl Into<String>) -> TimelineEntry {
    TimelineEntry {
        title: title.into(),
        description: String::new(),
        bullet: None,
        chrome: SlotChrome::default(),
    }
}

impl TimelineEntry {
    crate::impl_slot_chrome_methods!();

    pub fn description(mut self, text: impl Into<String>) -> Self {
        self.description = text.into();
        self
    }

    pub fn bullet(mut self, name: impl Into<String>) -> Self {
        let name = name.into();
        self.bullet = if name.trim().is_empty() {
            None
        } else {
            Some(name)
        };
        self
    }
}

impl From<&str> for TimelineEntry {
    fn from(s: &str) -> Self {
        Self {
            title: s.to_string(),
            description: String::new(),
            bullet: None,
            chrome: SlotChrome::default(),
        }
    }
}

impl From<String> for TimelineEntry {
    fn from(title: String) -> Self {
        Self {
            title,
            description: String::new(),
            bullet: None,
            chrome: SlotChrome::default(),
        }
    }
}

impl From<(&str, &str)> for TimelineEntry {
    fn from((title, description): (&str, &str)) -> Self {
        Self {
            title: title.to_string(),
            description: description.to_string(),
            bullet: None,
            chrome: SlotChrome::default(),
        }
    }
}

impl From<(String, String)> for TimelineEntry {
    fn from((title, description): (String, String)) -> Self {
        Self {
            title,
            description,
            bullet: None,
            chrome: SlotChrome::default(),
        }
    }
}

/// Root context: 0-based `active` (`-1` = none), disc / line sizes.
#[derive(Clone, Copy)]
pub struct TimelineCtx {
    pub active: Signal<i32>,
    pub bullet_size: f32,
    pub line_size: f32,
}

#[derive(Clone, Default)]
struct TimelineItemSpec {
    index: usize,
    title: String,
    description: String,
    bullet: Option<String>,
    chrome: SlotChrome,
    bullet_slot: SlotAuthor,
    title_slot: SlotAuthor,
    description_slot: SlotAuthor,
}

struct TimelineCollect {
    seq: usize,
    items: Vec<TimelineItemSpec>,
}

thread_local! {
    static TIMELINE_STACK: RefCell<Vec<TimelineCollect>> = const { RefCell::new(Vec::new()) };
}

fn timeline_context() -> Context<TimelineCtx> {
    static CTX: OnceLock<Context<TimelineCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<TimelineCtx>)
}

/// Read the nearest [`Timeline`] context.
pub fn use_timeline() -> Option<TimelineCtx> {
    use_context(timeline_context())
}

fn require_timeline(part: &str) -> TimelineCtx {
    use_timeline().unwrap_or_else(|| {
        panic!("{part} must be used within <Timeline> / timeline_scope");
    })
}

fn push_collect() {
    TIMELINE_STACK.with(|s| {
        s.borrow_mut().push(TimelineCollect {
            seq: 0,
            items: Vec::new(),
        });
    });
}

fn pop_collect() -> Vec<TimelineItemSpec> {
    TIMELINE_STACK.with(|s| s.borrow_mut().pop().map(|c| c.items).unwrap_or_default())
}

fn register_item(spec: TimelineItemSpec) {
    TIMELINE_STACK.with(|s| {
        if let Some(top) = s.borrow_mut().last_mut() {
            top.items.push(spec);
        }
    });
}

fn next_item_index() -> usize {
    TIMELINE_STACK.with(|s| {
        if let Some(top) = s.borrow_mut().last_mut() {
            let i = top.seq;
            top.seq += 1;
            i
        } else {
            0
        }
    })
}

/// solid-ui: no chrome when `activeItem == -1`; else bullet `i` if `active >= i`.
pub fn timeline_bullet_active(active: i32, index: usize) -> bool {
    active >= 0 && active >= index as i32
}

/// Lucide glyph size inside a bullet disc.
pub fn bullet_glyph_px(bullet_size: f32) -> f32 {
    (bullet_size * BULLET_GLYPH_RATIO).max(BULLET_GLYPH_MIN_PX)
}

/// solid-ui: line under item `i` is primary when `active >= i + 1`.
pub fn timeline_line_active(active: i32, index: usize, is_last: bool) -> bool {
    if is_last {
        false
    } else {
        active >= 0 && active > (index as i32)
    }
}

/// Provide timeline context, run `body` so items can register, then paint.
pub fn timeline_scope(
    id: impl Into<String>,
    active: Option<Signal<i32>>,
    default_active: i32,
    bullet_size: f32,
    line_size: f32,
    class: Option<String>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = resolve_instance_id(id, "timeline");
    let bullet_size = bullet_size.max(1.0);
    let line_size = line_size.max(1.0);
    let active = active
        .unwrap_or_else(|| use_signal(format!("{id}__active").as_str(), move || default_active));

    with_child_owner(|| {
        provide_context(
            timeline_context(),
            TimelineCtx {
                active,
                bullet_size,
                line_size,
            },
        );
        push_collect();
        let _ = body();
        let specs = pop_collect();
        let view_id = format!("{id}__timeline_view");
        let class = class.clone();
        let view = reactive_view(view_id, move || {
            let current = active.get();
            paint_timeline(&specs, current, bullet_size, line_size, class.as_deref())
        });
        let mut shell: Element = r#box().w("100%").child(view).into();
        shell.template_id = Some(format!("{id}__timeline"));
        shell
    })
}

fn paint_timeline(
    specs: &[TimelineItemSpec],
    active: i32,
    bullet_size: f32,
    line_size: f32,
    class: Option<&str>,
) -> Element {
    let n = specs.len();
    let mut col = vstack().w("100%").gap("0");
    for (i, spec) in specs.iter().enumerate() {
        let is_last = i + 1 == n;
        col = col.child(paint_item(spec, active, is_last, bullet_size, line_size));
    }
    let mut el: Element = col.into();
    el.set_attr(ATTR_SLOT, SLOT_ROOT);
    el.set_attr(ATTR_ACTIVE, active.to_string());
    el = el.accesskit_role("list");
    if let Some(c) = class {
        if !c.trim().is_empty() {
            el = with_class_list(el, c);
        }
    }
    el
}

fn paint_item(
    spec: &TimelineItemSpec,
    active: i32,
    is_last: bool,
    bullet_size: f32,
    line_size: f32,
) -> Element {
    let bullet_on = timeline_bullet_active(active, spec.index);
    let line_on = timeline_line_active(active, spec.index, is_last);
    let disc = spec.bullet_slot.apply(paint_bullet(
        spec.bullet.as_deref(),
        bullet_on,
        bullet_size,
        line_size,
    ));

    // Equal flexible segments center the disc against the whole text block.
    // The fixed tail matches the body's bottom padding, keeping row spacing
    // out of the centering calculation and connectors continuous when text wraps.
    let before =
        paint_separator(bullet_on, line_size).opacity(if spec.index == 0 { 0.0 } else { 1.0 });
    let after = paint_separator(line_on, line_size).opacity(if is_last { 0.0 } else { 1.0 });
    let mut rail = vstack()
        .w(bullet_size)
        .items_center()
        .flex_shrink(0.0)
        .child(before)
        .child(disc)
        .child(after);
    if !is_last {
        rail = rail.child(
            paint_separator(line_on, line_size)
                .h(ITEM_PB)
                .flex_grow(0.0),
        );
    }

    let mut body = vstack()
        .gap("0.5rem")
        .min_w(0.0)
        .flex_grow(1.0)
        .justify_center()
        .pb(if is_last { "0" } else { ITEM_PB });
    body = body.child(spec.title_slot.apply(paint_title(&spec.title)));
    if !spec.description.trim().is_empty() {
        body = body.child(
            spec.description_slot
                .apply(paint_description(&spec.description)),
        );
    }

    let cell = hstack()
        .w("100%")
        .items("stretch")
        .gap(BODY_GAP)
        .child(rail)
        .child(body);
    let mut el: Element = cell.into();
    el.set_attr(ATTR_SLOT, SLOT_ITEM);
    el.set_attr(ATTR_INDEX, spec.index.to_string());
    el.set_attr(ATTR_STATE, if bullet_on { "active" } else { "inactive" });
    el = el.accesskit_role("listitem");
    spec.chrome.apply(el)
}

fn paint_separator(active: bool, line_size: f32) -> Element {
    let mut line: Element = r#box()
        .w(line_size)
        .h(0.0)
        .min_h(0.0)
        .flex_grow(1.0)
        .flex_shrink(0.0)
        .into();
    line = line.bg(parse_color(if active { "primary" } else { "border" }));
    line.set_attr(ATTR_SLOT, SLOT_SEPARATOR);
    line.set_attr(ATTR_STATE, if active { "active" } else { "inactive" });
    line.set_attr("aria-hidden", "true");
    line
}

fn paint_bullet(icon: Option<&str>, active: bool, bullet_size: f32, line_size: f32) -> Element {
    let mut disc: Element = r#box()
        .w(bullet_size)
        .h(bullet_size)
        .rounded_full()
        .items_center()
        .justify_center()
        .flex_shrink(0.0)
        .z_index(1)
        .into();
    disc = disc.bg(parse_color("background"));
    apply_semantic_border(
        &mut disc,
        Some(SemanticBorder {
            color: if active {
                parse_color("primary")
            } else {
                parse_color("border")
            },
            width: line_size,
        }),
    );
    if let Some(name) = icon {
        if !name.trim().is_empty() {
            let glyph_px = bullet_glyph_px(bullet_size);
            let color = if active {
                parse_text_color("primary")
            } else {
                parse_text_color("border")
            };
            let icon_el = with_icon_size(build_icon_named(name), glyph_px)
                .text_color(color)
                .pointer_events_none();
            disc = disc.child(icon_el);
        }
    }
    disc.set_attr(ATTR_SLOT, SLOT_BULLET);
    disc.set_attr(ATTR_STATE, if active { "active" } else { "inactive" });
    disc.set_attr("aria-hidden", "true");
    disc
}

fn paint_title(text: &str) -> Element {
    let node = build_text_node(
        text,
        TextNodeConfig {
            size_px: TITLE_SIZE_PX,
            weight: TITLE_WEIGHT,
            color: parse_text_color("foreground"),
            line_height_px: Some(TITLE_SIZE_PX),
            ..Default::default()
        },
    );
    let mut wrap: Element = vstack().w("100%").min_w(0.0).child(node).into();
    wrap.set_attr(ATTR_SLOT, SLOT_TITLE);
    wrap
}

fn paint_description(text: &str) -> Element {
    let node = build_text_node(
        text,
        TextNodeConfig {
            size_px: DESC_SIZE_PX,
            weight: 400,
            color: parse_text_color("muted-foreground"),
            ..Default::default()
        },
    );
    let mut wrap: Element = vstack().w("100%").min_w(0.0).child(node).into();
    wrap.set_attr(ATTR_SLOT, SLOT_DESCRIPTION);
    wrap
}

fn extract_text(el: &Element) -> String {
    if let Some(t) = &el.text {
        if !t.is_empty() {
            return t.clone();
        }
    }
    for c in &el.children {
        let t = extract_text(c);
        if !t.is_empty() {
            return t;
        }
    }
    String::new()
}

fn slot_of(el: &Element) -> Option<&str> {
    el.attrs.get(ATTR_SLOT).map(|s| s.as_str())
}

fn find_icon_name(el: &Element) -> Option<String> {
    crate::core::tree::find_in_tree(el, |n| {
        n.get_attr(ATTR_ICON_NAME)
            .is_some_and(|s| !s.trim().is_empty())
    })
    .and_then(|n| n.get_attr(ATTR_ICON_NAME).cloned())
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

pub struct TimelineItemBuilder {
    title: String,
    description: String,
    bullet: Option<String>,
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn timeline_item() -> TimelineItemBuilder {
    TimelineItemBuilder {
        title: String::new(),
        description: String::new(),
        bullet: None,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl TimelineItemBuilder {
    impl_slot_chrome_methods!();

    pub fn title(mut self, text: impl Into<String>) -> Self {
        self.title = text.into();
        self
    }

    pub fn description(mut self, text: impl Into<String>) -> Self {
        self.description = text.into();
        self
    }

    pub fn bullet(mut self, name: impl Into<String>) -> Self {
        self.bullet = Some(name.into());
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl From<TimelineItemBuilder> for Element {
    fn from(builder: TimelineItemBuilder) -> Self {
        push_pending_item_chrome(builder.chrome);
        TimelineItem(TimelineItemProps {
            title: Some(builder.title),
            description: if builder.description.is_empty() {
                None
            } else {
                Some(builder.description)
            },
            bullet: builder.bullet,
            children: if builder.children.is_empty() {
                None
            } else {
                let mut frag = Element::new();
                frag.set_attr("jadix_fragment", "1");
                Some(frag.children(builder.children))
            },
            base: None,
        })
    }
}

pub struct TimelineBuilder {
    id: String,
    items: Vec<TimelineEntry>,
    active: Option<Signal<i32>>,
    default_active: i32,
    bullet_size: f32,
    line_size: f32,
    chrome: SlotChrome,
    item_chrome: SlotChrome,
    custom_items: Vec<Box<dyn FnOnce() -> Element>>,
    child: Option<Box<dyn FnOnce() -> Element>>,
}

/// Builder entry (ADR-9 surface B). `id` is optional (WP-2.8).
pub fn timeline() -> TimelineBuilder {
    TimelineBuilder {
        id: String::new(),
        items: Vec::new(),
        active: None,
        default_active: -1,
        bullet_size: DEFAULT_BULLET_PX,
        line_size: DEFAULT_LINE_PX,
        chrome: SlotChrome::default(),
        item_chrome: SlotChrome::default(),
        custom_items: Vec::new(),
        child: None,
    }
}

impl TimelineBuilder {
    impl_slot_chrome_methods!();

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    /// Data-driven sugar: [`timeline_entry`] / `&str` / `(title, description)`.
    /// Per-item class/style ride on the sugar value; the group default is [`Self::item_chrome`].
    pub fn items(mut self, items: impl IntoIterator<Item = impl Into<TimelineEntry>>) -> Self {
        self.items = items.into_iter().map(|s| s.into()).collect();
        self
    }

    pub fn item(mut self, item: impl Into<TimelineEntry>) -> Self {
        self.items.push(item.into());
        self
    }

    pub fn item_chrome(mut self, chrome: SlotChrome) -> Self {
        self.item_chrome = chrome;
        self
    }

    pub fn item_with(
        mut self,
        f: impl FnOnce(TimelineItemBuilder) -> TimelineItemBuilder + 'static,
    ) -> Self {
        self.custom_items
            .push(Box::new(move || f(timeline_item()).into()));
        self
    }

    pub fn active_item_signal(mut self, s: Signal<i32>) -> Self {
        self.active = Some(s);
        self
    }

    /// Uncontrolled / seed value. `-1` leaves every disc and connector idle.
    pub fn active_item(mut self, i: i32) -> Self {
        self.default_active = i;
        self
    }

    pub fn bullet_size(mut self, v: impl IntoF32Raw) -> Self {
        self.bullet_size = v.into_f32_raw().max(1.0);
        self
    }

    pub fn line_size(mut self, v: impl IntoF32Raw) -> Self {
        self.line_size = v.into_f32_raw().max(1.0);
        self
    }

    /// Compound surface: parts run inside [`timeline_scope`].
    pub fn child(mut self, body: impl FnOnce() -> Element + 'static) -> Self {
        self.child = Some(Box::new(body));
        self
    }
}

impl crate::core::tree::IntoElementList for TimelineBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<TimelineBuilder> for Element {
    fn from(builder: TimelineBuilder) -> Self {
        let TimelineBuilder {
            id,
            items,
            active,
            default_active,
            bullet_size,
            line_size,
            chrome,
            item_chrome,
            custom_items,
            child,
        } = builder;
        let class = if chrome.class.trim().is_empty() {
            None
        } else {
            Some(chrome.class.clone())
        };
        let style = chrome.style.clone();
        let mut el = timeline_scope(
            id,
            active,
            default_active,
            bullet_size,
            line_size,
            class,
            move || {
                if let Some(body) = child {
                    body()
                } else {
                    let mut frag = Element::new();
                    frag.set_attr("jadix_fragment", "1");
                    let mut kids: Vec<Element> = items
                        .into_iter()
                        .map(|s| {
                            let chrome = item_chrome.clone().overlay(s.chrome);
                            push_pending_item_chrome(chrome);
                            TimelineItem(TimelineItemProps {
                                title: Some(s.title),
                                description: if s.description.is_empty() {
                                    None
                                } else {
                                    Some(s.description)
                                },
                                bullet: s.bullet,
                                children: None,
                                base: None,
                            })
                        })
                        .collect();
                    for build in custom_items {
                        kids.push(build());
                    }
                    frag.children(kids)
                }
            },
        );
        if let Some(style) = style {
            el = style(BaseComponentBuilder::new(el)).el;
        }
        el
    }
}

crate::__component_contract! {
TimelineProps [];
/// RSX fallback when the `ui!` special-case is not used. Compound children must
/// go through the macro → [`timeline_scope`] so items see context.
#[allow(non_snake_case)]
pub fn Timeline(
identity [] id: String,
plain [] active: Option<Signal<i32>>,
plain [] active_item: Option<Signal<i32>>,
plain [] active_item_signal: Option<Signal<i32>>,
plain [] default_active: Option<i32>,
plain [] bullet_size: Option<f32>,
plain [] line_size: Option<f32>,
plain [] children: Option<Element>,
) -> Element {
    let mut b = timeline().id(id);
    if let Some(s) = active_item_signal.or(active_item).or(active) {
        b = b.active_item_signal(s);
    } else {
        b = b.active_item(default_active.unwrap_or(-1));
    }
    if let Some(v) = bullet_size {
        b = b.bullet_size(v);
    }
    if let Some(v) = line_size {
        b = b.line_size(v);
    }
    if let Some(ch) = children {
        let _ = ch;
    }
    b.into()
}
}

// ---------------------------------------------------------------------------
// Parts
// ---------------------------------------------------------------------------

crate::__component_contract! {
TimelineItemProps [];
/// One event. Auto-indexed inside [`timeline_scope`].
#[allow(non_snake_case)]
pub fn TimelineItem(
plain [] title: Option<String>,
plain [] description: Option<String>,
plain [] bullet: Option<String>,
plain [] children: Option<Element>,
) -> Element {
    let _ctx = require_timeline("TimelineItem");
    let index = next_item_index();

    let mut title = title.unwrap_or_default();
    let mut description = description.unwrap_or_default();
    let mut bullet = bullet.filter(|s| !s.trim().is_empty());
    let mut title_slot = SlotAuthor::default();
    let mut description_slot = SlotAuthor::default();
    let mut bullet_slot = SlotAuthor::default();
    if let Some(ch) = children {
        for child in take_packed(ch) {
            match slot_of(&child) {
                Some(SLOT_TITLE) => {
                    title_slot = SlotAuthor::from_el(&child);
                    let t = extract_text(&child);
                    if !t.is_empty() {
                        title = t;
                    }
                }
                Some(SLOT_DESCRIPTION) => {
                    description_slot = SlotAuthor::from_el(&child);
                    let t = extract_text(&child);
                    if !t.is_empty() {
                        description = t;
                    }
                }
                Some(SLOT_BULLET) => {
                    bullet_slot = SlotAuthor::from_el(&child);
                    // Icon children carry default foreground/size/weight. Only
                    // styles authored on the bullet slot override state chrome.
                    bullet_slot.color = child.text_style.color;
                    bullet_slot.size = child.text_style.size;
                    bullet_slot.weight = child.text_style.weight;
                    if bullet.is_none() {
                        bullet = find_icon_name(&child);
                    }
                }
                _ => {
                    if bullet.is_none() {
                        bullet = find_icon_name(&child);
                    }
                    let t = extract_text(&child);
                    if title.is_empty() && !t.is_empty() && find_icon_name(&child).is_none() {
                        title = t;
                    }
                }
            }
        }
    }

    let chrome = take_pending_item_chrome();
    register_item(TimelineItemSpec {
        index,
        title,
        description,
        bullet,
        chrome,
        bullet_slot,
        title_slot,
        description_slot,
    });
    Element::new()
}
}

crate::__component_contract! {
TimelineItemBulletProps [];
/// Disc slot. Optional `<Icon name>` child becomes the Lucide glyph.
#[allow(non_snake_case)]
pub fn TimelineItemBullet(
plain [] children: Option<Element>,
) -> Element {
    let mut el: Element = r#box().into();
    el.set_attr(ATTR_SLOT, SLOT_BULLET);
    if let Some(ch) = children {
        el = el.children(take_packed(ch));
    }
    el
}
}

crate::__component_contract! {
TimelineItemTitleProps [];
/// Event title (`text-base` / `font-semibold` / `leading-none`).
#[allow(non_snake_case)]
pub fn TimelineItemTitle(
plain [] children: Option<Element>,
) -> Element {
    let mut el: Element = r#box().into();
    el.set_attr(ATTR_SLOT, SLOT_TITLE);
    if let Some(ch) = children {
        el = el.children(take_packed(ch));
    }
    el
}
}

crate::__component_contract! {
TimelineItemDescriptionProps [];
/// Supporting copy (`text-sm` · `text-muted-foreground`).
#[allow(non_snake_case)]
pub fn TimelineItemDescription(
plain [] children: Option<Element>,
) -> Element {
    let mut el: Element = r#box().into();
    el.set_attr(ATTR_SLOT, SLOT_DESCRIPTION);
    if let Some(ch) = children {
        el = el.children(take_packed(ch));
    }
    el
}
}

crate::impl_simple_slot_builder!(
    TimelineItemBulletBuilder,
    timeline_item_bullet,
    TimelineItemBullet,
    TimelineItemBulletProps
);
crate::impl_simple_slot_builder!(
    TimelineItemTitleBuilder,
    timeline_item_title,
    TimelineItemTitle,
    TimelineItemTitleProps
);
crate::impl_simple_slot_builder!(
    TimelineItemDescriptionBuilder,
    timeline_item_description,
    TimelineItemDescription,
    TimelineItemDescriptionProps
);

impl TimelineItemBuilder {
    pub fn title_with(
        self,
        f: impl FnOnce(TimelineItemTitleBuilder) -> TimelineItemTitleBuilder,
    ) -> Self {
        self.child(f(timeline_item_title()))
    }

    pub fn description_with(
        self,
        f: impl FnOnce(TimelineItemDescriptionBuilder) -> TimelineItemDescriptionBuilder,
    ) -> Self {
        self.child(f(timeline_item_description()))
    }

    pub fn bullet_with(
        self,
        f: impl FnOnce(TimelineItemBulletBuilder) -> TimelineItemBulletBuilder,
    ) -> Self {
        self.child(f(timeline_item_bullet()))
    }
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/timeline.rs"]
mod tests;
