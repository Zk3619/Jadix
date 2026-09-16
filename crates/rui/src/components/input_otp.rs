//! # InputOTP
//!
//! 对位 ShadCN **InputOTP**（`bases/base/ui/input-otp.tsx`）：固定长度验证码格。
//! **不是** [`super::input::Input`] 的 `type` 变体——多槽、按格焦点/粘贴填充，交互与单行 Input 不同。
//!
//! ```text
//! InputOTP                       // role=group · 焦点 / 键盘 / 粘贴
//! ├── InputOTPGroup              // 连续槽行
//! │   └── InputOTPSlot           // index → 单字符 + fake caret
//! └── InputOTPSeparator          // 段间分隔（Minus）
//! ```
//!
//! ## 用法（双写法 · ADR-9）
//! ```ignore
//! // A · RSX / 标签
//! <InputOTP id="code" maxLength={6}>
//!     <InputOTPGroup>
//!         <InputOTPSlot index={0} />
//!         <InputOTPSlot index={1} />
//!         <InputOTPSlot index={2} />
//!     </InputOTPGroup>
//!     <InputOTPSeparator />
//!     <InputOTPGroup>
//!         <InputOTPSlot index={3} />
//!         <InputOTPSlot index={4} />
//!         <InputOTPSlot index={5} />
//!     </InputOTPGroup>
//! </InputOTP>
//!
//! // B · Builder（自动铺槽；`separator_at` 在索引处分段）
//! Element::from(input_otp("code").max_length(6).separator_at(3))
//! ```
//!
//! ## 注意
//! - **必填稳定 `id`**（多实例非受控）；空 → `__rui_input_otp_rt_N`。
//! - `focus_id` 为 `input-otp:{id}`；键盘落在根上。
//! - 默认仅接受 ASCII 数字；`pattern="alnum"` 可字母数字。
//! - `<InputOTP*>` 走通用 PascalCase → `*Props`；RSX `maxLength` 由宏映到 `max_length`。

use std::cell::Cell;
use std::sync::OnceLock;

use crate::components::base::SlotChrome;
use crate::core::reactivity::{
    create_context, create_effect, provide_context, use_context, use_signal, with_child_owner,
    Context, Signal,
};
use crate::core::tree::{Element, LayoutStatus};
use crate::impl_simple_slot_builder;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box};
use crate::renderer::caret_blink_visible;
use crate::style::{
    apply_focus_ring_shadow_color, focus_ring_border_color, focus_ring_color,
    focus_ring_destructive_color, parse_color, parse_text_color,
};
use crate::tokens;
use crate::utils::controlled::resolve_string_signal;
use crate::utils::instance_id::resolve_instance_id;
use crate::utils::lucide::lucide_glyph;
use crate::utils::take_fragment;
use crate::utils::text_node::{build_icon_text_node, build_text_node, TextNodeConfig};
use rui_macros::component;
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

const ATTR_SLOT: &str = "data-slot";
const SLOT_ROOT: &str = "input-otp";
const SLOT_GROUP: &str = "input-otp-group";
const SLOT_CELL: &str = "input-otp-slot";
const SLOT_SEPARATOR: &str = "input-otp-separator";

pub const INPUT_OTP_FOCUS_PREFIX: &str = "input-otp:";

const SLOT_SIZE: &str = "2.25rem";
/// Tailwind `rounded-md`（`first:rounded-l-md` / `last:rounded-r-md`）。
const SLOT_EDGE_RADIUS: f32 = 6.0;

thread_local! {
    /// Set during slot paint when an empty active OTP cell needs caret blink wake
    /// (same ~520ms `WaitUntil` path as Input `InputCaretDraw`).
    static OTP_CARET_BLINK_NEEDED: Cell<bool> = const { Cell::new(false) };
}

/// Whether this frame's OTP paint requested caret blink scheduling.
pub(crate) fn take_otp_caret_blink_needed() -> bool {
    OTP_CARET_BLINK_NEEDED.replace(false)
}

fn mark_otp_caret_blink_needed() {
    OTP_CARET_BLINK_NEEDED.set(true);
}

/// `focus_id` 是否为 InputOTP（`input-otp:{id}`）。
pub(crate) fn is_input_otp_focus(focus_id: &str) -> bool {
    focus_id.starts_with(INPUT_OTP_FOCUS_PREFIX)
}

/// Group 内边位置（供圆角 / 重叠描边复用）。
const ATTR_OTP_EDGE: &str = "rui_otp_edge";
const EDGE_START: &str = "start";
const EDGE_MIDDLE: &str = "middle";
const EDGE_END: &str = "end";
const EDGE_ONLY: &str = "only";

/// CSS `border-radius` 顺序 **[TL, TR, BR, BL]**。
fn otp_slot_radii(edge: &str) -> [f32; 4] {
    let r = SLOT_EDGE_RADIUS;
    match edge {
        EDGE_ONLY => [r, r, r, r],
        EDGE_START => [r, 0.0, 0.0, r], // TL + BL
        EDGE_END => [0.0, r, r, 0.0],   // TR + BR
        _ => [0.0; 4],
    }
}

fn otp_edge_name(is_first: bool, is_last: bool) -> &'static str {
    match (is_first, is_last) {
        (true, true) => EDGE_ONLY,
        (true, false) => EDGE_START,
        (false, true) => EDGE_END,
        (false, false) => EDGE_MIDDLE,
    }
}

/// shadcn `shadow-xs` ≈ `0 1px 2px 0 rgb(0 0 0 / 0.05)`。
fn slot_shadow_xs(el: &mut Element) {
    el.style.shadow_color = tokens::shadow_ink(0.05);
    el.style.shadow_offset = [0.0, 1.0];
    el.style.shadow_blur = 2.0;
    el.style.shadow_spread = 0.0;
    el.style.shadow_inset = false;
}

/// `dark:bg-input/30`（浅色下同 token 半透明，贴近 v4 registry）。
fn slot_bg() -> [f32; 4] {
    let mut c = parse_color("input");
    c[3] *= 0.3;
    c
}

fn caret_blink_on() -> bool {
    caret_blink_visible()
}

fn build_slot_glyph(ch: char) -> Element {
    build_text_node(
        ch.to_string(),
        TextNodeConfig {
            size_px: 14.0,
            weight: 500,
            color: parse_text_color("foreground"),
            text_align: crate::style::TextAlign::Center,
            ..Default::default()
        },
    )
}

fn build_slot_caret() -> Element {
    let caret_line: Element = r#box()
        .w("1px")
        .h("1rem")
        .bg(parse_color("foreground"))
        .into();
    r#box()
        .absolute(true)
        .inset(0.0)
        .items_center()
        .justify_center()
        .pointer_events("none")
        .child(caret_line)
        .into()
}

/// 对齐 ButtonGroup：均匀边框 + 正确圆角序 + 非首格 `paint_nudge` 叠掉共享边。
fn patch_slot_group_edges(el: &mut Element, is_first: bool, is_last: bool) {
    if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) != Some(SLOT_CELL) {
        return;
    }
    let edge = otp_edge_name(is_first, is_last);
    el.set_attr(ATTR_OTP_EDGE, edge);
    el.style.radii = otp_slot_radii(edge);
    // 非均匀边框 + 圆角会强制 SDF 画「整圈」边，观感变成独立圆角盒；改均匀边 + nudge。
    if el.style.border_width_sides.is_some() {
        let w = el
            .style
            .resolved_border_widths()
            .into_iter()
            .fold(0.0_f32, f32::max)
            .max(1.0);
        el.style.set_uniform_border_width(w);
    }
    if !is_first {
        el.set_attr(crate::components::button_group::ATTR_PAINT_NUDGE, "x");
    } else {
        el.attrs
            .remove(crate::components::button_group::ATTR_PAINT_NUDGE);
    }
}

fn sync_otp_slot_paint(
    el: &mut Element,
    index: usize,
    max_length: usize,
    value: Signal<String>,
    active: Signal<usize>,
    focus_id: &str,
    disabled: bool,
    invalid: bool,
) -> LayoutStatus {
    let text = value.peek();
    let active_i = clamp_active(active.peek(), max_length, text.chars().count());
    let focused = use_signal("__rui_button_focus_id", String::new).peek();
    let is_focused = focused == focus_id && !disabled;
    let is_active = is_focused && active_i == index;
    let ch = char_at(&text, index);

    let border_c = if invalid {
        parse_color("destructive")
    } else if is_active {
        focus_ring_border_color()
    } else {
        parse_color("input")
    };

    let mut status = LayoutStatus::Clean;
    if el.style.border_color != border_c {
        el.style.border_color = border_c;
        el.style.border_color_sides = None;
        status = status.merge(LayoutStatus::StyleChanged);
    }
    el.style.bg_color = slot_bg();

    let edge = el
        .attrs
        .get(ATTR_OTP_EDGE)
        .map(|s| s.as_str())
        .unwrap_or(EDGE_MIDDLE);
    let want_radii = otp_slot_radii(edge);
    if el.style.radii != want_radii {
        el.style.radii = want_radii;
        status = status.merge(LayoutStatus::StyleChanged);
    }

    if is_active {
        let ring = if invalid {
            focus_ring_destructive_color()
        } else {
            focus_ring_color()
        };
        apply_focus_ring_shadow_color(el, true, false, ring);
        if el.style.z_index != 10 {
            el.style.z_index = 10;
            status = status.merge(LayoutStatus::StyleChanged);
        }
    } else {
        slot_shadow_xs(el);
        if el.style.z_index != 0 {
            el.style.z_index = 0;
            status = status.merge(LayoutStatus::StyleChanged);
        }
    }

    let want_active = if is_active { "true" } else { "false" };
    if el.attrs.get("data-active").map(|s| s.as_str()) != Some(want_active) {
        el.set_attr("data-active", want_active);
        status = status.merge(LayoutStatus::StyleChanged);
    }

    let show_caret = is_active && ch.is_none() && caret_blink_on();
    if is_active && ch.is_none() {
        // Arm event-loop WaitUntil (~520ms) so caret toggles without continuous redraw.
        mark_otp_caret_blink_needed();
    }
    let mut kids: Vec<Element> = Vec::new();
    if let Some(c) = ch {
        kids.push(build_slot_glyph(c));
    }
    if show_caret {
        kids.push(build_slot_caret());
    }

    let kids_changed = el.children.len() != kids.len()
        || el
            .children
            .iter()
            .zip(kids.iter())
            .any(|(a, b)| a.text != b.text || a.children.len() != b.children.len());
    if kids_changed {
        el.children = kids;
        status = status.merge(LayoutStatus::StructureChanged);
    }

    status
}

/// 允许写入的字符集。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InputOtpPattern {
    #[default]
    Digits,
    Alphanumeric,
}

impl InputOtpPattern {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "alnum" | "alphanumeric" | "alpha" => Self::Alphanumeric,
            _ => Self::Digits,
        }
    }

    fn accept(self, c: char) -> Option<char> {
        match self {
            Self::Digits if c.is_ascii_digit() => Some(c),
            Self::Alphanumeric if c.is_ascii_alphanumeric() => Some(c.to_ascii_uppercase()),
            _ => None,
        }
    }
}

impl From<&str> for InputOtpPattern {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

pub fn input_otp_focus_id(id: &str) -> String {
    format!("{INPUT_OTP_FOCUS_PREFIX}{id}")
}

fn otp_active_key(id: &str) -> String {
    format!("{id}__otp_active")
}

// ── Context ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct InputOtpCtx {
    pub id: String,
    pub max_length: usize,
    pub value: Signal<String>,
    pub active: Signal<usize>,
    pub disabled: bool,
    pub invalid: bool,
    pub pattern: InputOtpPattern,
}

fn input_otp_context() -> Context<InputOtpCtx> {
    static CTX: OnceLock<Context<InputOtpCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<InputOtpCtx>)
}

pub fn use_input_otp() -> Option<InputOtpCtx> {
    use_context(input_otp_context())
}

fn require_input_otp(part: &str) -> InputOtpCtx {
    use_input_otp().unwrap_or_else(|| {
        panic!("{part} must be used within <InputOTP> / input_otp_scope");
    })
}

fn register_otp_redraw(instance_id: &str, value: Signal<String>, active: Signal<usize>) {
    let key = format!("{instance_id}__otp_redraw");
    use_signal(key.as_str(), move || {
        create_effect(move || {
            let _ = value.get();
            let _ = active.get();
            crate::app::runtime::request_redraw();
        });
    });
}

/// Peel single anonymous wrappers (OTP Slot often wrapped once), then shared `take_packed`.
fn take_otp_packed(mut el: Element) -> Vec<Element> {
    while el.attrs.get(ATTR_SLOT).is_none()
        && el.children.len() == 1
        && el.text.as_ref().is_none_or(|t| t.is_empty())
        && el.on_click.is_none()
        && el.focus_id.is_none()
        && el.attrs.get("rui_fragment").map(|s| s.as_str()) != Some("1")
    {
        el = el.children.pop().unwrap();
    }
    crate::utils::take_packed(el)
}

fn char_at(value: &str, index: usize) -> Option<char> {
    value.chars().nth(index)
}

fn clamp_active(active: usize, max_length: usize, value_len: usize) -> usize {
    if max_length == 0 {
        return 0;
    }
    active.min(value_len).min(max_length - 1)
}

fn insert_chars(
    value: &str,
    max_length: usize,
    start: usize,
    chars: impl IntoIterator<Item = char>,
    pattern: InputOtpPattern,
) -> (String, usize) {
    let mut buf: Vec<char> = value.chars().take(max_length).collect();
    let mut i = start.min(buf.len()).min(max_length);
    for c in chars {
        let Some(c) = pattern.accept(c) else {
            continue;
        };
        if i >= max_length {
            break;
        }
        if i < buf.len() {
            buf[i] = c;
        } else {
            buf.push(c);
        }
        i += 1;
    }
    let next_active = i.min(max_length.saturating_sub(1)).min(buf.len());
    (buf.into_iter().collect(), next_active)
}

fn handle_otp_key(ev: &KeyEvent, ctx: &InputOtpCtx) -> bool {
    if ctx.disabled {
        return false;
    }
    let max = ctx.max_length.max(1);
    let value = ctx.value.peek();
    let mut active = clamp_active(ctx.active.peek(), max, value.chars().count());

    // 多字符 Character（部分平台粘贴为一串）→ 当作粘贴填充。
    // Ctrl/Cmd+V：若 `text` 带入多字符也走同一路径。
    if let Key::Character(s) = &ev.logical_key {
        let accepted: Vec<char> = s.chars().filter_map(|c| ctx.pattern.accept(c)).collect();
        if accepted.is_empty() {
            // fall through to named keys only
        } else {
            let (next, next_i) = insert_chars(&value, max, active, accepted, ctx.pattern);
            ctx.value.set(next);
            ctx.active.set(next_i);
            crate::app::runtime::request_redraw();
            return true;
        }
    }
    if let Some(text) = ev.text.as_ref() {
        if text.chars().count() > 1 {
            let accepted: Vec<char> = text.chars().filter_map(|c| ctx.pattern.accept(c)).collect();
            if !accepted.is_empty() {
                let (next, next_i) = insert_chars(&value, max, active, accepted, ctx.pattern);
                ctx.value.set(next);
                ctx.active.set(next_i);
                crate::app::runtime::request_redraw();
                return true;
            }
        }
    }

    match &ev.logical_key {
        Key::Named(NamedKey::ArrowLeft) => {
            if active > 0 {
                ctx.active.set(active - 1);
                crate::app::runtime::request_redraw();
            }
            true
        }
        Key::Named(NamedKey::ArrowRight) => {
            if active + 1 < max {
                ctx.active
                    .set((active + 1).min(value.chars().count().min(max - 1)));
                crate::app::runtime::request_redraw();
            }
            true
        }
        Key::Named(NamedKey::Home) => {
            ctx.active.set(0);
            crate::app::runtime::request_redraw();
            true
        }
        Key::Named(NamedKey::End) => {
            let end = value.chars().count().min(max - 1);
            ctx.active.set(end);
            crate::app::runtime::request_redraw();
            true
        }
        Key::Named(NamedKey::Backspace) => {
            let len = value.chars().count();
            if active < len {
                let mut chars: Vec<char> = value.chars().collect();
                chars.remove(active);
                ctx.value.set(chars.into_iter().collect());
            } else if active > 0 && len > 0 {
                let mut chars: Vec<char> = value.chars().collect();
                chars.pop();
                active = active.saturating_sub(1);
                ctx.value.set(chars.into_iter().collect());
                ctx.active.set(active);
            }
            crate::app::runtime::request_redraw();
            true
        }
        Key::Named(NamedKey::Delete) => {
            let len = value.chars().count();
            if active < len {
                let mut chars: Vec<char> = value.chars().collect();
                chars.remove(active);
                ctx.value.set(chars.into_iter().collect());
                crate::app::runtime::request_redraw();
            }
            true
        }
        _ => false,
    }
}

// ── Scope ────────────────────────────────────────────────────────────────────

/// 提供 OTP 上下文并构建根壳（`ui!` / Builder 共用）。
pub fn input_otp_scope(
    id: impl Into<String>,
    max_length: usize,
    value: Option<Signal<String>>,
    disabled: bool,
    invalid: bool,
    pattern: InputOtpPattern,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = resolve_instance_id(id, "input_otp");
    let max_length = max_length.clamp(1, 32);
    let value = resolve_string_signal(value, &id, "value", String::new());
    // Truncate oversized controlled values.
    {
        let cur = value.peek();
        if cur.chars().count() > max_length {
            value.set(cur.chars().take(max_length).collect());
        }
    }
    let active = use_signal(&otp_active_key(&id), || 0usize);
    {
        let len = value.peek().chars().count();
        let a = clamp_active(active.peek(), max_length, len);
        if a != active.peek() {
            active.set(a);
        }
    }
    register_otp_redraw(&id, value, active);

    let fid = input_otp_focus_id(&id);
    let ctx = InputOtpCtx {
        id: id.clone(),
        max_length,
        value,
        active,
        disabled,
        invalid,
        pattern,
    };

    let mut root = with_child_owner(|| {
        provide_context(input_otp_context(), ctx.clone());
        let mut kids = take_fragment(body());
        if kids.is_empty() {
            kids = take_fragment(build_default_slots(max_length, None));
        }
        let mut row = hstack().items_center().gap("0.5rem").w("auto").min_w("0");
        for k in kids {
            row = row.child(k);
        }
        let el: Element = row.into();
        el
    });

    root.set_attr(ATTR_SLOT, SLOT_ROOT);
    root.template_id = Some(id.clone());
    if disabled {
        root = root.opacity(0.5).cursor_not_allowed();
    } else {
        let ctx_key = ctx.clone();
        root = interactive(root)
            .focus_id(fid.clone())
            .focusable(true)
            .cursor("text")
            .on_click(move || {
                use_signal("__rui_button_focus_id", String::new).set(fid.clone());
                use_signal("__rui_focus_visible", || false).set(true);
            })
            .on_key_down(move |ev| handle_otp_key(&ev, &ctx_key))
            .into();
    }
    root = root.accesskit_role("group");
    root
}

fn build_default_slots(max_length: usize, separator_at: Option<usize>) -> Element {
    let mut frag_kids = Vec::new();
    let mut group_kids = Vec::new();
    for i in 0..max_length {
        if separator_at == Some(i) && !group_kids.is_empty() {
            let mut g = Element::new();
            g.set_attr("rui_fragment", "1");
            frag_kids.push(InputOTPGroup(InputOTPGroupProps {
                children: Some(g.children(std::mem::take(&mut group_kids))),
                base: None,
                ..Default::default()
            }));
            frag_kids.push(InputOTPSeparator(InputOTPSeparatorProps {
                children: None,
                base: None,
                ..Default::default()
            }));
        }
        group_kids.push(InputOTPSlot(InputOTPSlotProps {
            index: Some(i as i32),
            children: None,
            base: None,
            ..Default::default()
        }));
    }
    if !group_kids.is_empty() {
        let mut g = Element::new();
        g.set_attr("rui_fragment", "1");
        frag_kids.push(InputOTPGroup(InputOTPGroupProps {
            children: Some(g.children(group_kids)),
            base: None,
            ..Default::default()
        }));
    }
    let mut frag = Element::new();
    frag.set_attr("rui_fragment", "1");
    frag.children(frag_kids)
}

// ── RSX parts ────────────────────────────────────────────────────────────────

/// 根：焦点壳 + 上下文；子节点为 Group / Separator / Slot。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn InputOTP(
    id: String,
    /// 槽位数（默认 6）。RSX：`maxLength` / `max_length`。
    max_length: Option<i32>,
    value: Option<Signal<String>>,
    disabled: bool,
    invalid: bool,
    /// `digits`（默认）| `alnum`
    pattern: Option<String>,
    children: Option<Element>,
) -> Element {
    let max = max_length.unwrap_or(6).max(1) as usize;
    let pattern = pattern
        .as_deref()
        .map(InputOtpPattern::parse)
        .unwrap_or_default();
    let body_children = children;
    input_otp_scope(
        id,
        max,
        value,
        disabled,
        invalid,
        pattern,
        move || match body_children {
            Some(c) => c,
            None => build_default_slots(max, None),
        },
    )
}

/// 连续槽容器（`gap-0`；子 Slot 拼成连体条）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn InputOTPGroup(children: Option<Element>) -> Element {
    let _ = require_input_otp("InputOTPGroup");
    let mut kids = children.map(take_otp_packed).unwrap_or_default();
    let n = kids.len();
    for (i, kid) in kids.iter_mut().enumerate() {
        patch_slot_group_edges(kid, i == 0, i + 1 == n);
    }
    let mut row = hstack().items_center().gap("0").flex_shrink(0.0);
    for k in kids {
        row = row.child(k);
    }
    let mut el: Element = row.into();
    el.set_attr(ATTR_SLOT, SLOT_GROUP);
    el
}

/// 单字符槽。`index` 从 0 起。
///
/// 默认均匀边框；[`InputOTPGroup`] 写入 `rui_otp_edge` / 圆角 / paint-nudge（对位 shadcn
/// `first:` / `last:` + ButtonGroup 连体描边）。内容与焦点态由 `reactive_sync` 每帧同步。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn InputOTPSlot(index: Option<i32>, children: Option<Element>) -> Element {
    let _ = children;
    let ctx = require_input_otp("InputOTPSlot");
    let index = index.unwrap_or(0).max(0) as usize;
    let fid = input_otp_focus_id(&ctx.id);
    let value_sig = ctx.value;
    let active_sig = ctx.active;
    let max_length = ctx.max_length;
    let disabled = ctx.disabled;
    let invalid = ctx.invalid;

    let value = value_sig.peek();
    let active = clamp_active(active_sig.peek(), max_length, value.chars().count());
    let focused = use_signal("__rui_button_focus_id", String::new).peek();
    let is_focused = focused == fid && !disabled;
    let is_active = is_focused && active == index;
    let ch = char_at(&value, index);

    let border_c = if invalid {
        parse_color("destructive")
    } else if is_active {
        focus_ring_border_color()
    } else {
        parse_color("input")
    };

    // 均匀 1px 边框（SDF 圆角要求）；Group 再叠 nudge 消掉共享双边。
    let mut cell: Element = r#box()
        .w(SLOT_SIZE)
        .h(SLOT_SIZE)
        .min_w(SLOT_SIZE)
        .items_center()
        .justify_center()
        .relative(true)
        .flex_shrink(0.0)
        .into();
    cell = cell.bg(slot_bg()).border(border_c, 1.0);
    cell.style.radii = [0.0; 4];

    if is_active {
        let ring = if invalid {
            focus_ring_destructive_color()
        } else {
            focus_ring_color()
        };
        apply_focus_ring_shadow_color(&mut cell, true, false, ring);
        cell.style.z_index = 10;
    } else {
        slot_shadow_xs(&mut cell);
    }

    if let Some(c) = ch {
        cell = cell.child(build_slot_glyph(c));
    }
    if is_active && ch.is_none() && caret_blink_on() {
        cell = cell.child(build_slot_caret());
    }

    cell.set_attr(ATTR_SLOT, SLOT_CELL);
    cell.set_attr("data-active", if is_active { "true" } else { "false" });
    cell.set_attr("data-index", index.to_string());
    // Group 尚未 patch 时先按 middle；patch 后会改写。
    cell.set_attr(ATTR_OTP_EDGE, EDGE_MIDDLE);

    let focus_key = fid.clone();
    if !disabled {
        let active_click = active_sig;
        cell = cell.on_click(move || {
            active_click.set(index);
            use_signal("__rui_button_focus_id", String::new).set(focus_key.clone());
            use_signal("__rui_focus_visible", || false).set(true);
            crate::app::runtime::request_redraw();
        });
    }

    let sync_fid = fid;
    cell = cell.reactive_sync(move |el, _| {
        sync_otp_slot_paint(
            el, index, max_length, value_sig, active_sig, &sync_fid, disabled, invalid,
        )
    });

    cell
}

/// 段间分隔（Minus 图标）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn InputOTPSeparator(children: Option<Element>) -> Element {
    let _ = children;
    let _ = require_input_otp("InputOTPSeparator");
    let glyph = lucide_glyph("minus").unwrap_or_else(|| "—".into());
    let icon = build_icon_text_node(glyph)
        .text_size(14.0)
        .text_color(parse_text_color("muted-foreground"));
    let mut el: Element = hstack()
        .items_center()
        .justify_center()
        .w("0.75rem")
        .flex_shrink(0.0)
        .child(icon)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_SEPARATOR);
    el = el.accesskit_role("separator").pointer_events("none");
    el
}

impl_simple_slot_builder!(
    InputOtpGroupBuilder,
    input_otp_group,
    InputOTPGroup,
    InputOTPGroupProps
);
impl_simple_slot_builder!(
    InputOtpSeparatorBuilder,
    input_otp_separator,
    InputOTPSeparator,
    InputOTPSeparatorProps
);

pub struct InputOtpSlotBuilder {
    index: i32,
    chrome: crate::components::base::SlotChrome,
}

pub fn input_otp_slot() -> InputOtpSlotBuilder {
    InputOtpSlotBuilder {
        index: 0,
        chrome: crate::components::base::SlotChrome::default(),
    }
}

impl InputOtpSlotBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn index(mut self, index: i32) -> Self {
        self.index = index.max(0);
        self
    }
}

impl From<InputOtpSlotBuilder> for Element {
    fn from(builder: InputOtpSlotBuilder) -> Self {
        let el = InputOTPSlot(InputOTPSlotProps {
            index: Some(builder.index),
            children: None,
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

impl crate::core::tree::IntoElementList for InputOtpSlotBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

// ── Builder ──────────────────────────────────────────────────────────────────

pub struct InputOtpBuilder {
    id: String,
    max_length: usize,
    value: Option<Signal<String>>,
    disabled: bool,
    invalid: bool,
    pattern: InputOtpPattern,
    chrome: SlotChrome,
    /// 在该索引前插入 Separator（如 `3` → `[0,1,2] | [3,4,5]`）。
    separator_at: Option<usize>,
    custom_body: Option<Box<dyn FnOnce() -> Element>>,
}

/// Builder 入口。
pub fn input_otp(id: impl Into<String>) -> InputOtpBuilder {
    InputOtpBuilder {
        id: id.into(),
        max_length: 6,
        value: None,
        disabled: false,
        invalid: false,
        pattern: InputOtpPattern::Digits,
        chrome: SlotChrome::default(),
        separator_at: None,
        custom_body: None,
    }
}

impl InputOtpBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn max_length(mut self, n: usize) -> Self {
        self.max_length = n.clamp(1, 32);
        self
    }
    pub fn value_signal(mut self, s: Signal<String>) -> Self {
        self.value = Some(s);
        self
    }
    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }
    pub fn invalid(mut self, v: bool) -> Self {
        self.invalid = v;
        self
    }
    pub fn pattern(mut self, p: impl Into<InputOtpPattern>) -> Self {
        self.pattern = p.into();
        self
    }
    pub fn separator_at(mut self, index: usize) -> Self {
        self.separator_at = Some(index);
        self
    }
    /// 自定义子树（Group / Slot / Separator）；覆盖默认铺槽。
    pub fn child(mut self, body: impl FnOnce() -> Element + 'static) -> Self {
        self.custom_body = Some(Box::new(body));
        self
    }
}

impl From<InputOtpBuilder> for Element {
    fn from(b: InputOtpBuilder) -> Self {
        let InputOtpBuilder {
            id,
            max_length,
            value,
            disabled,
            invalid,
            pattern,
            chrome,
            separator_at,
            custom_body,
        } = b;
        chrome.apply(input_otp_scope(
            id,
            max_length,
            value,
            disabled,
            invalid,
            pattern,
            move || {
                if let Some(body) = custom_body {
                    body()
                } else {
                    build_default_slots(max_length, separator_at)
                }
            },
        ))
    }
}

impl crate::core::tree::IntoElementList for InputOtpBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::create_root;

    #[test]
    fn pattern_accepts_digits() {
        assert_eq!(InputOtpPattern::Digits.accept('5'), Some('5'));
        assert_eq!(InputOtpPattern::Digits.accept('a'), None);
        assert_eq!(InputOtpPattern::Alphanumeric.accept('a'), Some('A'));
    }

    #[test]
    fn insert_chars_advances() {
        let (v, i) = insert_chars("", 6, 0, "123".chars(), InputOtpPattern::Digits);
        assert_eq!(v, "123");
        assert_eq!(i, 3);
    }

    #[test]
    fn dual_surface_and_slots() {
        create_root(|| {
            let built: Element = input_otp("t-otp-b").max_length(4).separator_at(2).into();
            assert_eq!(
                built.attrs.get(ATTR_SLOT).map(String::as_str),
                Some(SLOT_ROOT)
            );
            assert_eq!(built.focus_id.as_deref(), Some("input-otp:t-otp-b"));

            let rsx = InputOTP(InputOTPProps {
                id: "t-otp-t".into(),
                max_length: Some(6),
                ..Default::default()
            });
            assert_eq!(
                rsx.attrs.get(ATTR_SLOT).map(String::as_str),
                Some(SLOT_ROOT)
            );
        });
    }

    #[test]
    fn builder_slot_style() {
        create_root(|| {
            let el: Element = input_otp("t-otp-chrome")
                .max_length(1)
                .child(|| {
                    input_otp_group()
                        .child(input_otp_slot().index(0).style(|b| b.text_size(20.0)))
                        .into()
                })
                .into();
            fn find_size(el: &Element, want: f32) -> bool {
                el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
                    || (el.text_style.size_resolved() - want).abs() < 0.01
                    || el.children.iter().any(|c| find_size(c, want))
            }
            assert!(
                find_size(&el, 20.0),
                "otp slot .style(|b| b.text_size(20)) must land on the slot tree"
            );
        });
    }

    #[test]
    fn slot_radii_use_css_tl_tr_br_bl_order() {
        // [TL, TR, BR, BL]
        assert_eq!(
            otp_slot_radii(EDGE_START),
            [SLOT_EDGE_RADIUS, 0.0, 0.0, SLOT_EDGE_RADIUS]
        );
        assert_eq!(
            otp_slot_radii(EDGE_END),
            [0.0, SLOT_EDGE_RADIUS, SLOT_EDGE_RADIUS, 0.0]
        );
        assert_eq!(otp_slot_radii(EDGE_MIDDLE), [0.0; 4]);
    }

    #[test]
    fn group_patches_outer_radii_and_nudge() {
        create_root(|| {
            let el: Element = input_otp("t-otp-edge").max_length(3).into();
            // root → group → slots
            let group = el
                .children
                .iter()
                .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_GROUP))
                .expect("group");
            assert_eq!(group.children.len(), 3);
            let start = &group.children[0];
            let mid = &group.children[1];
            let end = &group.children[2];
            assert_eq!(
                start.attrs.get(ATTR_OTP_EDGE).map(String::as_str),
                Some(EDGE_START)
            );
            assert_eq!(
                mid.attrs.get(ATTR_OTP_EDGE).map(String::as_str),
                Some(EDGE_MIDDLE)
            );
            assert_eq!(
                end.attrs.get(ATTR_OTP_EDGE).map(String::as_str),
                Some(EDGE_END)
            );
            assert_eq!(start.style.radii, otp_slot_radii(EDGE_START));
            assert_eq!(mid.style.radii, otp_slot_radii(EDGE_MIDDLE));
            assert_eq!(end.style.radii, otp_slot_radii(EDGE_END));
            assert!(start
                .attrs
                .get(crate::components::button_group::ATTR_PAINT_NUDGE)
                .is_none());
            assert_eq!(
                mid.attrs
                    .get(crate::components::button_group::ATTR_PAINT_NUDGE)
                    .map(String::as_str),
                Some("x")
            );
            assert_eq!(
                end.attrs
                    .get(crate::components::button_group::ATTR_PAINT_NUDGE)
                    .map(String::as_str),
                Some("x")
            );
        });
    }

    #[test]
    fn focus_prefix() {
        assert_eq!(input_otp_focus_id("x"), "input-otp:x");
    }
}
