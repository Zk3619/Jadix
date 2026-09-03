//! # Questionnaire
//!
//! Aligns with ShadCN **base**
//! [`questionnaire.tsx`](../../../../Reference/ShadCN-UI/apps/v4/registry/bases/base/ui/questionnaire.tsx):
//! a one-question-at-a-time flow with single/multiple choice, freeform input, skip, and
//! validation.
//!
//! ```text
//! Questionnaire                    // Root form shell (flex column)
//! ├── QuestionnaireProgress        // "Question {n} of {total}"
//! ├── QuestionnaireItem            // One step (hidden when inactive)
//! │   ├── QuestionnaireTitle
//! │   ├── QuestionnaireDescription
//! │   ├── QuestionnaireChoices
//! │   │   ├── QuestionnaireChoice  // radio | checkbox + label + shortcut
//! │   │   └── QuestionnaireInput   // freeform alternative
//! │   └── QuestionnaireError
//! └── QuestionnaireActions
//!     ├── QuestionnairePrevious
//!     ├── QuestionnaireSkip
//!     ├── QuestionnaireNext
//!     └── QuestionnaireSubmit
//! ```
//!
//! `<Questionnaire>` / `<QuestionnaireItem>` in `ui!` expand through the macro to
//! [`questionnaire_scope`] / [`questionnaire_item_scope`]. Builder [`questionnaire`]
//! shares the same implementation (ADR-9 dual style).
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX (camelCase macro aliases: defaultItem / itemSignal / onSubmit)
//! ui! {
//!     <Questionnaire id="plan" defaultItem="direction" shortcuts="letters"
//!         on_submit={move |answers| { let _ = answers; }}>
//!         <QuestionnaireProgress />
//!         <QuestionnaireItem name="direction" required={true}>
//!             <QuestionnaireTitle>"What should the agent build next?"</QuestionnaireTitle>
//!             <QuestionnaireDescription>"Choose a direction or describe another task."</QuestionnaireDescription>
//!             <QuestionnaireChoices>
//!                 <QuestionnaireChoice value="tool-calls">
//!                     "Tool call timeline"
//!                     <QuestionnaireChoiceDescription>
//!                         "Show what the agent ran and what came back."
//!                     </QuestionnaireChoiceDescription>
//!                 </QuestionnaireChoice>
//!                 <QuestionnaireChoice value="approvals">"Approval checkpoints"</QuestionnaireChoice>
//!                 <QuestionnaireInput placeholder="Describe another feature…" />
//!             </QuestionnaireChoices>
//!             <QuestionnaireError />
//!         </QuestionnaireItem>
//!         <QuestionnaireActions>
//!             <QuestionnairePrevious />
//!             <QuestionnaireSkip />
//!             <QuestionnaireNext />
//!             <QuestionnaireSubmit>"Save plan"</QuestionnaireSubmit>
//!         </QuestionnaireActions>
//!     </Questionnaire>
//! }
//!
//! // B · Builder (snake_case only). `.class` is the root; `.parts` styles
//! // auto-assembled slots. `.child(|| …)` composes parts yourself (like Dialog).
//! questionnaire("plan")
//!     .class("gap-8")
//!     .parts(|p| p.choices("gap-2").actions("gap-3").title("text-lg"))
//!     .shortcuts("letters")
//!     .default_item("direction")
//!     .question("direction", "What should the agent build next?", |q| {
//!         q.required(true)
//!             .class("gap-4")
//!             .description("Choose a direction or describe another task.")
//!             .choice("tool-calls", "Tool call timeline")
//!             .input("Describe another feature…")
//!     })
//!     .on_submit(|answers| { let _ = answers; })
//! ```
//!
//! **Identity (WP-2.8)**: `id` optional; empty string → [`resolve_instance_id`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

use rui_macros::component;
use taffy::style::Display;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, NamedKey};

use crate::app::reactive::bind_prop;
use crate::components::button::button;
use crate::components::input::input;
use crate::components::kbd::kbd;
use crate::core::reactivity::{
    create_context, create_effect, provide_context, use_context, use_signal, with_child_owner,
    Context, Signal,
};
use crate::core::tree::{parse_color, parse_text_color, Element, Handler, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::style::{with_class_list, LineStyle};
use crate::utils::controlled::resolve_string_signal;
use crate::utils::lucide_glyph;
use crate::utils::resolve_instance_id;
use crate::utils::text_node::{build_icon_text_node, build_text_node, TextNodeConfig};
use crate::utils::{children_none_if_vacuous, take_fragment, take_packed};

const ATTR_SLOT: &str = "data-slot";
const SLOT_ROOT: &str = "questionnaire";
const SLOT_PROGRESS: &str = "questionnaire-progress";
const SLOT_ITEM: &str = "questionnaire-item";
const SLOT_TITLE: &str = "questionnaire-title";
const SLOT_DESCRIPTION: &str = "questionnaire-description";
const SLOT_CHOICES: &str = "questionnaire-choices";
const SLOT_CHOICE: &str = "questionnaire-choice";
const SLOT_CHOICE_DESC: &str = "questionnaire-choice-description";
const SLOT_CHOICE_INDICATOR: &str = "questionnaire-choice-indicator";
const SLOT_INPUT: &str = "questionnaire-input";
const SLOT_ERROR: &str = "questionnaire-error";
const SLOT_ACTIONS: &str = "questionnaire-actions";
const SLOT_PREVIOUS: &str = "questionnaire-previous";
const SLOT_SKIP: &str = "questionnaire-skip";
const SLOT_NEXT: &str = "questionnaire-next";
const SLOT_SUBMIT: &str = "questionnaire-submit";

const INDICATOR_PX: f32 = 16.0;
const DOT_PX: f32 = 10.0;
const CHECK_PX: f32 = 12.0;
const BORDER_W: f32 = 1.5;
const TITLE_SIZE_PX: f32 = 18.0;
const TITLE_WEIGHT: u16 = 600;
const BODY_SIZE_PX: f32 = 14.0;
const DEFAULT_ERROR: &str = "Please provide an answer.";

pub type QuestionnaireSubmitFn = Rc<RefCell<dyn FnMut(QuestionnaireAnswers)>>;
pub type QuestionnaireItemChangeFn = Rc<RefCell<dyn FnMut(String)>>;
pub type QuestionnaireStatusChangeFn = Rc<RefCell<dyn FnMut(QuestionnaireItemStatus)>>;

/// Wrap a submit callback for [`questionnaire_scope`] / the `ui!` macro.
pub fn questionnaire_submit_fn(
    f: impl FnMut(QuestionnaireAnswers) + 'static,
) -> QuestionnaireSubmitFn {
    Rc::new(RefCell::new(f))
}

/// Wrap an active-item observer for [`questionnaire_scope`].
pub fn questionnaire_item_change_fn(f: impl FnMut(String) + 'static) -> QuestionnaireItemChangeFn {
    Rc::new(RefCell::new(f))
}

/// Wrap an item status observer for [`questionnaire_item_scope`].
pub fn questionnaire_status_change_fn(
    f: impl FnMut(QuestionnaireItemStatus) + 'static,
) -> QuestionnaireStatusChangeFn {
    Rc::new(RefCell::new(f))
}

// ── Public types ──────────────────────────────────────────────────────────────

/// Answer shortcut mode (`Root.shortcuts`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuestionnaireShortcuts {
    #[default]
    None,
    Letters,
    Numbers,
}

impl QuestionnaireShortcuts {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "letters" | "letter" => Self::Letters,
            "numbers" | "number" => Self::Numbers,
            _ => Self::None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Letters => "letters",
            Self::Numbers => "numbers",
        }
    }
}

impl From<&str> for QuestionnaireShortcuts {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for QuestionnaireShortcuts {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

/// Per-item status (`unanswered` | `answered` | `skipped`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuestionnaireItemStatus {
    #[default]
    Unanswered,
    Answered,
    Skipped,
}

impl QuestionnaireItemStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unanswered => "unanswered",
            Self::Answered => "answered",
            Self::Skipped => "skipped",
        }
    }
}

/// One submitted item (`FormData.get` / `getAll` analogue).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QuestionnaireItemAnswer {
    pub name: String,
    pub status: QuestionnaireItemStatus,
    pub values: Vec<String>,
}

/// Snapshot passed to `on_submit` after every enabled item validates.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QuestionnaireAnswers {
    pub current: String,
    pub items: Vec<QuestionnaireItemAnswer>,
}

impl QuestionnaireAnswers {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.items
            .iter()
            .find(|i| i.name == name)
            .and_then(|i| i.values.first())
            .map(|s| s.as_str())
    }

    pub fn get_all(&self, name: &str) -> Vec<String> {
        self.items
            .iter()
            .find(|i| i.name == name)
            .map(|i| i.values.clone())
            .unwrap_or_default()
    }

    pub fn status_of(&self, name: &str) -> QuestionnaireItemStatus {
        self.items
            .iter()
            .find(|i| i.name == name)
            .map(|i| i.status)
            .unwrap_or(QuestionnaireItemStatus::Unanswered)
    }
}

/// Slot `class` tokens for the convenience Builder (`From<QuestionnaireBuilder>`).
///
/// Root `.class()` still styles the shell. Each field here is applied to the
/// matching auto-assembled part; a per-item / per-choice `class` is appended
/// and wins on conflicting utilities (same order as ShadCN `cn(defaults, className)`).
#[derive(Clone, Debug, Default)]
pub struct QuestionnairePartsClass {
    pub progress: Option<String>,
    pub item: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub choices: Option<String>,
    pub choice: Option<String>,
    pub choice_description: Option<String>,
    pub input: Option<String>,
    pub error: Option<String>,
    pub actions: Option<String>,
    pub previous: Option<String>,
    pub skip: Option<String>,
    pub next: Option<String>,
    pub submit: Option<String>,
}

impl QuestionnairePartsClass {
    pub fn progress(mut self, c: impl Into<String>) -> Self {
        self.progress = Some(c.into());
        self
    }
    pub fn item(mut self, c: impl Into<String>) -> Self {
        self.item = Some(c.into());
        self
    }
    pub fn title(mut self, c: impl Into<String>) -> Self {
        self.title = Some(c.into());
        self
    }
    pub fn description(mut self, c: impl Into<String>) -> Self {
        self.description = Some(c.into());
        self
    }
    pub fn choices(mut self, c: impl Into<String>) -> Self {
        self.choices = Some(c.into());
        self
    }
    pub fn choice(mut self, c: impl Into<String>) -> Self {
        self.choice = Some(c.into());
        self
    }
    pub fn choice_description(mut self, c: impl Into<String>) -> Self {
        self.choice_description = Some(c.into());
        self
    }
    pub fn input(mut self, c: impl Into<String>) -> Self {
        self.input = Some(c.into());
        self
    }
    pub fn error(mut self, c: impl Into<String>) -> Self {
        self.error = Some(c.into());
        self
    }
    pub fn actions(mut self, c: impl Into<String>) -> Self {
        self.actions = Some(c.into());
        self
    }
    pub fn previous(mut self, c: impl Into<String>) -> Self {
        self.previous = Some(c.into());
        self
    }
    pub fn skip(mut self, c: impl Into<String>) -> Self {
        self.skip = Some(c.into());
        self
    }
    pub fn next(mut self, c: impl Into<String>) -> Self {
        self.next = Some(c.into());
        self
    }
    pub fn submit(mut self, c: impl Into<String>) -> Self {
        self.submit = Some(c.into());
        self
    }
}

/// Optional collection passed to Root (`items`) for progress / order / shortcuts.
#[derive(Clone, Debug, Default)]
pub struct QuestionnaireItemDef {
    pub name: String,
    pub title: String,
    pub description: String,
    pub required: bool,
    pub multiple: bool,
    pub disabled: bool,
    pub choices: Vec<QuestionnaireChoiceDef>,
    pub input_placeholder: Option<String>,
    pub class: Option<String>,
    pub title_class: Option<String>,
    pub description_class: Option<String>,
    pub choices_class: Option<String>,
    pub choice_class: Option<String>,
    pub input_class: Option<String>,
    pub error_class: Option<String>,
}

/// Fixed-answer row in [`QuestionnaireItemDef`].
#[derive(Clone, Debug, Default)]
pub struct QuestionnaireChoiceDef {
    pub value: String,
    pub label: String,
    pub description: String,
    pub disabled: bool,
    pub class: Option<String>,
    pub description_class: Option<String>,
}

impl QuestionnaireItemDef {
    pub fn new(name: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            title: title.into(),
            ..Self::default()
        }
    }

    pub fn required(mut self, v: bool) -> Self {
        self.required = v;
        self
    }

    pub fn multiple(mut self, v: bool) -> Self {
        self.multiple = v;
        self
    }

    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }

    pub fn description(mut self, s: impl Into<String>) -> Self {
        self.description = s.into();
        self
    }

    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.class = Some(c.into());
        self
    }

    pub fn title_class(mut self, c: impl Into<String>) -> Self {
        self.title_class = Some(c.into());
        self
    }

    pub fn description_class(mut self, c: impl Into<String>) -> Self {
        self.description_class = Some(c.into());
        self
    }

    pub fn choices_class(mut self, c: impl Into<String>) -> Self {
        self.choices_class = Some(c.into());
        self
    }

    pub fn choice_class(mut self, c: impl Into<String>) -> Self {
        self.choice_class = Some(c.into());
        self
    }

    pub fn input_class(mut self, c: impl Into<String>) -> Self {
        self.input_class = Some(c.into());
        self
    }

    pub fn error_class(mut self, c: impl Into<String>) -> Self {
        self.error_class = Some(c.into());
        self
    }

    pub fn choice(mut self, value: impl Into<String>, label: impl Into<String>) -> Self {
        self.choices.push(QuestionnaireChoiceDef::new(value, label));
        self
    }

    pub fn choice_with(
        mut self,
        value: impl Into<String>,
        label: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        self.choices
            .push(QuestionnaireChoiceDef::new(value, label).description(description));
        self
    }

    pub fn add_choice(mut self, def: QuestionnaireChoiceDef) -> Self {
        self.choices.push(def);
        self
    }

    pub fn input(mut self, placeholder: impl Into<String>) -> Self {
        self.input_placeholder = Some(placeholder.into());
        self
    }
}

impl QuestionnaireChoiceDef {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            ..Self::default()
        }
    }

    pub fn description(mut self, s: impl Into<String>) -> Self {
        self.description = s.into();
        self
    }

    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }

    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.class = Some(c.into());
        self
    }

    pub fn description_class(mut self, c: impl Into<String>) -> Self {
        self.description_class = Some(c.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestionnaireItemMeta {
    pub name: String,
    pub required: bool,
    pub disabled: bool,
    pub multiple: bool,
}

// ── Context ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct QuestionnaireItemHandle {
    pub name: String,
    pub required: bool,
    pub multiple: bool,
    pub disabled: bool,
    pub selected: Signal<Vec<String>>,
    pub freeform: Signal<String>,
    pub status: Signal<QuestionnaireItemStatus>,
    pub invalid_shown: Signal<bool>,
    pub choices: Signal<Vec<String>>,
}

#[derive(Clone)]
pub struct QuestionnaireCtx {
    pub id: String,
    pub current: Signal<String>,
    pub shortcuts: QuestionnaireShortcuts,
    pub item_order: Signal<Vec<QuestionnaireItemMeta>>,
}

#[derive(Clone)]
pub struct QuestionnaireItemCtx {
    pub root_id: String,
    pub name: String,
    pub required: bool,
    pub multiple: bool,
    pub disabled: bool,
    pub selected: Signal<Vec<String>>,
    pub freeform: Signal<String>,
    pub status: Signal<QuestionnaireItemStatus>,
    pub invalid_shown: Signal<bool>,
    pub choices: Signal<Vec<String>>,
}

thread_local! {
    static Q_ITEMS: RefCell<HashMap<String, HashMap<String, QuestionnaireItemHandle>>> =
        RefCell::new(HashMap::new());
    static Q_SUBMIT: RefCell<HashMap<String, QuestionnaireSubmitFn>> =
        RefCell::new(HashMap::new());
    static Q_ITEM_CHANGE: RefCell<HashMap<String, QuestionnaireItemChangeFn>> =
        RefCell::new(HashMap::new());
    static Q_STATUS_CHANGE: RefCell<HashMap<String, QuestionnaireStatusChangeFn>> =
        RefCell::new(HashMap::new());
}

fn store_handle(root_id: &str, handle: QuestionnaireItemHandle) {
    Q_ITEMS.with(|m| {
        m.borrow_mut()
            .entry(root_id.to_string())
            .or_default()
            .insert(handle.name.clone(), handle);
    });
}

fn get_handle(root_id: &str, name: &str) -> Option<QuestionnaireItemHandle> {
    Q_ITEMS.with(|m| {
        m.borrow()
            .get(root_id)
            .and_then(|inner| inner.get(name).cloned())
    })
}

fn store_submit(root_id: &str, cb: Option<QuestionnaireSubmitFn>) {
    Q_SUBMIT.with(|m| {
        if let Some(cb) = cb {
            m.borrow_mut().insert(root_id.to_string(), cb);
        } else {
            m.borrow_mut().remove(root_id);
        }
    });
}

fn store_item_change(root_id: &str, cb: Option<QuestionnaireItemChangeFn>) {
    Q_ITEM_CHANGE.with(|m| {
        if let Some(cb) = cb {
            m.borrow_mut().insert(root_id.to_string(), cb);
        } else {
            m.borrow_mut().remove(root_id);
        }
    });
}

fn status_change_key(root_id: &str, name: &str) -> String {
    format!("{root_id}\0{name}")
}

fn store_status_change(root_id: &str, name: &str, cb: Option<QuestionnaireStatusChangeFn>) {
    Q_STATUS_CHANGE.with(|m| {
        let key = status_change_key(root_id, name);
        if let Some(cb) = cb {
            m.borrow_mut().insert(key, cb);
        } else {
            m.borrow_mut().remove(&key);
        }
    });
}

fn fire_status_change(root_id: &str, name: &str, status: QuestionnaireItemStatus) {
    Q_STATUS_CHANGE.with(|m| {
        if let Some(cb) = m.borrow().get(&status_change_key(root_id, name)) {
            cb.borrow_mut()(status);
        }
    });
}

fn questionnaire_context() -> Context<QuestionnaireCtx> {
    static CTX: OnceLock<Context<QuestionnaireCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<QuestionnaireCtx>)
}

fn questionnaire_item_context() -> Context<QuestionnaireItemCtx> {
    static CTX: OnceLock<Context<QuestionnaireItemCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<QuestionnaireItemCtx>)
}

pub fn use_questionnaire() -> Option<QuestionnaireCtx> {
    use_context(questionnaire_context())
}

pub fn use_questionnaire_item() -> Option<QuestionnaireItemCtx> {
    use_context(questionnaire_item_context())
}

fn require_questionnaire(part: &str) -> QuestionnaireCtx {
    use_questionnaire().unwrap_or_else(|| {
        panic!("{part} must be used within <Questionnaire> / questionnaire_scope");
    })
}

fn require_questionnaire_item(part: &str) -> QuestionnaireItemCtx {
    use_questionnaire_item().unwrap_or_else(|| {
        panic!("{part} must be used within <QuestionnaireItem>");
    })
}

// ── Pure helpers (unit-tested) ────────────────────────────────────────────────

pub fn assign_shortcut(mode: QuestionnaireShortcuts, enabled_index: usize) -> Option<String> {
    match mode {
        QuestionnaireShortcuts::None => None,
        QuestionnaireShortcuts::Letters if enabled_index < 26 => {
            Some(((b'A' + enabled_index as u8) as char).to_string())
        }
        QuestionnaireShortcuts::Numbers if enabled_index < 9 => {
            Some(((b'1' + enabled_index as u8) as char).to_string())
        }
        _ => None,
    }
}

pub fn enabled_item_names(order: &[QuestionnaireItemMeta]) -> Vec<String> {
    order
        .iter()
        .filter(|m| !m.disabled)
        .map(|m| m.name.clone())
        .collect()
}

/// Required items need an answer. Optional items need an answer **or** an explicit skip.
pub fn item_has_valid_answer(
    required: bool,
    status: QuestionnaireItemStatus,
    selected: &[String],
    freeform: &str,
) -> bool {
    let _ = required;
    if status == QuestionnaireItemStatus::Skipped {
        return true;
    }
    !selected.is_empty() || !freeform.trim().is_empty()
}

fn item_values(selected: &[String], freeform: &str) -> Vec<String> {
    let mut out = selected.to_vec();
    let t = freeform.trim();
    if !t.is_empty() && !out.iter().any(|v| v == t) {
        out.push(t.to_string());
    }
    out
}

fn caption_text(el: &Element) -> Option<String> {
    if let Some(t) = el.text.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        return Some(t.to_string());
    }
    el.children.iter().find_map(caption_text)
}

fn unpack_children(children: Option<Element>) -> Vec<Element> {
    match children_none_if_vacuous(children) {
        Some(el) => take_packed(el),
        None => Vec::new(),
    }
}

fn check_glyph() -> String {
    lucide_glyph("check").unwrap_or_else(|| "✓".into())
}

fn progress_position(order: &[QuestionnaireItemMeta], current: &str) -> (usize, usize) {
    let enabled = enabled_item_names(order);
    let total = enabled.len().max(1);
    let idx = enabled.iter().position(|n| n == current).unwrap_or(0);
    (idx + 1, total)
}

fn first_enabled_name(order: &[QuestionnaireItemMeta]) -> Option<String> {
    enabled_item_names(order).into_iter().next()
}

fn neighbor_name(order: &[QuestionnaireItemMeta], current: &str, delta: isize) -> Option<String> {
    let enabled = enabled_item_names(order);
    let idx = enabled.iter().position(|n| n == current)?;
    let next = idx as isize + delta;
    if next < 0 || next >= enabled.len() as isize {
        return None;
    }
    enabled.get(next as usize).cloned()
}

fn is_first_enabled(order: &[QuestionnaireItemMeta], current: &str) -> bool {
    first_enabled_name(order).as_deref() == Some(current)
}

fn is_last_enabled(order: &[QuestionnaireItemMeta], current: &str) -> bool {
    let enabled = enabled_item_names(order);
    enabled.last().map(|s| s.as_str()) == Some(current)
}

fn register_item_meta(order: Signal<Vec<QuestionnaireItemMeta>>, meta: QuestionnaireItemMeta) {
    let mut list = order.peek();
    if let Some(existing) = list.iter_mut().find(|m| m.name == meta.name) {
        *existing = meta;
    } else {
        list.push(meta);
    }
    order.set(list);
}

fn collect_answers(ctx: &QuestionnaireCtx) -> QuestionnaireAnswers {
    let order = ctx.item_order.peek();
    let mut items = Vec::new();
    for meta in order.iter().filter(|m| !m.disabled) {
        if let Some(h) = get_handle(&ctx.id, &meta.name) {
            let status = h.status.peek();
            let selected = h.selected.peek();
            let freeform = h.freeform.peek();
            items.push(QuestionnaireItemAnswer {
                name: meta.name.clone(),
                status,
                values: if status == QuestionnaireItemStatus::Skipped {
                    Vec::new()
                } else {
                    item_values(&selected, &freeform)
                },
            });
        }
    }
    QuestionnaireAnswers {
        current: ctx.current.peek(),
        items,
    }
}

fn set_item_status(item: &QuestionnaireItemCtx, status: QuestionnaireItemStatus) {
    if item.status.peek() != status {
        item.status.set(status);
        fire_status_change(&item.root_id, &item.name, status);
    }
}

fn refresh_answered_status(item: &QuestionnaireItemCtx) {
    let selected = item.selected.peek();
    let freeform = item.freeform.peek();
    let has = !selected.is_empty() || !freeform.trim().is_empty();
    if has {
        set_item_status(item, QuestionnaireItemStatus::Answered);
        item.invalid_shown.set(false);
    } else if item.status.peek() != QuestionnaireItemStatus::Skipped {
        set_item_status(item, QuestionnaireItemStatus::Unanswered);
    }
}

fn validate_item(handle: &QuestionnaireItemHandle) -> bool {
    let ok = item_has_valid_answer(
        handle.required,
        handle.status.peek(),
        &handle.selected.peek(),
        &handle.freeform.peek(),
    );
    handle.invalid_shown.set(!ok);
    ok
}

fn go_to(ctx: &QuestionnaireCtx, name: String) {
    if ctx.current.peek() == name {
        return;
    }
    ctx.current.set(name.clone());
    Q_ITEM_CHANGE.with(|m| {
        if let Some(cb) = m.borrow().get(&ctx.id) {
            cb.borrow_mut()(name);
        }
    });
}

fn try_advance(ctx: &QuestionnaireCtx) -> bool {
    let current = ctx.current.peek();
    let Some(handle) = get_handle(&ctx.id, &current) else {
        return false;
    };
    if !validate_item(&handle) {
        return false;
    }
    let order = ctx.item_order.peek();
    match neighbor_name(&order, &current, 1) {
        Some(next) => {
            go_to(ctx, next);
            true
        }
        None => false,
    }
}

fn try_submit(ctx: &QuestionnaireCtx) -> bool {
    let order = ctx.item_order.peek();
    let enabled = enabled_item_names(&order);
    let mut first_invalid = None::<String>;
    for name in &enabled {
        if let Some(handle) = get_handle(&ctx.id, name) {
            if !validate_item(&handle) && first_invalid.is_none() {
                first_invalid = Some(name.clone());
            }
        }
    }
    if let Some(name) = first_invalid {
        go_to(ctx, name);
        return false;
    }
    let answers = collect_answers(ctx);
    Q_SUBMIT.with(|m| {
        if let Some(cb) = m.borrow().get(&ctx.id) {
            cb.borrow_mut()(answers);
        }
    });
    true
}

fn skip_current(ctx: &QuestionnaireCtx) {
    let current = ctx.current.peek();
    let Some(handle) = get_handle(&ctx.id, &current) else {
        return;
    };
    if handle.required {
        return;
    }
    handle.status.set(QuestionnaireItemStatus::Skipped);
    handle.invalid_shown.set(false);
    fire_status_change(&ctx.id, &current, QuestionnaireItemStatus::Skipped);
    let order = ctx.item_order.peek();
    if let Some(next) = neighbor_name(&order, &current, 1) {
        go_to(ctx, next);
    } else {
        let _ = try_submit(ctx);
    }
}

fn go_previous(ctx: &QuestionnaireCtx) {
    let order = ctx.item_order.peek();
    let current = ctx.current.peek();
    if let Some(prev) = neighbor_name(&order, &current, -1) {
        go_to(ctx, prev);
    }
}

fn select_choice(item: &QuestionnaireItemCtx, value: &str) {
    if item.disabled {
        return;
    }
    if item.multiple {
        let mut sel = item.selected.peek();
        if let Some(i) = sel.iter().position(|v| v == value) {
            sel.remove(i);
        } else {
            sel.push(value.to_string());
        }
        item.selected.set(sel);
    } else {
        item.selected.set(vec![value.to_string()]);
        if !item.freeform.peek().is_empty() {
            item.freeform.set(String::new());
        }
    }
    refresh_answered_status(item);
}

fn focus_is_text_input() -> bool {
    let id = use_signal("__rui_button_focus_id", String::new).peek();
    id.starts_with(crate::utils::text_field::INPUT_FOCUS_PREFIX)
}

fn handle_root_key(ev: &KeyEvent, ctx: &QuestionnaireCtx) -> bool {
    if ev.state != ElementState::Pressed {
        return false;
    }
    if focus_is_text_input() {
        return false;
    }
    if matches!(ev.logical_key, Key::Named(NamedKey::Enter)) {
        let order = ctx.item_order.peek();
        let current = ctx.current.peek();
        if is_last_enabled(&order, &current) {
            try_submit(ctx);
        } else {
            let _ = try_advance(ctx);
        }
        return true;
    }
    let Key::Character(ch) = &ev.logical_key else {
        return false;
    };
    let raw = ch.trim();
    if raw.is_empty() {
        return false;
    }
    let needle = raw.to_ascii_uppercase();
    let current = ctx.current.peek();
    let Some(handle) = get_handle(&ctx.id, &current) else {
        return false;
    };
    if handle.disabled {
        return false;
    }
    let choice_values = handle.choices.peek();
    let mut enabled_idx = 0usize;
    let mut matched: Option<String> = None;
    for value in choice_values.iter() {
        let Some(key) = assign_shortcut(ctx.shortcuts, enabled_idx) else {
            enabled_idx += 1;
            continue;
        };
        enabled_idx += 1;
        if key == needle {
            matched = Some(value.clone());
            break;
        }
    }
    if let Some(value) = matched {
        let item = QuestionnaireItemCtx {
            root_id: ctx.id.clone(),
            name: handle.name.clone(),
            required: handle.required,
            multiple: handle.multiple,
            disabled: handle.disabled,
            selected: handle.selected,
            freeform: handle.freeform,
            status: handle.status,
            invalid_shown: handle.invalid_shown,
            choices: handle.choices,
        };
        select_choice(&item, &value);
        return true;
    }
    false
}

fn register_questionnaire_redraw(
    instance_id: &str,
    current: Signal<String>,
    order: Signal<Vec<QuestionnaireItemMeta>>,
) {
    let key = format!("{instance_id}__questionnaire_redraw");
    use_signal(key.as_str(), move || {
        create_effect(move || {
            let _ = current.get();
            let _ = order.get();
            crate::app::runtime::request_redraw();
        });
        ()
    });
}

fn sync_display(el: &mut Element, layout_tree: &mut LayoutTree, show: bool) -> LayoutStatus {
    let want = if show { Display::Flex } else { Display::None };
    if el.style.display == want {
        return LayoutStatus::Clean;
    }
    el.style.display = want;
    if el.layout_node.is_some() {
        layout_tree.update_taffy_node_style(el);
    }
    crate::app::damage::push_frame_paint_damage(el.layout_bounds);
    LayoutStatus::LayoutChanged
}

fn mark_slot(mut el: Element, slot: &str) -> Element {
    el.attrs.insert(ATTR_SLOT.to_string(), slot.to_string());
    el
}

fn apply_class(el: Element, class: Option<String>) -> Element {
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        with_class_list(el, &c)
    } else {
        el
    }
}

/// `parts` first, then the local slot class (later utilities win on conflict).
fn merge_slot_class(part: Option<&str>, local: Option<&str>) -> Option<String> {
    let part = part.map(str::trim).filter(|s| !s.is_empty());
    let local = local.map(str::trim).filter(|s| !s.is_empty());
    match (part, local) {
        (None, None) => None,
        (Some(a), None) => Some(a.to_string()),
        (None, Some(b)) => Some(b.to_string()),
        (Some(a), Some(b)) => Some(format!("{a} {b}")),
    }
}

fn text_leaf(content: &str, size_px: f32, weight: u16, color: &str) -> Element {
    build_text_node(
        content,
        TextNodeConfig {
            size_px,
            weight,
            color: parse_text_color(color),
            leading_scale: Some(1.35),
            ..Default::default()
        },
    )
}

fn children_or_text(
    children: Option<Element>,
    fallback: &str,
    size_px: f32,
    weight: u16,
    color: &str,
) -> Element {
    let kids = unpack_children(children);
    if kids.len() == 1 {
        if let Some(t) = caption_text(&kids[0]) {
            return text_leaf(&t, size_px, weight, color);
        }
        return kids.into_iter().next().unwrap();
    }
    if kids.is_empty() {
        return text_leaf(fallback, size_px, weight, color);
    }
    let mut wrap: Element = vstack()
        .w("100%")
        .min_w(0.0)
        .gap("0.125rem")
        .children(kids)
        .into();
    wrap = wrap.min_w(0.0);
    wrap
}

// ── Scope ─────────────────────────────────────────────────────────────────────

/// Provide Questionnaire context and build the root shell (`ui!` `<Questionnaire>` / Builder).
pub fn questionnaire_scope(
    id: impl Into<String>,
    item: Option<Signal<String>>,
    default_item: impl Into<String>,
    shortcuts: impl Into<QuestionnaireShortcuts>,
    seed_items: Vec<QuestionnaireItemDef>,
    class: Option<String>,
    on_submit: Option<QuestionnaireSubmitFn>,
    on_item_change: Option<QuestionnaireItemChangeFn>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = resolve_instance_id(id, "questionnaire");
    let default_item = default_item.into();
    let shortcuts = shortcuts.into();
    let current = resolve_string_signal(item, &id, "questionnaire", default_item);
    let seed_meta: Vec<QuestionnaireItemMeta> = seed_items
        .iter()
        .map(|d| QuestionnaireItemMeta {
            name: d.name.clone(),
            required: d.required,
            disabled: d.disabled,
            multiple: d.multiple,
        })
        .collect();
    let item_order = use_signal(format!("{id}__q_order").as_str(), move || seed_meta);
    register_questionnaire_redraw(&id, current, item_order);
    store_submit(&id, on_submit);
    store_item_change(&id, on_item_change);
    let ctx = QuestionnaireCtx {
        id: id.clone(),
        current,
        shortcuts,
        item_order,
    };
    let root = with_child_owner(|| {
        provide_context(questionnaire_context(), ctx.clone());
        let kids = take_fragment(body());
        let mut root: Element = vstack()
            .w("100%")
            .min_w(0.0)
            .gap("1.5rem")
            .children(kids)
            .into();
        root = apply_class(root, class);
        root = mark_slot(root, SLOT_ROOT);
        if shortcuts != QuestionnaireShortcuts::None {
            root.attrs
                .insert("data-shortcuts".to_string(), shortcuts.as_str().to_string());
        }
        root.template_id = Some(id.clone());
        root = root.accesskit_role("form");
        let key_ctx = ctx.clone();
        root = root.on_key_down(move |ev| handle_root_key(&ev, &key_ctx));
        let sync_current = ctx.current;
        let sync_order = ctx.item_order;
        root = root.reactive_sync(move |el, _layout| {
            let order = sync_order.peek();
            let cur = sync_current.peek();
            let (n, total) = progress_position(&order, &cur);
            el.attrs.insert("data-current".to_string(), n.to_string());
            el.attrs.insert("data-total".to_string(), total.to_string());
            if is_first_enabled(&order, &cur) {
                el.attrs
                    .insert("data-first".to_string(), "true".to_string());
            } else {
                el.attrs.remove("data-first");
            }
            if is_last_enabled(&order, &cur) {
                el.attrs.insert("data-last".to_string(), "true".to_string());
            } else {
                el.attrs.remove("data-last");
            }
            LayoutStatus::Clean
        });
        root
    });
    let order = ctx.item_order.peek();
    if ctx.current.peek().is_empty() {
        if let Some(first) = first_enabled_name(&order) {
            ctx.current.set(first);
        }
    }
    root
}

/// Provide Item context and build the step shell (`ui!` `<QuestionnaireItem>`).
pub fn questionnaire_item_scope(
    name: impl Into<String>,
    required: bool,
    multiple: bool,
    disabled: bool,
    invalid: bool,
    class: Option<String>,
    on_status_change: Option<QuestionnaireStatusChangeFn>,
    body: impl FnOnce() -> Element,
) -> Element {
    let root = require_questionnaire("QuestionnaireItem");
    let name = name.into();
    let selected = use_signal(
        format!("{}__{}__sel", root.id, name).as_str(),
        Vec::<String>::new,
    );
    let freeform = use_signal(format!("{}__{}__text", root.id, name).as_str(), String::new);
    let status = use_signal(format!("{}__{}__status", root.id, name).as_str(), || {
        QuestionnaireItemStatus::Unanswered
    });
    let invalid_shown = use_signal(
        format!("{}__{}__invalid", root.id, name).as_str(),
        move || invalid,
    );
    if invalid && !invalid_shown.peek() {
        invalid_shown.set(true);
    }
    let choices = use_signal(
        format!("{}__{}__choices", root.id, name).as_str(),
        Vec::<String>::new,
    );
    let handle = QuestionnaireItemHandle {
        name: name.clone(),
        required,
        multiple,
        disabled,
        selected,
        freeform,
        status,
        invalid_shown,
        choices,
    };
    store_handle(&root.id, handle.clone());
    store_status_change(&root.id, &name, on_status_change);
    register_item_meta(
        root.item_order,
        QuestionnaireItemMeta {
            name: name.clone(),
            required,
            disabled,
            multiple,
        },
    );
    let redraw_key = format!("{}__{}__item_redraw", root.id, name);
    use_signal(redraw_key.as_str(), move || {
        create_effect(move || {
            let _ = selected.get();
            let _ = freeform.get();
            let _ = status.get();
            let _ = invalid_shown.get();
            crate::app::runtime::request_redraw();
        });
        ()
    });
    let item_ctx = QuestionnaireItemCtx {
        root_id: root.id.clone(),
        name: name.clone(),
        required,
        multiple,
        disabled,
        selected,
        freeform,
        status,
        invalid_shown,
        choices,
    };
    with_child_owner(|| {
        provide_context(questionnaire_item_context(), item_ctx);
        let kids = take_fragment(body());
        let mut shell: Element = vstack()
            .w("100%")
            .min_w(0.0)
            .gap("0.75rem")
            .children(kids)
            .into();
        shell = apply_class(shell, class);
        shell = mark_slot(shell, SLOT_ITEM);
        shell.attrs.insert("data-name".to_string(), name.clone());
        if required {
            shell
                .attrs
                .insert("data-required".to_string(), "true".to_string());
        }
        if multiple {
            shell
                .attrs
                .insert("data-multiple".to_string(), "true".to_string());
        }
        if disabled {
            shell
                .attrs
                .insert("data-disabled".to_string(), "true".to_string());
        }
        shell.template_id = Some(format!("{}:item:{name}", root.id));
        shell = shell.accesskit_role("group");
        let current = root.current;
        let sync_name = name.clone();
        let sync_status = status;
        let sync_invalid = invalid_shown;
        let sync_disabled = disabled;
        shell = shell.reactive_sync(move |el, layout_tree| {
            let active = current.peek() == sync_name;
            if active {
                el.attrs
                    .insert("data-active".to_string(), "true".to_string());
            } else {
                el.attrs.remove("data-active");
            }
            el.attrs.insert(
                "data-status".to_string(),
                sync_status.peek().as_str().to_string(),
            );
            if sync_invalid.peek() {
                el.attrs
                    .insert("data-invalid".to_string(), "true".to_string());
            } else {
                el.attrs.remove("data-invalid");
            }
            let show = active && !sync_disabled;
            sync_display(el, layout_tree, show)
        });
        shell
    })
}

// ── RSX parts ─────────────────────────────────────────────────────────────────

/// Fallback RSX entry when the macro does not expand to [`questionnaire_scope`].
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Questionnaire(
    id: String,
    item: Option<Signal<String>>,
    default_item: String,
    shortcuts: Option<String>,
    items: Option<Vec<QuestionnaireItemDef>>,
    class: Option<String>,
    on_submit: Option<Handler<QuestionnaireAnswers>>,
    children: Option<Element>,
) -> Element {
    let shortcuts = shortcuts
        .as_deref()
        .map(QuestionnaireShortcuts::parse)
        .unwrap_or_default();
    let on_submit = on_submit.map(|mut h| questionnaire_submit_fn(move |a| h.call(a)));
    questionnaire_scope(
        id,
        item,
        default_item,
        shortcuts,
        items.unwrap_or_default(),
        class,
        on_submit,
        None,
        move || children.unwrap_or_else(Element::new),
    )
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn QuestionnaireItem(
    name: String,
    required: bool,
    multiple: bool,
    disabled: bool,
    invalid: bool,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    questionnaire_item_scope(
        name,
        required,
        multiple,
        disabled,
        invalid,
        class,
        None,
        move || children.unwrap_or_else(Element::new),
    )
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn QuestionnaireProgress(class: Option<String>, children: Option<Element>) -> Element {
    let ctx = require_questionnaire("QuestionnaireProgress");
    let custom = unpack_children(children);
    if !custom.is_empty() {
        let mut wrap: Element = hstack().min_w("14ch").children(custom).into();
        wrap = apply_class(mark_slot(wrap, SLOT_PROGRESS), class);
        return wrap;
    }
    let mut label = text_leaf("Question 1 of 1", BODY_SIZE_PX, 500, "muted-foreground");
    label = mark_slot(label, SLOT_PROGRESS);
    label = apply_class(label, class);
    let current = ctx.current;
    let order = ctx.item_order;
    let bind_key = format!("{}__progress", ctx.id);
    bind_prop(
        label,
        bind_key,
        move || {
            let (n, total) = progress_position(&order.get(), &current.get());
            format!("Question {n} of {total}")
        },
        |el, next| {
            if el.text.as_deref() == Some(next.as_str()) {
                LayoutStatus::Clean
            } else {
                el.text = Some(next);
                LayoutStatus::LayoutChanged
            }
        },
    )
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn QuestionnaireTitle(class: Option<String>, children: Option<Element>) -> Element {
    let _ = require_questionnaire_item("QuestionnaireTitle");
    let mut el = children_or_text(children, "", TITLE_SIZE_PX, TITLE_WEIGHT, "foreground");
    el = apply_class(mark_slot(el, SLOT_TITLE), class);
    el.w("100%").min_w(0.0)
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn QuestionnaireDescription(class: Option<String>, children: Option<Element>) -> Element {
    let _ = require_questionnaire_item("QuestionnaireDescription");
    let mut el = children_or_text(children, "", BODY_SIZE_PX, 400, "muted-foreground");
    el = apply_class(mark_slot(el, SLOT_DESCRIPTION), class);
    el.w("100%").min_w(0.0)
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn QuestionnaireChoices(class: Option<String>, children: Option<Element>) -> Element {
    let root = require_questionnaire("QuestionnaireChoices");
    let kids = unpack_children(children);
    let mut el: Element = vstack()
        .w("100%")
        .min_w(0.0)
        .gap("0.25rem")
        .children(kids)
        .into();
    el = apply_class(mark_slot(el, SLOT_CHOICES), class);
    if root.shortcuts != QuestionnaireShortcuts::None {
        el.attrs.insert(
            "data-shortcuts".to_string(),
            root.shortcuts.as_str().to_string(),
        );
    }
    el
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn QuestionnaireChoiceDescription(class: Option<String>, children: Option<Element>) -> Element {
    let mut el = children_or_text(children, "", 13.0, 400, "muted-foreground");
    el = apply_class(mark_slot(el, SLOT_CHOICE_DESC), class);
    el.w("100%").min_w(0.0)
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn QuestionnaireChoice(
    value: String,
    disabled: bool,
    default_checked: bool,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    let root = require_questionnaire("QuestionnaireChoice");
    let item = require_questionnaire_item("QuestionnaireChoice");
    let disabled = disabled || item.disabled;
    if default_checked && item.selected.peek().is_empty() {
        item.selected.set(vec![value.clone()]);
        refresh_answered_status(&item);
    }
    let enabled_idx = {
        let mut list = item.choices.peek();
        if !list.iter().any(|v| v == &value) {
            list.push(value.clone());
            item.choices.set(list.clone());
        }
        list.iter().position(|v| v == &value).unwrap_or(0)
    };
    let shortcut = if disabled {
        None
    } else {
        assign_shortcut(root.shortcuts, enabled_idx)
    };

    let kids = unpack_children(children);
    let mut label_kids = Vec::new();
    let mut desc = None::<Element>;
    for kid in kids {
        if kid.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_CHOICE_DESC) {
            desc = Some(kid);
        } else {
            label_kids.push(kid);
        }
    }
    let label_el = if label_kids.len() == 1 {
        if let Some(t) = caption_text(&label_kids[0]) {
            text_leaf(&t, BODY_SIZE_PX, 500, "foreground")
        } else {
            label_kids.remove(0)
        }
    } else if label_kids.is_empty() {
        text_leaf(&value, BODY_SIZE_PX, 500, "foreground")
    } else {
        vstack()
            .min_w(0.0)
            .gap("0.125rem")
            .children(label_kids)
            .into()
    };

    let mut content: Element = vstack().min_w(0.0).flex_grow(1.0).gap("0.125rem").into();
    content = content.child(label_el);
    if let Some(d) = desc {
        content = content.child(d);
    }

    let is_checkbox = item.multiple;
    let checked_now = item.selected.peek().iter().any(|v| v == &value);
    let mut row: Element = hstack()
        .w("100%")
        .min_w(0.0)
        .min_h("2.75rem")
        .gap("0.75rem")
        .items("start")
        .px("0.5rem")
        .py("0.5rem")
        .child(build_choice_indicator(is_checkbox, checked_now))
        .child(content)
        .into();
    if let Some(key) = shortcut.clone() {
        let mut hint: Element = kbd().text(key).into();
        hint = hint.flex_shrink(0.0);
        row = row.child(hint);
    }
    row = apply_class(
        mark_slot(row, SLOT_CHOICE),
        Some(format!(
            "rounded-md hover:bg-accent/50 {}",
            class.unwrap_or_default()
        )),
    );
    row.attrs.insert(
        "data-type".to_string(),
        if is_checkbox { "checkbox" } else { "radio" }.to_string(),
    );
    if let Some(key) = shortcut {
        row.attrs.insert("data-shortcut".to_string(), key);
    }
    if disabled {
        row.attrs
            .insert("data-disabled".to_string(), "true".to_string());
        row = row.opacity(0.5);
    }
    let focus_id = format!("{}:{}:{value}", root.id, item.name);
    let click_item = item.clone();
    let click_value = value.clone();
    let mut row: Element = interactive(row)
        .focus_id(focus_id.clone())
        .focusable(!disabled)
        .focus_ring(!disabled)
        .on_click(move || {
            if !disabled {
                use_signal("__rui_button_focus_id", String::new).set(focus_id.clone());
                select_choice(&click_item, &click_value);
            }
        })
        .into();
    if disabled {
        row = row.cursor_not_allowed();
    } else {
        row = row.cursor_kind(crate::core::tree::CursorKind::Pointer);
    }
    row.template_id = Some(format!("{}:choice:{value}", root.id));
    let sync_selected = item.selected;
    let sync_value = value.clone();
    let sync_invalid = item.invalid_shown;
    let sync_checkbox = is_checkbox;
    row = row.reactive_sync(move |el, layout_tree| {
        let on = sync_selected.peek().iter().any(|v| v == &sync_value);
        if on {
            el.attrs
                .insert("data-checked".to_string(), "true".to_string());
            el.attrs.remove("data-unchecked");
        } else {
            el.attrs.remove("data-checked");
            el.attrs
                .insert("data-unchecked".to_string(), "true".to_string());
        }
        if sync_invalid.peek() {
            el.attrs
                .insert("data-invalid".to_string(), "true".to_string());
        } else {
            el.attrs.remove("data-invalid");
        }
        sync_choice_indicator(el, layout_tree, sync_checkbox, on)
    });
    row
}

fn build_choice_indicator(checkbox: bool, on: bool) -> Element {
    if checkbox {
        let (bg, border) = if on {
            (parse_color("primary"), parse_color("primary"))
        } else {
            (parse_color("background"), parse_color("primary"))
        };
        let fg = if on {
            parse_text_color("primary-foreground")
        } else {
            [0, 0, 0, 0]
        };
        let icon = build_icon_text_node(check_glyph())
            .w(CHECK_PX)
            .h(CHECK_PX)
            .min_w(CHECK_PX)
            .min_h(CHECK_PX)
            .text_size(CHECK_PX)
            .text_color(fg)
            .text_center()
            .flex_shrink(0.0);
        let mut box_el: Element = hstack()
            .w(INDICATOR_PX)
            .h(INDICATOR_PX)
            .min_w(INDICATOR_PX)
            .min_h(INDICATOR_PX)
            .rounded("0.25rem")
            .items_center()
            .justify_center()
            .flex_shrink(0.0)
            .child(icon)
            .into();
        box_el = box_el.border(border, BORDER_W).bg(bg);
        box_el
            .attrs
            .insert(ATTR_SLOT.to_string(), SLOT_CHOICE_INDICATOR.to_string());
        box_el
    } else {
        let mut dot: Element = r#box()
            .w(DOT_PX)
            .h(DOT_PX)
            .min_w(DOT_PX)
            .min_h(DOT_PX)
            .rounded_full()
            .bg(if on {
                parse_color("primary")
            } else {
                [0.0, 0.0, 0.0, 0.0]
            })
            .flex_shrink(0.0)
            .into();
        dot.attrs.insert(
            ATTR_SLOT.to_string(),
            "questionnaire-choice-indicator-dot".to_string(),
        );
        let mut ring: Element = hstack()
            .w(INDICATOR_PX)
            .h(INDICATOR_PX)
            .min_w(INDICATOR_PX)
            .min_h(INDICATOR_PX)
            .rounded_full()
            .items_center()
            .justify_center()
            .flex_shrink(0.0)
            .child(dot)
            .into();
        ring = ring
            .border(parse_color("primary"), BORDER_W)
            .bg(parse_color("background"));
        ring.attrs
            .insert(ATTR_SLOT.to_string(), SLOT_CHOICE_INDICATOR.to_string());
        ring
    }
}

fn find_indicator_mut(el: &mut Element) -> Option<&mut Element> {
    if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_CHOICE_INDICATOR) {
        return Some(el);
    }
    el.children.iter_mut().find_map(find_indicator_mut)
}

fn sync_choice_indicator(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    checkbox: bool,
    on: bool,
) -> LayoutStatus {
    let Some(indicator) = find_indicator_mut(el) else {
        return LayoutStatus::Clean;
    };
    let mut changed = false;
    if checkbox {
        let (bg, border) = if on {
            (parse_color("primary"), parse_color("primary"))
        } else {
            (parse_color("background"), parse_color("primary"))
        };
        if indicator.style.bg_color != bg {
            indicator.style.bg_color = bg;
            changed = true;
        }
        if indicator.style.border_color != border {
            indicator.style.border_color = border;
            indicator.style.border_width = BORDER_W;
            indicator.style.border_style = LineStyle::Solid;
            changed = true;
        }
        let fg = if on {
            parse_text_color("primary-foreground")
        } else {
            [0, 0, 0, 0]
        };
        if let Some(icon) = indicator.children.first_mut() {
            if icon.text_style.color_resolved() != fg {
                icon.text_style.color = Some(fg);
                changed = true;
            }
        }
    } else if let Some(dot) = indicator.children.first_mut() {
        let want = if on {
            parse_color("primary")
        } else {
            [0.0, 0.0, 0.0, 0.0]
        };
        if dot.style.bg_color != want {
            dot.style.bg_color = want;
            changed = true;
        }
    }
    if changed {
        if indicator.layout_node.is_some() {
            layout_tree.update_taffy_node_style(indicator);
        }
        crate::app::damage::push_frame_paint_damage(el.layout_bounds);
        LayoutStatus::StyleChanged
    } else {
        LayoutStatus::Clean
    }
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn QuestionnaireInput(
    placeholder: String,
    disabled: bool,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    let root = require_questionnaire("QuestionnaireInput");
    let item = require_questionnaire_item("QuestionnaireInput");
    let _ = children;
    let disabled = disabled || item.disabled;
    let field_id = format!("{}__{}__q-input", root.id, item.name);
    let mut field: Element = input(&field_id)
        .placeholder(placeholder)
        .disabled(disabled)
        .value_signal(item.freeform)
        .h("2.75rem")
        .into();
    field = apply_class(mark_slot(field, SLOT_INPUT), class);
    let freeform = item.freeform;
    let selected = item.selected;
    let status = item.status;
    let invalid_shown = item.invalid_shown;
    let multiple = item.multiple;
    let watch_key = format!("{}__{}__input_watch", root.id, item.name);
    use_signal(watch_key.as_str(), move || {
        create_effect(move || {
            let text = freeform.get();
            if !text.trim().is_empty() {
                if !multiple && !selected.peek().is_empty() {
                    selected.set(Vec::new());
                }
                if status.peek() != QuestionnaireItemStatus::Answered {
                    status.set(QuestionnaireItemStatus::Answered);
                }
                if invalid_shown.peek() {
                    invalid_shown.set(false);
                }
            } else if selected.peek().is_empty()
                && status.peek() == QuestionnaireItemStatus::Answered
            {
                status.set(QuestionnaireItemStatus::Unanswered);
            }
        });
        ()
    });
    let sync_freeform = item.freeform;
    let sync_invalid = item.invalid_shown;
    field = field.reactive_sync(move |el, _layout| {
        let filled = !sync_freeform.peek().trim().is_empty();
        if filled {
            el.attrs
                .insert("data-filled".to_string(), "true".to_string());
            el.attrs.remove("data-empty");
        } else {
            el.attrs.remove("data-filled");
            el.attrs
                .insert("data-empty".to_string(), "true".to_string());
        }
        if sync_invalid.peek() {
            el.attrs
                .insert("data-invalid".to_string(), "true".to_string());
        } else {
            el.attrs.remove("data-invalid");
        }
        LayoutStatus::Clean
    });
    field
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn QuestionnaireError(class: Option<String>, children: Option<Element>) -> Element {
    let item = require_questionnaire_item("QuestionnaireError");
    let kids = unpack_children(children);
    let message = if kids.len() == 1 {
        caption_text(&kids[0]).unwrap_or_else(|| DEFAULT_ERROR.to_string())
    } else if kids.is_empty() {
        DEFAULT_ERROR.to_string()
    } else {
        caption_text(&kids[0]).unwrap_or_else(|| DEFAULT_ERROR.to_string())
    };
    let mut el = text_leaf(&message, 13.0, 400, "destructive");
    el = apply_class(mark_slot(el, SLOT_ERROR), class);
    let invalid = item.invalid_shown;
    el = el.reactive_sync(move |el, layout_tree| {
        let show = invalid.peek();
        if show {
            el.attrs
                .insert("data-invalid".to_string(), "true".to_string());
        } else {
            el.attrs.remove("data-invalid");
        }
        sync_display(el, layout_tree, show)
    });
    el
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn QuestionnaireActions(class: Option<String>, children: Option<Element>) -> Element {
    let _ = require_questionnaire("QuestionnaireActions");
    let kids = unpack_children(children);
    let mut el: Element = hstack()
        .w("100%")
        .min_w(0.0)
        .min_h("2.75rem")
        .gap("0.5rem")
        .items("center")
        .children(kids)
        .into();
    el = apply_class(mark_slot(el, SLOT_ACTIONS), class);
    el
}

fn action_label(children: Option<Element>, fallback: &str) -> String {
    unpack_children(children)
        .first()
        .and_then(caption_text)
        .unwrap_or_else(|| fallback.to_string())
}

fn spacer() -> Element {
    let mut el: Element = r#box().flex_grow(1.0).min_w(0.0).h("1px").into();
    el.attrs.insert(
        ATTR_SLOT.to_string(),
        "questionnaire-actions-spacer".to_string(),
    );
    el
}

fn wire_action_visibility(
    mut el: Element,
    slot: &str,
    class: Option<String>,
    visible: impl Fn() -> bool + 'static,
    status_of: impl Fn() -> QuestionnaireItemStatus + 'static,
) -> Element {
    el = apply_class(mark_slot(el, slot), class);
    el = el.reactive_sync(move |el, layout_tree| {
        let show = visible();
        if show {
            el.attrs
                .insert("data-visible".to_string(), "true".to_string());
            el.attrs.remove("data-hidden");
        } else {
            el.attrs.remove("data-visible");
            el.attrs
                .insert("data-hidden".to_string(), "true".to_string());
        }
        el.attrs
            .insert("data-status".to_string(), status_of().as_str().to_string());
        sync_display(el, layout_tree, show)
    });
    el
}

fn current_status(ctx: &QuestionnaireCtx) -> QuestionnaireItemStatus {
    let cur = ctx.current.peek();
    get_handle(&ctx.id, &cur)
        .map(|h| h.status.peek())
        .unwrap_or(QuestionnaireItemStatus::Unanswered)
}

fn current_required(ctx: &QuestionnaireCtx) -> bool {
    let cur = ctx.current.peek();
    get_handle(&ctx.id, &cur)
        .map(|h| h.required)
        .unwrap_or(false)
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn QuestionnairePrevious(class: Option<String>, children: Option<Element>) -> Element {
    let ctx = require_questionnaire("QuestionnairePrevious");
    let label = action_label(children, "Previous");
    let id = format!("{}__previous", ctx.id);
    let click = ctx.clone();
    let btn: Element = button(&id)
        .variant("outline")
        .text(label)
        .on_click(move || go_previous(&click))
        .into();
    let order = ctx.item_order;
    let current = ctx.current;
    let vis_ctx = ctx.clone();
    wire_action_visibility(
        btn,
        SLOT_PREVIOUS,
        class,
        move || !is_first_enabled(&order.peek(), &current.peek()),
        move || current_status(&vis_ctx),
    )
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn QuestionnaireSkip(class: Option<String>, children: Option<Element>) -> Element {
    let ctx = require_questionnaire("QuestionnaireSkip");
    let label = action_label(children, "Skip");
    let id = format!("{}__skip", ctx.id);
    let click = ctx.clone();
    let btn: Element = button(&id)
        .variant("outline")
        .text(label)
        .on_click(move || skip_current(&click))
        .into();
    let vis_ctx = ctx.clone();
    let vis_ctx2 = ctx.clone();
    wire_action_visibility(
        btn,
        SLOT_SKIP,
        class,
        move || !current_required(&vis_ctx),
        move || current_status(&vis_ctx2),
    )
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn QuestionnaireNext(class: Option<String>, children: Option<Element>) -> Element {
    let ctx = require_questionnaire("QuestionnaireNext");
    let label = action_label(children, "Next");
    let id = format!("{}__next", ctx.id);
    let click = ctx.clone();
    let mut btn: Element = button(&id)
        .text(label)
        .on_click(move || {
            let _ = try_advance(&click);
        })
        .into();
    btn.attrs
        .insert("data-shortcut".to_string(), "Enter".to_string());
    let order = ctx.item_order;
    let current = ctx.current;
    let vis_ctx = ctx.clone();
    wire_action_visibility(
        btn,
        SLOT_NEXT,
        class,
        move || !is_last_enabled(&order.peek(), &current.peek()),
        move || current_status(&vis_ctx),
    )
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn QuestionnaireSubmit(class: Option<String>, children: Option<Element>) -> Element {
    let ctx = require_questionnaire("QuestionnaireSubmit");
    let label = action_label(children, "Submit");
    let id = format!("{}__submit", ctx.id);
    let click = ctx.clone();
    let mut btn: Element = button(&id)
        .text(label)
        .on_click(move || {
            let _ = try_submit(&click);
        })
        .into();
    btn.attrs
        .insert("data-shortcut".to_string(), "Enter".to_string());
    let order = ctx.item_order;
    let current = ctx.current;
    let vis_ctx = ctx.clone();
    wire_action_visibility(
        btn,
        SLOT_SUBMIT,
        class,
        move || is_last_enabled(&order.peek(), &current.peek()),
        move || current_status(&vis_ctx),
    )
}

// ── Builder ───────────────────────────────────────────────────────────────────

pub struct QuestionnaireQuestionBuilder {
    def: QuestionnaireItemDef,
}

impl QuestionnaireQuestionBuilder {
    pub fn required(mut self, v: bool) -> Self {
        self.def.required = v;
        self
    }
    pub fn multiple(mut self, v: bool) -> Self {
        self.def.multiple = v;
        self
    }
    pub fn disabled(mut self, v: bool) -> Self {
        self.def.disabled = v;
        self
    }
    pub fn description(mut self, s: impl Into<String>) -> Self {
        self.def.description = s.into();
        self
    }
    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.def.class = Some(c.into());
        self
    }
    pub fn title_class(mut self, c: impl Into<String>) -> Self {
        self.def.title_class = Some(c.into());
        self
    }
    pub fn description_class(mut self, c: impl Into<String>) -> Self {
        self.def.description_class = Some(c.into());
        self
    }
    pub fn choices_class(mut self, c: impl Into<String>) -> Self {
        self.def.choices_class = Some(c.into());
        self
    }
    pub fn choice_class(mut self, c: impl Into<String>) -> Self {
        self.def.choice_class = Some(c.into());
        self
    }
    pub fn input_class(mut self, c: impl Into<String>) -> Self {
        self.def.input_class = Some(c.into());
        self
    }
    pub fn error_class(mut self, c: impl Into<String>) -> Self {
        self.def.error_class = Some(c.into());
        self
    }
    pub fn choice(mut self, value: impl Into<String>, label: impl Into<String>) -> Self {
        self.def = self.def.choice(value, label);
        self
    }
    pub fn choice_with(
        mut self,
        value: impl Into<String>,
        label: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        self.def = self.def.choice_with(value, label, description);
        self
    }
    pub fn add_choice(mut self, def: QuestionnaireChoiceDef) -> Self {
        self.def = self.def.add_choice(def);
        self
    }
    pub fn input(mut self, placeholder: impl Into<String>) -> Self {
        self.def = self.def.input(placeholder);
        self
    }
}

pub struct QuestionnaireBuilder {
    id: String,
    item: Option<Signal<String>>,
    default_item: String,
    shortcuts: QuestionnaireShortcuts,
    class: Option<String>,
    parts: QuestionnairePartsClass,
    on_submit: Option<QuestionnaireSubmitFn>,
    on_item_change: Option<QuestionnaireItemChangeFn>,
    questions: Vec<QuestionnaireItemDef>,
}

/// Builder entry (dual style B; RSX uses `<Questionnaire>`).
pub fn questionnaire(id: impl Into<String>) -> QuestionnaireBuilder {
    QuestionnaireBuilder {
        id: id.into(),
        item: None,
        default_item: String::new(),
        shortcuts: QuestionnaireShortcuts::None,
        class: None,
        parts: QuestionnairePartsClass::default(),
        on_submit: None,
        on_item_change: None,
        questions: Vec::new(),
    }
}

impl QuestionnaireBuilder {
    pub fn item_signal(mut self, s: Signal<String>) -> Self {
        self.item = Some(s);
        self
    }
    pub fn default_item(mut self, v: impl Into<String>) -> Self {
        self.default_item = v.into();
        self
    }
    pub fn shortcuts(mut self, v: impl Into<QuestionnaireShortcuts>) -> Self {
        self.shortcuts = v.into();
        self
    }
    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.class = Some(c.into());
        self
    }
    /// Default `class` tokens for auto-assembled slots (Progress, Title, Choices, …).
    pub fn parts(
        mut self,
        build: impl FnOnce(QuestionnairePartsClass) -> QuestionnairePartsClass,
    ) -> Self {
        self.parts = build(self.parts);
        self
    }
    pub fn on_submit(mut self, f: impl FnMut(QuestionnaireAnswers) + 'static) -> Self {
        self.on_submit = Some(questionnaire_submit_fn(f));
        self
    }
    pub fn on_item_change(mut self, f: impl FnMut(String) + 'static) -> Self {
        self.on_item_change = Some(questionnaire_item_change_fn(f));
        self
    }
    pub fn item(mut self, def: QuestionnaireItemDef) -> Self {
        self.questions.push(def);
        self
    }
    pub fn question(
        mut self,
        name: impl Into<String>,
        title: impl Into<String>,
        build: impl FnOnce(QuestionnaireQuestionBuilder) -> QuestionnaireQuestionBuilder,
    ) -> Self {
        let b = build(QuestionnaireQuestionBuilder {
            def: QuestionnaireItemDef::new(name, title),
        });
        self.questions.push(b.def);
        self
    }

    /// Compose Progress / Item / Actions yourself (Dialog-style). Slot `.parts()`
    /// does not apply here — put `class` on each part. `.question()` still seeds
    /// item order when present.
    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        questionnaire_scope(
            self.id,
            self.item,
            self.default_item,
            self.shortcuts,
            self.questions,
            self.class,
            self.on_submit,
            self.on_item_change,
            body,
        )
    }
}

fn render_def_item(def: QuestionnaireItemDef, parts: &QuestionnairePartsClass) -> Element {
    let required = def.required;
    let multiple = def.multiple;
    let disabled = def.disabled;
    let name = def.name.clone();
    let item_class = merge_slot_class(parts.item.as_deref(), def.class.as_deref());
    let title_class = merge_slot_class(parts.title.as_deref(), def.title_class.as_deref());
    let description_class = merge_slot_class(
        parts.description.as_deref(),
        def.description_class.as_deref(),
    );
    let choices_class = merge_slot_class(parts.choices.as_deref(), def.choices_class.as_deref());
    let input_class = merge_slot_class(parts.input.as_deref(), def.input_class.as_deref());
    let error_class = merge_slot_class(parts.error.as_deref(), def.error_class.as_deref());
    let choice_fallback = merge_slot_class(parts.choice.as_deref(), def.choice_class.as_deref());
    let choice_desc_part = parts.choice_description.clone();
    questionnaire_item_scope(
        name,
        required,
        multiple,
        disabled,
        false,
        item_class,
        None,
        move || {
            let mut kids = Vec::new();
            kids.push(QuestionnaireTitle(QuestionnaireTitleProps {
                class: title_class.clone(),
                children: Some(Element::new().text(def.title.clone())),
                base: None,
            }));
            if !def.description.trim().is_empty() {
                kids.push(QuestionnaireDescription(QuestionnaireDescriptionProps {
                    class: description_class.clone(),
                    children: Some(Element::new().text(def.description.clone())),
                    base: None,
                }));
            }
            let mut choice_els = Vec::new();
            for ch in def.choices {
                let mut choice_kids = vec![Element::new().text(ch.label)];
                if !ch.description.trim().is_empty() {
                    choice_kids.push(QuestionnaireChoiceDescription(
                        QuestionnaireChoiceDescriptionProps {
                            class: merge_slot_class(
                                choice_desc_part.as_deref(),
                                ch.description_class.as_deref(),
                            ),
                            children: Some(Element::new().text(ch.description)),
                            base: None,
                        },
                    ));
                }
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                choice_els.push(QuestionnaireChoice(QuestionnaireChoiceProps {
                    value: Some(ch.value),
                    disabled: Some(ch.disabled),
                    default_checked: None,
                    class: merge_slot_class(choice_fallback.as_deref(), ch.class.as_deref()),
                    children: Some(frag.children(choice_kids)),
                    base: None,
                }));
            }
            if let Some(ph) = def.input_placeholder {
                choice_els.push(QuestionnaireInput(QuestionnaireInputProps {
                    placeholder: Some(ph),
                    disabled: None,
                    class: input_class.clone(),
                    children: None,
                    base: None,
                }));
            }
            let mut choices_frag = Element::new();
            choices_frag.set_attr("rui_fragment", "1");
            kids.push(QuestionnaireChoices(QuestionnaireChoicesProps {
                class: choices_class.clone(),
                children: Some(choices_frag.children(choice_els)),
                base: None,
            }));
            kids.push(QuestionnaireError(QuestionnaireErrorProps {
                class: error_class.clone(),
                children: None,
                base: None,
            }));
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag.children(kids)
        },
    )
}

impl From<QuestionnaireBuilder> for Element {
    fn from(builder: QuestionnaireBuilder) -> Self {
        let QuestionnaireBuilder {
            id,
            item,
            default_item,
            shortcuts,
            class,
            parts,
            on_submit,
            on_item_change,
            questions,
        } = builder;
        let seed = questions.clone();
        let parts_body = parts.clone();
        questionnaire_scope(
            id,
            item,
            default_item,
            shortcuts,
            seed,
            class,
            on_submit,
            on_item_change,
            move || {
                let mut kids = Vec::new();
                kids.push(QuestionnaireProgress(QuestionnaireProgressProps {
                    class: parts_body.progress.clone(),
                    children: None,
                    base: None,
                }));
                for def in questions {
                    kids.push(render_def_item(def, &parts_body));
                }
                let mut actions = vec![
                    QuestionnairePrevious(QuestionnairePreviousProps {
                        class: parts_body.previous.clone(),
                        children: None,
                        base: None,
                    }),
                    spacer(),
                    QuestionnaireSkip(QuestionnaireSkipProps {
                        class: parts_body.skip.clone(),
                        children: None,
                        base: None,
                    }),
                    QuestionnaireNext(QuestionnaireNextProps {
                        class: parts_body.next.clone(),
                        children: None,
                        base: None,
                    }),
                    QuestionnaireSubmit(QuestionnaireSubmitProps {
                        class: parts_body.submit.clone(),
                        children: None,
                        base: None,
                    }),
                ];
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                kids.push(QuestionnaireActions(QuestionnaireActionsProps {
                    class: parts_body.actions.clone(),
                    children: Some(frag.children(std::mem::take(&mut actions))),
                    base: None,
                }));
                let mut out = Element::new();
                out.set_attr("rui_fragment", "1");
                out.children(kids)
            },
        )
    }
}

impl crate::core::tree::IntoElementList for QuestionnaireBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_letters_and_numbers() {
        assert_eq!(
            assign_shortcut(QuestionnaireShortcuts::Letters, 0).as_deref(),
            Some("A")
        );
        assert_eq!(
            assign_shortcut(QuestionnaireShortcuts::Letters, 25).as_deref(),
            Some("Z")
        );
        assert_eq!(assign_shortcut(QuestionnaireShortcuts::Letters, 26), None);
        assert_eq!(
            assign_shortcut(QuestionnaireShortcuts::Numbers, 0).as_deref(),
            Some("1")
        );
        assert_eq!(assign_shortcut(QuestionnaireShortcuts::Numbers, 9), None);
        assert_eq!(assign_shortcut(QuestionnaireShortcuts::None, 0), None);
    }

    #[test]
    fn enabled_names_skip_disabled() {
        let order = vec![
            QuestionnaireItemMeta {
                name: "a".into(),
                required: true,
                disabled: false,
                multiple: false,
            },
            QuestionnaireItemMeta {
                name: "b".into(),
                required: false,
                disabled: true,
                multiple: false,
            },
            QuestionnaireItemMeta {
                name: "c".into(),
                required: true,
                disabled: false,
                multiple: false,
            },
        ];
        assert_eq!(enabled_item_names(&order), vec!["a", "c"]);
        assert_eq!(progress_position(&order, "c"), (2, 2));
        assert!(is_first_enabled(&order, "a"));
        assert!(is_last_enabled(&order, "c"));
        assert_eq!(neighbor_name(&order, "a", 1).as_deref(), Some("c"));
        assert_eq!(neighbor_name(&order, "a", -1), None);
    }

    #[test]
    fn validation_requires_answer_or_skip() {
        assert!(!item_has_valid_answer(
            true,
            QuestionnaireItemStatus::Unanswered,
            &[],
            ""
        ));
        assert!(item_has_valid_answer(
            true,
            QuestionnaireItemStatus::Answered,
            &["x".into()],
            ""
        ));
        assert!(item_has_valid_answer(
            false,
            QuestionnaireItemStatus::Skipped,
            &[],
            ""
        ));
        assert!(!item_has_valid_answer(
            false,
            QuestionnaireItemStatus::Unanswered,
            &[],
            ""
        ));
        assert!(item_has_valid_answer(
            true,
            QuestionnaireItemStatus::Answered,
            &[],
            "typed"
        ));
    }

    #[test]
    fn answers_get_and_get_all() {
        let a = QuestionnaireAnswers {
            current: "signals".into(),
            items: vec![
                QuestionnaireItemAnswer {
                    name: "direction".into(),
                    status: QuestionnaireItemStatus::Answered,
                    values: vec!["tool-calls".into()],
                },
                QuestionnaireItemAnswer {
                    name: "signals".into(),
                    status: QuestionnaireItemStatus::Answered,
                    values: vec!["progress".into(), "risks".into()],
                },
            ],
        };
        assert_eq!(a.get("direction"), Some("tool-calls"));
        assert_eq!(a.get_all("signals"), vec!["progress", "risks"]);
        assert_eq!(a.get("missing"), None);
    }

    #[test]
    fn slot_class_joins_parts_then_local() {
        assert_eq!(merge_slot_class(None, None), None);
        assert_eq!(
            merge_slot_class(Some("gap-8"), None).as_deref(),
            Some("gap-8")
        );
        assert_eq!(merge_slot_class(None, Some("p-2")).as_deref(), Some("p-2"));
        assert_eq!(
            merge_slot_class(Some("gap-8"), Some("p-2")).as_deref(),
            Some("gap-8 p-2")
        );
        assert_eq!(merge_slot_class(Some("  "), Some("")), None);
    }
}
