//! # Stepper
//!
//! Numbered step indicator: current / completed / upcoming chrome, optional
//! descriptions, horizontal or vertical orientation.
//!
//! **Not an official ShadCN component** — ShadCN has no `stepper` in the
//! registry. This is a RUI control that reuses the same tokens and density
//! (`size-8` discs, `text-sm` titles, `bg-primary` / `bg-muted` / `bg-border`
//! connectors, Lucide `check` on completed steps).
//!
//! ```text
//! Stepper                         // Root · orientation horizontal | vertical
//! ├── StepperItem                 // one step (auto index)
//! │   ├── StepperIndicator        // disc · number or check
//! │   ├── StepperTitle
//! │   └── StepperDescription?     // muted supporting copy
//! └── (auto connector between items)
//! ```
//!
//! `<Stepper>` expands to [`stepper_scope`] so items see [`StepperCtx`] before
//! they mount. Builder [`stepper`] shares that path (`.steps()` sugar or
//! `.child(|| …)`).
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags (camelCase: currentSignal, defaultStep)
//! ui! {
//!     <Stepper current={step} orientation="horizontal">
//!         <StepperItem>
//!             <StepperIndicator />
//!             <StepperTitle>"Account"</StepperTitle>
//!             <StepperDescription>"Email and password"</StepperDescription>
//!         </StepperItem>
//!         <StepperItem>
//!             <StepperIndicator />
//!             <StepperTitle>"Profile"</StepperTitle>
//!         </StepperItem>
//!         <StepperItem>
//!             <StepperIndicator />
//!             <StepperTitle>"Confirm"</StepperTitle>
//!         </StepperItem>
//!     </Stepper>
//! }
//!
//! // B · Builder (snake_case only)
//! Element::from(
//!     stepper()
//!         .current_signal(step)
//!         .steps([
//!             ("Account", "Email and password"),
//!             ("Profile", "Your details"),
//!             ("Confirm", "Review and submit"),
//!         ]),
//! )
//! stepper().default_step(1).orientation("vertical").steps(["A", "B", "C"])
//! ```
//!
//! ## Notes
//! - **Identity (WP-2.8)**: `id` optional; empty → [`resolve_instance_id`].
//! - Controlled `current` / `current_signal` is 0-based. Uncontrolled uses
//!   `default_step` (also 0-based) on an instance-keyed signal.
//! - Clicking an item sets `current` when `clickable` is true (default).
//! - Completed discs show a Lucide check; the active disc is filled primary;
//!   upcoming discs use muted fill + muted-foreground numerals.
//! - Connectors sit in a slot the same cross-size as the disc so they line up
//!   with the circle, not the title block.
//! - Titles are `whitespace-nowrap` + `leading-none` so a current-step change
//!   cannot drop the last letter onto the description.

use std::cell::RefCell;
use std::sync::OnceLock;

use crate::app::reactive::reactive_view;
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, with_child_owner, Context, Signal,
};
use crate::core::tree::{parse_color, parse_text_color, Element};
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::style::{apply_semantic_border, with_class_list, SemanticBorder};
use crate::tokens;
use crate::utils::lucide_glyph;
use crate::utils::resolve_instance_id;
use crate::utils::take_packed;
use crate::utils::text_node::{build_icon_text_node, build_text_node, TextNodeConfig};
use rui_macros::component;

const ATTR_SLOT: &str = "data-slot";
const ATTR_ORIENTATION: &str = "data-orientation";
const ATTR_STATE: &str = "data-state";
const ATTR_INDEX: &str = "data-index";

const SLOT_ROOT: &str = "stepper";
const SLOT_ITEM: &str = "stepper-item";
const SLOT_INDICATOR: &str = "stepper-indicator";
const SLOT_TITLE: &str = "stepper-title";
const SLOT_DESCRIPTION: &str = "stepper-description";
const SLOT_SEPARATOR: &str = "stepper-separator";

/// ShadCN-style `size-8` disc.
const INDICATOR_SIZE: &str = "2rem";
const INDICATOR_GLYPH_PX: f32 = 14.0;
const CHECK_PX: f32 = 14.0;
const TITLE_SIZE_PX: f32 = 14.0;
const TITLE_WEIGHT_ACTIVE: u16 = 500;
const TITLE_WEIGHT_IDLE: u16 = 400;
const DESC_SIZE_PX: f32 = 12.0;
const CONNECTOR_H: &str = "2px";
const CONNECTOR_W: &str = "2px";
const VERTICAL_CONNECTOR_H: &str = "1.5rem";

// ---------------------------------------------------------------------------
// Orientation / state
// ---------------------------------------------------------------------------

/// Stepper main axis (same vocabulary as Tabs / Progress).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StepperOrientation {
    #[default]
    Horizontal,
    Vertical,
}

impl StepperOrientation {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "vertical" | "col" | "column" => Self::Vertical,
            _ => Self::Horizontal,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }

    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Vertical)
    }
}

impl From<&str> for StepperOrientation {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for StepperOrientation {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StepState {
    Completed,
    Active,
    Inactive,
}

impl StepState {
    fn from_index(index: usize, current: usize) -> Self {
        if index < current {
            Self::Completed
        } else if index == current {
            Self::Active
        } else {
            Self::Inactive
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Active => "active",
            Self::Inactive => "inactive",
        }
    }
}

/// One step for Builder `.steps()` sugar (title + optional description).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StepperStep {
    pub title: String,
    pub description: String,
}

impl From<&str> for StepperStep {
    fn from(s: &str) -> Self {
        Self {
            title: s.to_string(),
            description: String::new(),
        }
    }
}

impl From<String> for StepperStep {
    fn from(title: String) -> Self {
        Self {
            title,
            description: String::new(),
        }
    }
}

impl From<(&str, &str)> for StepperStep {
    fn from((title, description): (&str, &str)) -> Self {
        Self {
            title: title.to_string(),
            description: description.to_string(),
        }
    }
}

impl From<(String, String)> for StepperStep {
    fn from((title, description): (String, String)) -> Self {
        Self { title, description }
    }
}

// ---------------------------------------------------------------------------
// Context / scope
// ---------------------------------------------------------------------------

/// Root context: 0-based `current`, instance `id`, orientation, click-to-jump.
#[derive(Clone, Copy)]
pub struct StepperCtx {
    pub current: Signal<usize>,
    pub orientation: StepperOrientation,
    pub clickable: bool,
}

#[derive(Clone)]
struct StepperItemSpec {
    index: usize,
    title: String,
    description: String,
    disabled: bool,
    class: String,
}

struct StepperCollect {
    seq: usize,
    items: Vec<StepperItemSpec>,
}

thread_local! {
    static STEPPER_STACK: RefCell<Vec<StepperCollect>> = const { RefCell::new(Vec::new()) };
}

fn stepper_context() -> Context<StepperCtx> {
    static CTX: OnceLock<Context<StepperCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<StepperCtx>)
}

/// Read the nearest [`Stepper`] context.
pub fn use_stepper() -> Option<StepperCtx> {
    use_context(stepper_context())
}

fn require_stepper(part: &str) -> StepperCtx {
    use_stepper().unwrap_or_else(|| {
        panic!("{part} must be used within <Stepper> / stepper_scope");
    })
}

fn push_collect() {
    STEPPER_STACK.with(|s| {
        s.borrow_mut().push(StepperCollect {
            seq: 0,
            items: Vec::new(),
        });
    });
}

fn pop_collect() -> Vec<StepperItemSpec> {
    STEPPER_STACK.with(|s| s.borrow_mut().pop().map(|c| c.items).unwrap_or_default())
}

fn register_item(spec: StepperItemSpec) {
    STEPPER_STACK.with(|s| {
        if let Some(top) = s.borrow_mut().last_mut() {
            top.items.push(spec);
        }
    });
}

fn next_item_index() -> usize {
    STEPPER_STACK.with(|s| {
        if let Some(top) = s.borrow_mut().last_mut() {
            let i = top.seq;
            top.seq += 1;
            i
        } else {
            0
        }
    })
}

/// Provide stepper context, run `body` so items can register, then paint.
pub fn stepper_scope(
    id: impl Into<String>,
    current: Option<Signal<usize>>,
    default_step: usize,
    orientation: impl Into<StepperOrientation>,
    clickable: bool,
    class: Option<String>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = resolve_instance_id(id, "stepper");
    let orientation = orientation.into();
    let current =
        current.unwrap_or_else(|| use_signal(format!("{id}__step").as_str(), move || default_step));

    let root = with_child_owner(|| {
        provide_context(
            stepper_context(),
            StepperCtx {
                current,
                orientation,
                clickable,
            },
        );
        push_collect();
        let _ = body();
        let specs = pop_collect();
        let view_id = format!("{id}__stepper_view");
        let class = class.clone();
        let view = reactive_view(view_id, move || {
            let active = current.get();
            paint_stepper(
                &specs,
                current,
                active,
                orientation,
                clickable,
                class.as_deref(),
            )
        });
        let mut shell: Element = r#box().w("100%").child(view).into();
        shell.template_id = Some(format!("{id}__stepper"));
        shell
    });
    root
}

fn paint_stepper(
    specs: &[StepperItemSpec],
    current: Signal<usize>,
    active: usize,
    orientation: StepperOrientation,
    clickable: bool,
    class: Option<&str>,
) -> Element {
    let n = specs.len();
    let mut track = if orientation.is_vertical() {
        vstack().w("100%").gap("0")
    } else {
        hstack().w("100%").items("start").gap("0")
    };
    for (i, spec) in specs.iter().enumerate() {
        track = track.child(paint_item(
            spec,
            current,
            StepState::from_index(spec.index, active),
            orientation,
            clickable,
        ));
        if i + 1 < n {
            let filled = spec.index < active;
            track = track.child(paint_separator(filled, orientation));
        }
    }
    let mut el: Element = track.into();
    el.set_attr(ATTR_SLOT, SLOT_ROOT);
    el.set_attr(ATTR_ORIENTATION, orientation.as_str());
    el = el.accesskit_role("list");
    if let Some(c) = class {
        if !c.trim().is_empty() {
            el = with_class_list(el, c);
        }
    }
    el
}

fn paint_item(
    spec: &StepperItemSpec,
    current: Signal<usize>,
    state: StepState,
    orientation: StepperOrientation,
    clickable: bool,
) -> Element {
    let indicator = paint_indicator(spec.index, state, clickable && !spec.disabled, current);
    let title = paint_title(&spec.title, state, orientation);
    let desc = if spec.description.trim().is_empty() {
        None
    } else {
        Some(paint_description(&spec.description, orientation))
    };

    let mut copy = if orientation.is_vertical() {
        vstack().gap("0.125rem").min_w(0.0).flex_grow(1.0)
    } else {
        vstack().gap("0.125rem").items_center()
    };
    copy = copy.child(title);
    if let Some(d) = desc {
        copy = copy.child(d);
    }

    let cell = if orientation.is_vertical() {
        hstack()
            .w("100%")
            .items("start")
            .gap(tokens::SPACE_3)
            .child(indicator)
            .child(copy)
    } else {
        vstack()
            .items_center()
            .gap(tokens::SPACE_2)
            .flex_shrink(0.0)
            .child(indicator)
            .child(copy)
    };
    let mut el: Element = cell.into();
    el.set_attr(ATTR_SLOT, SLOT_ITEM);
    el.set_attr(ATTR_STATE, state.as_str());
    el.set_attr(ATTR_INDEX, spec.index.to_string());
    el = el.accesskit_role("listitem");
    if spec.disabled {
        el = el.opacity(0.5);
    }
    if !spec.class.trim().is_empty() {
        el = with_class_list(el, &spec.class);
    }
    el
}

fn paint_indicator(
    index: usize,
    state: StepState,
    clickable: bool,
    current: Signal<usize>,
) -> Element {
    let filled = matches!(state, StepState::Completed | StepState::Active);
    let mut disc: Element = r#box()
        .w(INDICATOR_SIZE)
        .h(INDICATOR_SIZE)
        .rounded_full()
        .items_center()
        .justify_center()
        .flex_shrink(0.0)
        .into();
    if filled {
        disc = disc.bg(parse_color("primary"));
        apply_semantic_border(&mut disc, None);
    } else {
        disc = disc.bg(parse_color("muted"));
        apply_semantic_border(
            &mut disc,
            Some(SemanticBorder {
                color: parse_color("border"),
                width: 1.0,
            }),
        );
    }

    let glyph_color = if filled {
        parse_text_color("primary-foreground")
    } else {
        parse_text_color("muted-foreground")
    };
    let child = if state == StepState::Completed {
        build_icon_text_node(lucide_glyph("check").unwrap_or_else(|| "✓".into()))
            .text_size(CHECK_PX)
            .text_color(glyph_color)
            .pointer_events_none()
    } else {
        build_text_node(
            format!("{}", index + 1),
            TextNodeConfig {
                size_px: INDICATOR_GLYPH_PX,
                weight: 500,
                color: glyph_color,
                ..Default::default()
            },
        )
        .pointer_events_none()
    };
    disc = disc.child(child);
    disc.set_attr(ATTR_SLOT, SLOT_INDICATOR);
    disc.set_attr(ATTR_STATE, state.as_str());

    if clickable {
        disc = disc.cursor("pointer").focusable(true);
        disc = interactive(disc)
            .on_click(move || {
                current.set(index);
            })
            .into();
    }
    disc
}

fn paint_title(text: &str, state: StepState, orientation: StepperOrientation) -> Element {
    let active = matches!(state, StepState::Active);
    let color = if matches!(state, StepState::Inactive) {
        parse_text_color("muted-foreground")
    } else {
        parse_text_color("foreground")
    };
    let align = if orientation.is_vertical() {
        crate::style::TextAlign::Start
    } else {
        crate::style::TextAlign::Center
    };
    // Short labels: `whitespace-nowrap` so a 1px-short box or 400→500 weight
    // bump cannot Word-wrap the last letter onto the description (same as
    // ShadCN Button / Tabs trigger). Leading-none keeps a one-line cap even
    // if wrap ever leaks through.
    let mut node = build_text_node(
        text,
        TextNodeConfig {
            size_px: TITLE_SIZE_PX,
            weight: if active {
                TITLE_WEIGHT_ACTIVE
            } else {
                TITLE_WEIGHT_IDLE
            },
            color,
            text_align: align,
            line_height_px: Some(TITLE_SIZE_PX),
            ..Default::default()
        },
    );
    node = node.whitespace_nowrap();
    let mut wrap: Element = if orientation.is_vertical() {
        vstack().w("100%").flex_shrink(0.0).child(node).into()
    } else {
        vstack().items_center().flex_shrink(0.0).child(node).into()
    };
    wrap.set_attr(ATTR_SLOT, SLOT_TITLE);
    wrap
}

fn paint_description(text: &str, orientation: StepperOrientation) -> Element {
    let align = if orientation.is_vertical() {
        crate::style::TextAlign::Start
    } else {
        crate::style::TextAlign::Center
    };
    let node = build_text_node(
        text,
        TextNodeConfig {
            size_px: DESC_SIZE_PX,
            weight: 400,
            color: parse_text_color("muted-foreground"),
            text_align: align,
            ..Default::default()
        },
    );
    let mut wrap: Element = if orientation.is_vertical() {
        vstack().w("100%").min_w(0.0).child(node).into()
    } else {
        vstack().items_center().child(node).into()
    };
    wrap.set_attr(ATTR_SLOT, SLOT_DESCRIPTION);
    wrap
}

fn paint_separator(filled: bool, orientation: StepperOrientation) -> Element {
    let color = if filled {
        parse_color("primary")
    } else {
        parse_color("border")
    };
    let mut el = if orientation.is_vertical() {
        let slot: Element = hstack()
            .w(INDICATOR_SIZE)
            .h(VERTICAL_CONNECTOR_H)
            .items_center()
            .justify_center()
            .flex_shrink(0.0)
            .into();
        let mut line: Element = r#box().w(CONNECTOR_W).h("100%").rounded_full().into();
        line = line.bg(color);
        slot.child(line)
    } else {
        let slot: Element = hstack()
            .h(INDICATOR_SIZE)
            .flex_grow(1.0)
            .min_w(tokens::SPACE_4)
            .items_center()
            .into();
        let mut line: Element = r#box().h(CONNECTOR_H).w("100%").rounded_full().into();
        line = line.bg(color);
        slot.child(line)
    };
    el.set_attr(ATTR_SLOT, SLOT_SEPARATOR);
    el.set_attr(ATTR_STATE, if filled { "completed" } else { "inactive" });
    el
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

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

pub struct StepperBuilder {
    id: String,
    steps: Vec<StepperStep>,
    current: Option<Signal<usize>>,
    default_step: usize,
    orientation: StepperOrientation,
    clickable: bool,
    class: String,
    child: Option<Box<dyn FnOnce() -> Element>>,
}

/// Builder entry (ADR-9 surface B). `id` is optional (WP-2.8).
pub fn stepper() -> StepperBuilder {
    StepperBuilder {
        id: String::new(),
        steps: Vec::new(),
        current: None,
        default_step: 0,
        orientation: StepperOrientation::Horizontal,
        clickable: true,
        class: String::new(),
        child: None,
    }
}

impl StepperBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn steps(mut self, steps: impl IntoIterator<Item = impl Into<StepperStep>>) -> Self {
        self.steps = steps.into_iter().map(|s| s.into()).collect();
        self
    }

    pub fn current_signal(mut self, s: Signal<usize>) -> Self {
        self.current = Some(s);
        self
    }

    pub fn default_step(mut self, i: usize) -> Self {
        self.default_step = i;
        self
    }

    pub fn orientation(mut self, o: impl Into<StepperOrientation>) -> Self {
        self.orientation = o.into();
        self
    }

    pub fn clickable(mut self, v: bool) -> Self {
        self.clickable = v;
        self
    }

    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.class = c.into();
        self
    }

    /// Compound surface: parts run inside [`stepper_scope`].
    pub fn child(mut self, body: impl FnOnce() -> Element + 'static) -> Self {
        self.child = Some(Box::new(body));
        self
    }
}

impl crate::core::tree::IntoElementList for StepperBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<StepperBuilder> for Element {
    fn from(builder: StepperBuilder) -> Self {
        let StepperBuilder {
            id,
            steps,
            current,
            default_step,
            orientation,
            clickable,
            class,
            child,
        } = builder;
        let class = if class.trim().is_empty() {
            None
        } else {
            Some(class)
        };
        stepper_scope(
            id,
            current,
            default_step,
            orientation,
            clickable,
            class,
            move || {
                if let Some(body) = child {
                    body()
                } else {
                    let mut frag = Element::new();
                    frag.set_attr("rui_fragment", "1");
                    let kids: Vec<Element> = steps
                        .into_iter()
                        .map(|s| {
                            StepperItem(StepperItemProps {
                                title: Some(s.title),
                                description: if s.description.is_empty() {
                                    None
                                } else {
                                    Some(s.description)
                                },
                                disabled: None,
                                class: None,
                                children: None,
                                base: None,
                            })
                        })
                        .collect();
                    frag.children(kids)
                }
            },
        )
    }
}

/// RSX fallback when the `ui!` special-case is not used. Compound children must
/// go through the macro → [`stepper_scope`] so items see context.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Stepper(
    id: String,
    current: Option<Signal<usize>>,
    current_signal: Option<Signal<usize>>,
    default_step: usize,
    orientation: Option<String>,
    clickable: Option<bool>,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    let mut b = stepper().id(id);
    if let Some(s) = current_signal.or(current) {
        b = b.current_signal(s);
    } else {
        b = b.default_step(default_step);
    }
    if let Some(o) = orientation {
        b = b.orientation(o);
    }
    if let Some(c) = clickable {
        b = b.clickable(c);
    }
    if let Some(c) = class {
        b = b.class(c);
    }
    if let Some(ch) = children {
        // Without the macro, children were built outside context; drop them.
        let _ = ch;
    }
    b.into()
}

// ---------------------------------------------------------------------------
// Parts
// ---------------------------------------------------------------------------

/// One step. Auto-indexed inside [`stepper_scope`]. Title / description come
/// from props or nested [`StepperTitle`] / [`StepperDescription`].
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn StepperItem(
    title: Option<String>,
    description: Option<String>,
    disabled: Option<bool>,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    let _ctx = require_stepper("StepperItem");
    let index = next_item_index();

    let mut title = title.unwrap_or_default();
    let mut description = description.unwrap_or_default();
    if let Some(ch) = children {
        for child in take_packed(ch) {
            match slot_of(&child) {
                Some(SLOT_TITLE) => {
                    let t = extract_text(&child);
                    if !t.is_empty() {
                        title = t;
                    }
                }
                Some(SLOT_DESCRIPTION) => {
                    let t = extract_text(&child);
                    if !t.is_empty() {
                        description = t;
                    }
                }
                Some(SLOT_INDICATOR) => {
                    // Default disc chrome is painted by the root; custom
                    // indicator children are not a product path yet.
                }
                _ => {
                    let t = extract_text(&child);
                    if title.is_empty() && !t.is_empty() {
                        title = t;
                    }
                }
            }
        }
    }

    register_item(StepperItemSpec {
        index,
        title,
        description,
        disabled: disabled.unwrap_or(false),
        class: class.unwrap_or_default(),
    });
    Element::new()
}

/// Disc slot. Empty → auto number / check. Present so RSX matches the part tree.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn StepperIndicator(children: Option<Element>) -> Element {
    let mut el: Element = r#box().into();
    el.set_attr(ATTR_SLOT, SLOT_INDICATOR);
    if let Some(ch) = children {
        el = el.children(take_packed(ch));
    }
    el
}

/// Step label (`text-sm`).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn StepperTitle(children: Option<Element>) -> Element {
    let mut el: Element = r#box().into();
    el.set_attr(ATTR_SLOT, SLOT_TITLE);
    if let Some(ch) = children {
        el = el.children(take_packed(ch));
    }
    el
}

/// Supporting copy (`text-xs` · `text-muted-foreground`).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn StepperDescription(children: Option<Element>) -> Element {
    let mut el: Element = r#box().into();
    el.set_attr(ATTR_SLOT, SLOT_DESCRIPTION);
    if let Some(ch) = children {
        el = el.children(take_packed(ch));
    }
    el
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::create_root;

    #[test]
    fn orientation_parse_falls_back_to_horizontal() {
        assert_eq!(
            StepperOrientation::parse("vertical"),
            StepperOrientation::Vertical
        );
        assert_eq!(
            StepperOrientation::parse("column"),
            StepperOrientation::Vertical
        );
        assert_eq!(
            StepperOrientation::parse("nope"),
            StepperOrientation::Horizontal
        );
    }

    #[test]
    fn step_from_str_and_pair() {
        let a = StepperStep::from("Account");
        assert_eq!(a.title, "Account");
        assert!(a.description.is_empty());
        let b = StepperStep::from(("Profile", "Details"));
        assert_eq!(b.title, "Profile");
        assert_eq!(b.description, "Details");
    }

    #[test]
    fn empty_id_does_not_write_empty_template() {
        let (_owner, el): (_, Element) = create_root(|| stepper().steps(["A", "B"]).into());
        let tid = el.template_id.as_deref().unwrap_or("");
        assert_ne!(tid, "");
    }

    #[test]
    fn builder_sugar_sets_instance_template() {
        let (_owner, el): (_, Element) = create_root(|| {
            stepper()
                .id("st-sugar")
                .steps(["Account", "Profile", "Confirm"])
                .default_step(1)
                .into()
        });
        assert_eq!(el.template_id.as_deref(), Some("st-sugar__stepper"));
    }

    #[test]
    fn dual_surface_compiles_shape() {
        let (_owner, _): (_, Element) = create_root(|| {
            let sig = use_signal("st-dual", || 0usize);
            let _b: Element = stepper()
                .current_signal(sig)
                .orientation("vertical")
                .steps([("A", "one"), ("B", "two")])
                .into();
            Stepper(StepperProps {
                id: "st-rsx".into(),
                current: Some(sig),
                current_signal: None,
                default_step: Some(0),
                orientation: Some("horizontal".into()),
                clickable: Some(true),
                class: None,
                children: None,
                base: None,
            })
        });
    }

    #[test]
    fn step_state_table() {
        assert_eq!(StepState::from_index(0, 1), StepState::Completed);
        assert_eq!(StepState::from_index(1, 1), StepState::Active);
        assert_eq!(StepState::from_index(2, 1), StepState::Inactive);
    }

    /// Clicking a step rebuilds the island; titles must stay one line so the last
    /// letter cannot drop onto the description (glyphon wrap when the box is 1px
    /// short of intrinsic, or when weight 400→500 grows inside a stale width).
    #[test]
    fn titles_stay_single_line_when_current_changes() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_effect, create_root};
        use crate::layout::layout_tree::LayoutTree;
        use crate::renderer::font_loader;
        use crate::text::{install_shared_font_system, intrinsic_width};
        use std::cell::RefCell;
        use std::rc::Rc;

        reset_registered_anchors();
        clear_test_reconcile_root();
        let mut fs = font_loader::create_font_system();
        font_loader::apply_default_families(fs.db_mut());
        install_shared_font_system(&mut fs);

        let tree: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&tree));

        let (handle, current) = create_root(|| {
            let current = use_signal("st-title-wrap", || 0usize);
            let tree_e = Rc::clone(&tree);
            let cur = current;
            create_effect(move || {
                let el: Element = stepper()
                    .id("st-title-wrap")
                    .current_signal(cur)
                    .steps([
                        ("Account", "Email and password"),
                        ("Profile", "Your details"),
                        ("Confirm", "Review and submit"),
                    ])
                    .into();
                *tree_e.borrow_mut() = el;
            });
            current
        });

        fn assert_titles(tree: &mut Element, label: &str) {
            let mut layout = LayoutTree::new();
            let result = layout.build_and_layout(tree, 360.0, 240.0, [-1.0, -1.0]);
            for title in ["Account", "Profile", "Confirm"] {
                let tn = result
                    .text_nodes
                    .iter()
                    .find(|n| n.text == title)
                    .unwrap_or_else(|| panic!("{label}: missing title {title:?}"));
                let prepared = tn.prepared.as_ref().expect("prepared");
                let intrinsic = intrinsic_width(prepared);
                let metrics = crate::text::layout(prepared, tn.width.max(1.0));
                assert_eq!(
                    metrics.line_count, 1,
                    "{label}: {title:?} wrapped into {} lines box_w={:.3} intrinsic={:.3}",
                    metrics.line_count, tn.width, intrinsic
                );
                assert!(
                    tn.width + 0.05 >= intrinsic,
                    "{label}: {title:?} box_w {:.3} < intrinsic {:.3}",
                    tn.width,
                    intrinsic
                );
                assert_eq!(
                    tn.style.white_space,
                    crate::style::WhiteSpace::Nowrap,
                    "{label}: {title:?} must nowrap so a short box cannot drop the last letter"
                );
                let size = tn.style.size_resolved();
                assert!(
                    tn.height < size * 1.85,
                    "{label}: {title:?} height {:.3} looks like a second line (size={size})",
                    tn.height
                );
            }
            let acc = result
                .text_nodes
                .iter()
                .find(|n| n.text == "Account")
                .expect("Account");
            let desc = result
                .text_nodes
                .iter()
                .find(|n| n.text == "Email and password")
                .expect("description");
            assert!(
                acc.y + acc.height <= desc.y + 1.0,
                "{label}: Account last-line mixes with description acc=({:.1},{:.1}+{:.1}) desc.y={:.1}",
                acc.x,
                acc.y,
                acc.height,
                desc.y
            );
        }

        {
            let mut t = tree.borrow_mut();
            assert_titles(&mut t, "current=0");
        }
        current.set(1);
        {
            let mut t = tree.borrow_mut();
            assert_titles(&mut t, "current=1");
        }
        current.set(2);
        {
            let mut t = tree.borrow_mut();
            assert_titles(&mut t, "current=2");
        }

        handle.dispose();
        clear_test_reconcile_root();
        reset_registered_anchors();
    }
}
