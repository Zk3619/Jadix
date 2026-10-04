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
//! - `focus_id` 为 `input-otp:{id}`；键盘落在根上；根壳 `w(fit-content)` 贴合槽位行（避免列布局 stretch 成整行焦点盒）。
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

fn slot_shadow_is_xs(el: &Element) -> bool {
    el.style.shadow_color == tokens::shadow_ink(0.05)
        && el.style.shadow_offset == [0.0, 1.0]
        && el.style.shadow_blur == 2.0
        && el.style.shadow_spread == 0.0
        && !el.style.shadow_inset
}

/// `dark:bg-input/30`（浅色下同 token 半透明，贴近 v4 registry）。
fn slot_bg() -> [f32; 4] {
    let mut c = parse_color("input");
    c[3] *= 0.3;
    c
}

#[cfg(test)]
thread_local! {
    /// Tests pin the blink phase. Production always reads the shared caret clock.
    static OTP_CARET_FORCE: Cell<Option<bool>> = const { Cell::new(None) };
}

fn caret_blink_on() -> bool {
    #[cfg(test)]
    if let Some(forced) = OTP_CARET_FORCE.with(|c| c.get()) {
        return forced;
    }
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

fn build_slot_caret(opacity: f32) -> Element {
    let mut caret_line: Element = r#box()
        .w("1px")
        .h("1rem")
        .bg(parse_color("foreground"))
        .into();
    caret_line.style.opacity = opacity;
    let mut wrap: Element = r#box()
        .absolute(true)
        .inset(0.0)
        .items_center()
        .justify_center()
        .pointer_events("none")
        .child(caret_line)
        .into();
    wrap.style.opacity = opacity;
    wrap
}

/// SDF quad extent of the shadow currently on `el` (spread / blur / offset + border + vertex AA).
fn painted_chrome_outset(el: &Element) -> f32 {
    crate::app::draw_cull::sdf_outset_shadow_pad(
        el.style.shadow_blur,
        el.style.shadow_spread,
        el.style.shadow_offset,
        el.style.border_width,
    )
}

/// The 1px stroke, not the absolute inset wrapper (that wrapper is the whole cell).
fn caret_line(slot: &Element) -> Option<&Element> {
    slot.children
        .iter()
        .find(|c| c.style.position == taffy::style::Position::Absolute)
        .and_then(|wrap| wrap.children.first())
}

/// Caret on/off is a Node on the stroke itself. A Region keyed to the slot is
/// `draw_scissor_fill(clear_color)` — the gray dirty rectangle around the cell.
/// The line is a retained fill, not text-caret chrome, so blink OFF still
/// dest-resets this box (see `paint_dirty_is_caret_overlay`).
fn push_caret_blink_hole(slot: &Element) {
    let Some(line) = caret_line(slot) else {
        return;
    };
    let b = crate::core::tree::visual_paint_aabb(line, slot.paint_scroll_acc);
    // Unlaid, or the line stretched to the cell: a cell-sized hole is the seam/ring bug.
    if b[2] <= 0.5 || b[2] > 8.0 || b[3] <= 0.5 {
        return;
    }
    crate::app::damage::push_frame_paint_key(crate::app::damage::paint_element_key(line), b);
}

/// Slot box expanded by the SDF shadow the quad actually paints.
fn push_slot_ring_node(el: &Element, pad: f32) -> bool {
    let b = crate::core::tree::visual_paint_aabb(el, el.paint_scroll_acc);
    if b[2] <= 0.5 || b[3] <= 0.5 || pad <= 0.0 {
        return false;
    }
    crate::app::damage::push_frame_paint_key(
        crate::app::damage::paint_element_key(el),
        [
            b[0] - pad,
            b[1] - pad,
            b[2] + pad * 2.0,
            b[3] + pad * 2.0,
        ],
    );
    true
}

fn apply_caret_opacity(slot: &mut Element, opacity: f32) -> bool {
    let mut changed = false;
    for child in &mut slot.children {
        if child.style.position != taffy::style::Position::Absolute {
            continue;
        }
        if (child.style.opacity - opacity).abs() > 0.01 {
            child.style.opacity = opacity;
            changed = true;
        }
        for line in &mut child.children {
            if (line.style.opacity - opacity).abs() > 0.01 {
                line.style.opacity = opacity;
                changed = true;
            }
        }
    }
    changed
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
    let was_active = el.attrs.get("data-active").map(|s| s.as_str()) == Some("true");
    // Ring pixels live outside the layout box. Measure before the shadow swap so the
    // hole covers the ring that is leaving, not only the shadow that replaces it.
    let old_outset = painted_chrome_outset(el);

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
        if !crate::style::focus_ring_shadow_matches_color(el, true, ring) {
            apply_focus_ring_shadow_color(el, true, false, ring);
            status = status.merge(LayoutStatus::StyleChanged);
        }
        if el.style.z_index != 10 {
            el.style.z_index = 10;
            status = status.merge(LayoutStatus::StyleChanged);
        }
        // Sort above the joined edge. A real stacking band would own every
        // later slot and replay the ring off the caret.
        if el.attrs.get("rui_z_paint_only").map(|s| s.as_str()) != Some("1") {
            el.set_attr("rui_z_paint_only", "1");
            status = status.merge(LayoutStatus::StyleChanged);
        }
    } else {
        let shadow_changed = !slot_shadow_is_xs(el);
        slot_shadow_xs(el);
        if shadow_changed {
            status = status.merge(LayoutStatus::StyleChanged);
        }
        if el.style.z_index != 0 {
            el.style.z_index = 0;
            status = status.merge(LayoutStatus::StyleChanged);
        }
        if el.attrs.remove("rui_z_paint_only").is_some() {
            status = status.merge(LayoutStatus::StyleChanged);
        }
    }

    let want_active = if is_active { "true" } else { "false" };
    if el.attrs.get("data-active").map(|s| s.as_str()) != Some(want_active) {
        el.set_attr("data-active", want_active);
        status = status.merge(LayoutStatus::StyleChanged);
    }

    // The caret stays mounted on the empty active slot. Adding or removing it
    // is StructureChanged; inside a clip that records a Region of the whole
    // cell, and the clear cuts the shared border and the focus-ring SDF.
    // Blink only changes opacity and dirties the stroke.
    let want_caret = is_active && ch.is_none();
    let caret_opacity = if caret_blink_on() { 1.0 } else { 0.0 };
    if want_caret {
        mark_otp_caret_blink_needed();
    }
    let mut kids: Vec<Element> = Vec::new();
    if let Some(c) = ch {
        kids.push(build_slot_glyph(c));
    }
    if want_caret {
        kids.push(build_slot_caret(caret_opacity));
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

    let mut suppress_slot_dirty = false;
    if was_active != is_active || kids_changed {
        // The ring is this slot's outset SDF. Dest-reset that box and replay the
        // quad. A StructureChanged Region of the layout box window-clears and
        // page-band-replays, which paints the ring stroke off the cell.
        let pad = old_outset.max(painted_chrome_outset(el));
        if push_slot_ring_node(el, pad) {
            suppress_slot_dirty = true;
        }
    }
    if !kids_changed && want_caret && apply_caret_opacity(el, caret_opacity) {
        status = status.merge(LayoutStatus::StyleChanged);
        push_caret_blink_hole(el);
        suppress_slot_dirty = true;
    }
    if suppress_slot_dirty {
        // StyleChanged would dest-reset the whole cell. StructureChanged inside
        // a clip would window-clear it and page-band-replay the ring.
        el.set_attr("rui_halo_dirty", "1");
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
        // `w("auto")` + column `align-items: stretch` fills the parent cross axis
        // (gallery VStack / card). The focusable shell then owns a full-width AABB
        // while slots only paint on the left —「聚焦是整行」. `fit-content` hugs via
        // `apply_content_keyword_cross_axis_hug` (same as Item / attachment).
        let mut row = hstack()
            .items_center()
            .gap("0.5rem")
            .w("fit-content")
            .min_w("0");
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
            .focus_ring(false)
            .cursor("text")
            .on_click(move || {
                use_signal("__rui_button_focus_id", String::new).set(fid.clone());
                use_signal("__rui_focus_visible", || false).set(true);
                crate::app::runtime::request_redraw();
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
        cell.set_attr("rui_z_paint_only", "1");
    } else {
        slot_shadow_xs(&mut cell);
    }

    if let Some(c) = ch {
        cell = cell.child(build_slot_glyph(c));
    }
    if is_active && ch.is_none() {
        cell = cell.child(build_slot_caret(if caret_blink_on() { 1.0 } else { 0.0 }));
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
            el,
            index,
            max_length,
            value_sig,
            active_sig,
            &sync_fid,
            disabled,
            invalid,
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
    use crate::primitives::r#box::vstack;

    #[test]
    fn focus_move_restores_scrolled_slot_ring_and_neighbor() {
        create_root(|| {
            use crate::app::damage::{take_frame_paint_nodes, PaintDirtyKind};
            let value = Signal::new("12".to_owned());
            let active = Signal::new(0usize);
            let focus = use_signal("__rui_button_focus_id", String::new);
            focus.set("otp-ring-test".into());
            let mut slot = Element::new();
            slot.layout_bounds = [100.0, 700.0, 36.0, 36.0];
            slot.paint_scroll_acc = [0.0, 600.0];
            sync_otp_slot_paint(&mut slot, 0, 6, value, active, "otp-ring-test", false, false);
            let _ = take_frame_paint_nodes();
            active.set(1);
            sync_otp_slot_paint(&mut slot, 0, 6, value, active, "otp-ring-test", false, false);
            let dirty = take_frame_paint_nodes();
            assert!(
                dirty.iter().any(|d| d.kind == PaintDirtyKind::Node
                    && d.bounds[0] <= 96.0
                    && d.bounds[1] <= 96.0
                    && d.bounds[0] + d.bounds[2] >= 140.0
                    && d.bounds[1] + d.bounds[3] >= 140.0),
                "leaving ring must dest-reset the SDF outset, not a clear strip: {dirty:?}"
            );
            assert!(
                dirty.iter().all(|d| d.kind != PaintDirtyKind::Region),
                "focus-ring damage must not window-clear into a gray frame: {dirty:?}"
            );
        });
    }

    #[test]
    fn grouped_focus_repaint_stays_inside_the_active_slot() {
        create_root(|| {
            use crate::app::damage::{clear_frame_paint_nodes, take_frame_paint_nodes, PaintDirtyKind};
            let focus = use_signal("__rui_button_focus_id", String::new);
            focus.set(String::new());
            let otp: Element = input_otp("otp-group-hole").separator_at(3).into();
            let page = Element::new()
                .w(400.0)
                .h(200.0)
                .child(Element::new().w(400.0).h(48.0))
                .child(otp);
            let mut root = Element::new()
                .w(420.0)
                .h(80.0)
                .overflow_y("scroll")
                .child(page);
            root.focus_id = Some("scroll:otp-hole".into());
            let mut tree = crate::layout::layout_tree::LayoutTree::new();
            tree.build_and_layout(&mut root, 420.0, 80.0, [-1.0; 2]);
            tree.set_collect_skip_allowed(true);
            let _ = tree.build_and_layout_fast(&mut root, 420.0, 80.0, [-1.0; 2], true, true);
            let mut slots = Vec::new();
            fn walk(el: &Element, out: &mut Vec<(usize, [f32; 4])>) {
                if el.attrs.get("data-slot").map(|s| s.as_str()) == Some("input-otp-slot") {
                    let index = el
                        .attrs
                        .get("data-index")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(usize::MAX);
                    out.push((index, el.layout_bounds));
                }
                for c in &el.children {
                    walk(c, out);
                }
            }
            walk(&root, &mut slots);
            slots.sort_by_key(|(i, _)| *i);
            assert!(slots.len() >= 6, "grouped otp slots: {slots:?}");
            let active = slots[0].1;
            clear_frame_paint_nodes();
            // Scroll changes after the last collect. The focus hole must stay the
            // slot, not the union of the stale document box and the live visual box.
            root.scroll_offset = [0.0, 40.0];
            use_signal(
                &crate::core::scroll_viewport::scroll_offset_y_signal_key("otp-hole"),
                || 0.0_f32,
            )
            .set(40.0);
            focus.set(input_otp_focus_id("otp-group-hole"));
            let mut collect = crate::core::tree::ReactiveSyncCollect::default();
            crate::core::tree::sync_reactive_elements_subtree_ex(
                &mut root,
                &mut tree,
                &mut LayoutStatus::Clean,
                420.0,
                80.0,
                [0.0; 2],
                false,
                &mut collect,
            );
            let layout_dirty: std::collections::HashSet<_> = collect.layout_dirty.into_iter().collect();
            tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
            let dirty = take_frame_paint_nodes();
            let holes: Vec<_> = dirty.iter().map(|d| d.bounds).collect();
            let vis = |b: [f32; 4]| [b[0], b[1] - 40.0, b[2], b[3]];
            let covers = |hole: [f32; 4], slot: [f32; 4]| {
                let cx = slot[0] + slot[2] * 0.5;
                let cy = slot[1] + slot[3] * 0.5;
                cx >= hole[0] && cy >= hole[1] && cx <= hole[0] + hole[2] && cy <= hole[1] + hole[3]
            };
            assert!(
                dirty.iter().all(|d| d.kind != PaintDirtyKind::Region),
                "focus must dest-reset the ring, not window-clear a region: {dirty:?}"
            );
            assert!(
                holes.iter().all(|b| b[2] <= active[2] + 24.0 && b[3] <= active[3] + 24.0),
                "focus hole bigger than the active slot {active:?}: holes={holes:?} slots={slots:?}"
            );
            assert!(
                holes.iter().any(|hole| covers(*hole, vis(active))),
                "focus hole misses the visual active slot {:?}: holes={holes:?}",
                vis(active)
            );
            for (index, bounds) in slots.iter().skip(1) {
                assert!(
                    holes.iter().all(|hole| !covers(*hole, vis(*bounds))),
                    "focus hole covers slot {index} {bounds:?}: holes={holes:?}"
                );
            }
            let frame = tree.build_and_layout_fast(&mut root, 420.0, 80.0, [-1.0; 2], false, true);
            let mut after = Vec::new();
            walk(&root, &mut after);
            after.sort_by_key(|(i, _)| *i);
            let covers_slot = |inst: &crate::renderer::InstanceInput, slot: [f32; 4]| {
                let x = inst.rect_pos[0];
                let y = inst.rect_pos[1];
                let w = inst.rect_size[0];
                let h = inst.rect_size[1];
                (x - slot[0]).abs() <= 2.0
                    && (y - slot[1]).abs() <= 2.0
                    && (w - slot[2]).abs() <= 4.0
                    && (h - slot[3]).abs() <= 4.0
                    && inst.border_width > 0.5
            };
            for (index, bounds) in &after {
                let want = vis(*bounds);
                let on_screen = want[0] < 420.0
                    && want[1] < 80.0
                    && want[0] + want[2] > 0.0
                    && want[1] + want[3] > 0.0;
                if !on_screen {
                    continue;
                }
                assert!(
                    frame.instances.iter().any(|inst| covers_slot(inst, want)),
                    "slot {index} lost its border quad after focus: visual={want:?}"
                );
            }
            let fat: Vec<_> = frame
                .instances
                .iter()
                .filter(|inst| {
                    let x = inst.rect_pos[0];
                    let y = inst.rect_pos[1];
                    let w = inst.rect_size[0];
                    let h = inst.rect_size[1];
                    let slot = vis(active);
                    let overlaps = x < slot[0] + slot[2] + 8.0
                        && y < slot[1] + slot[3] + 8.0
                        && x + w > slot[0] - 8.0
                        && y + h > slot[1] - 8.0;
                    overlaps && (w > active[2] + 16.0 || h > active[3] + 16.0)
                })
                .map(|inst| (inst.rect_pos, inst.rect_size, inst.color, inst.shadow_blur, inst.shadow_spread))
                .collect();
            assert!(
                fat.is_empty(),
                "focused slot painted a quad outside one cell: {fat:?} active={active:?}"
            );
            let slot_now = after[0].1;
            assert!(
                (slot_now[0] - active[0]).abs() < 1.0 && (slot_now[1] - active[1]).abs() < 1.0,
                "active slot moved when the caret was inserted: before={active:?} after={slot_now:?}"
            );
            let mut abs_boxes = Vec::new();
            fn walk_abs(el: &Element, in_slot: bool, out: &mut Vec<[f32; 4]>) {
                let in_slot = in_slot
                    || el.attrs.get("data-slot").map(|s| s.as_str()) == Some("input-otp-slot");
                if in_slot && el.style.position == taffy::style::Position::Absolute {
                    out.push(el.layout_bounds);
                }
                for c in &el.children {
                    walk_abs(c, in_slot, out);
                }
            }
            walk_abs(&root, false, &mut abs_boxes);
            for b in &abs_boxes {
                let inside = b[0] >= slot_now[0] - 1.0
                    && b[1] >= slot_now[1] - 1.0
                    && b[0] + b[2] <= slot_now[0] + slot_now[2] + 1.0
                    && b[1] + b[3] <= slot_now[1] + slot_now[3] + 1.0;
                assert!(inside, "caret left the active slot: caret={b:?} slot={slot_now:?}");
            }
            let scissor = holes.iter().copied().fold([0.0; 4], |acc, r| {
                if acc[2] <= 0.0 { r } else { crate::app::damage::union_rects(acc, r) }
            });
            let border = frame
                .instances
                .iter()
                .find(|inst| covers_slot(inst, vis(slot_now)))
                .expect("active border");
            assert!(
                crate::app::draw_cull::instance_hits_damage(border, scissor),
                "clear hole misses the active slot, so the slot stays erased\n scissor={scissor:?} slot={slot_now:?} inst=({:?}, {:?})",
                border.rect_pos,
                border.rect_size
            );
            let idx = frame
                .instances
                .iter()
                .position(|inst| covers_slot(inst, vis(slot_now)))
                .unwrap();
            let bands = frame.overlay_split.page_stack_band_ranges();
            let in_page_band = bands.iter().any(|b| idx >= b.instance.0 && idx < b.instance.1)
                || (bands.is_empty() && idx < frame.overlay_split.instance_start);
            let in_paint_op = bands.iter().any(|b| {
                frame.paint_ops[b.paint_ops.0..b.paint_ops.1]
                    .iter()
                    .any(|op| match op {
                        crate::layout::layout_tree::PaintOp::Rects { start, count } => {
                            idx >= *start as usize && idx < (*start as usize + *count as usize)
                        }
                        crate::layout::layout_tree::PaintOp::Textured { .. } => false,
                    })
            });
            assert!(
                in_page_band && (in_paint_op || bands.iter().all(|b| b.paint_ops.0 == b.paint_ops.1)),
                "active slot quad is outside page-band replay\n idx={idx} in_band={in_page_band} in_op={in_paint_op} overlay_start={} bands={:?}",
                frame.overlay_split.instance_start,
                bands
                    .iter()
                    .map(|b| (b.instance, b.paint_ops))
                    .collect::<Vec<_>>()
            );
            let owner = frame
                .node_paints
                .iter()
                .filter(|p| idx >= p.instance.0 && idx < p.instance.1)
                .min_by_key(|p| p.instance.1 - p.instance.0)
                .expect("border owner");
            let (chrome_lo, chrome_hi) =
                crate::app::draw_cull::slot_own_chrome_range(owner, &frame.instances, 1.0);
            assert!(
                idx >= chrome_lo && idx < chrome_hi,
                "focused slot border is outside its own-chrome replay\n idx={idx} chrome={chrome_lo}..{chrome_hi} owner_bounds={:?} owner_inst={:?} layered={} range={:?}",
                owner.bounds,
                owner.instance,
                owner.layered,
                frame.instances[owner.instance.0..owner.instance.1]
                    .iter()
                    .map(|i| (i.rect_pos, i.rect_size, i.border_width))
                    .collect::<Vec<_>>()
            );
            let scaled: Vec<crate::renderer::InstanceInput> = frame
                .instances
                .iter()
                .copied()
                .map(|mut inst| {
                    inst.rect_pos[0] *= 1.5;
                    inst.rect_pos[1] *= 1.5;
                    inst.rect_size[0] *= 1.5;
                    inst.rect_size[1] *= 1.5;
                    inst
                })
                .collect();
            let (dpi_lo, dpi_hi) =
                crate::app::draw_cull::slot_own_chrome_range(owner, &scaled, 1.5);
            assert!(
                idx >= dpi_lo && idx < dpi_hi,
                "150% scale drops the focused slot border from own-chrome\n idx={idx} chrome={dpi_lo}..{dpi_hi} bounds={:?} inst=({:?}, {:?})",
                owner.bounds,
                border.rect_pos,
                border.rect_size
            );
            let restored = dirty.iter().filter(|d| d.kind == PaintDirtyKind::Node).any(|d| {
                frame.node_paints.iter().filter(|p| p.key == d.key).any(|p| {
                    let (lo, hi) =
                        crate::app::draw_cull::slot_own_chrome_range(p, &frame.instances, 1.0);
                    (lo..hi).any(|i| covers_slot(&frame.instances[i], vis(slot_now)))
                })
            });
            assert!(
                restored,
                "focus dest-reset does not redraw the focused slot\n dirty={dirty:?}"
            );
            let slot_vis = vis(slot_now);
            assert!(
                border.clip_size[0] < 0.0
                    || (border.clip_pos[0] <= slot_vis[0] + 1.0
                        && border.clip_pos[1] <= slot_vis[1] + 1.0
                        && border.clip_pos[0] + border.clip_size[0] >= slot_vis[0] + slot_vis[2] - 1.0
                        && border.clip_pos[1] + border.clip_size[1] >= slot_vis[1] + slot_vis[3] - 1.0),
                "focused slot border is clipped off its own box\n clip=({:?}, {:?}) slot={slot_vis:?}",
                border.clip_pos,
                border.clip_size
            );
        });
    }

    /// Gallery RSX OTP: focus must dest-reset the active slot's visual box, not
    /// the whole `input-otp` row. A row-wide clear looks like a gray focus frame
    /// around every cell (截图「聚焦是整行」).
    #[test]
    fn rsx_focus_wipe_stays_on_the_active_slot_visual_not_the_row() {
        create_root(|| {
            use crate::app::damage::{
                clear_frame_paint_nodes, take_frame_paint_nodes, PaintDirtyKind,
            };
            let focus = use_signal("__rui_button_focus_id", String::new);
            focus.set(String::new());
            // Gallery RSX shape: two groups of three + separator (builder path).
            let otp: Element = input_otp("gal-otp-rsx-focus")
                .max_length(6)
                .separator_at(3)
                .into();
            let mut root = Element::new()
                .w(520.0)
                .h(120.0)
                .overflow_y("scroll")
                .child(
                    Element::new()
                        .w("100%")
                        .p(16.0)
                        .bg([0.95, 0.95, 0.95, 1.0])
                        .child(otp),
                );
            root.focus_id = Some("scroll:gal-otp-rsx-focus".into());
            root.scroll_offset = [0.0, 0.0];
            let mut tree = crate::layout::layout_tree::LayoutTree::new();
            tree.build_and_layout(&mut root, 520.0, 120.0, [-1.0; 2]);
            tree.set_collect_skip_allowed(true);
            let idle = tree.build_and_layout_fast(&mut root, 520.0, 120.0, [-1.0; 2], true, true);
            let last_present = crate::app::draw_cull::last_present_visual_slots(
                &idle.node_paints,
                &idle.instances,
                &[],
                &idle.text_nodes,
                &idle.input_chrome_instances,
            );
            clear_frame_paint_nodes();
            focus.set(input_otp_focus_id("gal-otp-rsx-focus"));
            let mut collect = crate::core::tree::ReactiveSyncCollect::default();
            crate::core::tree::sync_reactive_elements_subtree_ex(
                &mut root,
                &mut tree,
                &mut LayoutStatus::Clean,
                520.0,
                120.0,
                [0.0; 2],
                false,
                &mut collect,
            );
            let layout_dirty: std::collections::HashSet<_> =
                collect.layout_dirty.into_iter().collect();
            tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
            let mut dirty = take_frame_paint_nodes();
            let frame =
                tree.build_and_layout_fast(&mut root, 520.0, 120.0, [-1.0; 2], false, true);
            crate::app::draw_cull::prepare_node_present_dirties(
                &mut dirty,
                &frame.node_paints,
                &last_present,
                &frame.instances,
                &[],
                &[],
                1.0,
                None,
            );
            let slots = slot_boxes(&root);
            assert!(slots.len() >= 6, "rsx otp slots: {slots:?}");
            let active = slots[0].1;
            let row_w: f32 = {
                let left = slots.iter().map(|(_, b)| b[0]).fold(f32::INFINITY, f32::min);
                let right = slots
                    .iter()
                    .map(|(_, b)| b[0] + b[2])
                    .fold(f32::NEG_INFINITY, f32::max);
                right - left
            };
            assert!(
                row_w > active[2] * 2.5,
                "fixture row must be wider than one slot: row={row_w} slot={}",
                active[2]
            );
            assert!(
                dirty.iter().all(|d| d.kind != PaintDirtyKind::Region),
                "focus must not window-clear a Region around the row: {dirty:?}"
            );
            let rings: Vec<_> = frame
                .instances
                .iter()
                .filter(|i| i.shadow_spread >= 1.0 && i.shadow_color[3] > 0.2)
                .map(|i| [i.rect_pos[0], i.rect_pos[1], i.rect_size[0], i.rect_size[1]])
                .collect();
            assert_eq!(
                rings.len(),
                1,
                "focus must paint one Hard ring on the active slot, not the row: {rings:?}"
            );
            assert!(
                rings[0][2] <= active[2] + 4.0 && rings[0][3] <= active[3] + 4.0,
                "Hard ring quad is the whole row, not the active slot\n ring={:?} active={active:?}",
                rings[0]
            );
            for d in &dirty {
                if d.kind == PaintDirtyKind::Overlay {
                    continue;
                }
                assert!(
                    d.bounds[2] <= active[2] + 24.0 && d.bounds[3] <= active[3] + 24.0,
                    "focus dirty spans the row, not the active visual slot\n dirty={d:?} active={active:?} row_w={row_w}"
                );
                let scissor = crate::app::draw_cull::node_present_scissor_logical(
                    d.kind,
                    d.key,
                    d.bounds,
                    &last_present,
                    &frame.node_paints,
                    8.0,
                );
                if scissor[2] <= 0.5 || scissor[3] <= 0.5 {
                    continue;
                }
                assert!(
                    scissor[2] <= active[2] + 32.0 && scissor[3] <= active[3] + 32.0,
                    "focus scissor is the whole row\n scissor={scissor:?} active={active:?} dirty={d:?}"
                );
                let prev = crate::app::draw_cull::last_present_prev_for_dirty(
                    d.kind,
                    d.key,
                    &last_present,
                );
                if let Some(reset) = crate::app::draw_cull::node_present_dest_reset_full(
                    d.key,
                    d.bounds,
                    &frame.node_paints,
                    &frame.instances,
                    &[],
                    &[],
                    1.0,
                    prev,
                ) {
                    let wipe = [
                        reset.rect[0],
                        reset.rect[1],
                        reset.rect[2],
                        reset.rect[3],
                    ];
                    assert!(
                        wipe[2] <= active[2] + 24.0 && wipe[3] <= active[3] + 24.0,
                        "focus dest-reset wipe is the whole row\n wipe={wipe:?} active={active:?} dirty={d:?}"
                    );
                    for (index, bounds) in slots.iter().skip(1) {
                        assert!(
                            !center_in(wipe, *bounds),
                            "focus wipe clears slot {index} (整行)\n wipe={wipe:?} slot={bounds:?}"
                        );
                    }
                }
            }
        });
    }

    #[test]
    fn focus_inside_page_scroll_dirties_the_slot_not_the_viewport() {
        create_root(|| {
            use crate::app::damage::{clear_frame_paint_nodes, take_frame_paint_nodes, PaintDirtyKind};
            let focus = use_signal("__rui_button_focus_id", String::new);
            focus.set(String::new());
            let otp: Element = input_otp("otp-page-hole").into();
            let mut root = Element::new()
                .w(800.0)
                .h(600.0)
                .overflow_y("hidden")
                .child(Element::new().w(240.0).h(32.0))
                .child(otp);
            let mut tree = crate::layout::layout_tree::LayoutTree::new();
            tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0; 2]);
            clear_frame_paint_nodes();
            focus.set(input_otp_focus_id("otp-page-hole"));
            let mut collect = crate::core::tree::ReactiveSyncCollect::default();
            crate::core::tree::sync_reactive_elements_subtree_ex(
                &mut root,
                &mut tree,
                &mut LayoutStatus::Clean,
                800.0,
                600.0,
                [0.0; 2],
                false,
                &mut collect,
            );
            let dirty = take_frame_paint_nodes();
            assert!(
                !collect.page_layout_changed,
                "slot caret must not FullClear the page"
            );
            assert!(
                dirty.iter().all(|d| d.kind != PaintDirtyKind::Region),
                "focus inside a page scroll must not window-clear a region: {dirty:?}"
            );
            assert!(
                dirty.iter().any(|d| d.kind == PaintDirtyKind::Node && d.bounds[2] > 0.0 && d.bounds[3] > 0.0),
                "active slot still needs a local dest-reset: {dirty:?}"
            );
            assert!(
                dirty
                    .iter()
                    .all(|d| d.bounds[2] <= 80.0 && d.bounds[3] <= 80.0),
                "OTP focus inside a page scroll recorded the ancestor clip, not the slot: {dirty:?}"
            );
        });
    }

    #[test]
    fn typed_digit_is_collected_after_structure_sync() {
        create_root(|| {
            let value = use_signal("otp-collect-value", String::new);
            let otp: Element = input_otp("otp-collect").value_signal(value).into();
            let mut root = Element::new()
                .w(400.0)
                .h(100.0)
                .overflow_y("hidden")
                .child(otp);
            let mut tree = crate::layout::layout_tree::LayoutTree::new();
            tree.build_and_layout(&mut root, 400.0, 100.0, [-1.0; 2]);
            tree.set_collect_skip_allowed(true);
            tree.build_and_layout_fast(&mut root, 400.0, 100.0, [-1.0; 2], true, true);
            value.set("1".into());
            let mut collect = crate::core::tree::ReactiveSyncCollect::default();
            crate::core::tree::sync_reactive_elements_subtree_ex(
                &mut root,
                &mut tree,
                &mut LayoutStatus::Clean,
                400.0,
                100.0,
                [0.0; 2],
                false,
                &mut collect,
            );
            let dirty = collect.layout_dirty.into_iter().collect();
            tree.mark_dirty_nodes_in_tree(&root, &dirty);
            let frame = tree.build_and_layout_fast(&mut root, 400.0, 100.0, [-1.0; 2], false, true);
            assert!(
                frame
                    .text_nodes
                    .iter()
                    .any(|t| t.text == "1" && t.width > 0.0),
                "new OTP glyph must be laid out and collected on its input frame"
            );
        });
    }

    fn slot_boxes(root: &Element) -> Vec<(usize, [f32; 4])> {
        let mut slots = Vec::new();
        fn walk(el: &Element, out: &mut Vec<(usize, [f32; 4])>) {
            if el.attrs.get("data-slot").map(|s| s.as_str()) == Some("input-otp-slot") {
                let index = el
                    .attrs
                    .get("data-index")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(usize::MAX);
                let vis = crate::core::tree::visual_paint_aabb(el, el.paint_scroll_acc);
                let b = if vis[2] > 0.5 && vis[3] > 0.5 {
                    vis
                } else {
                    el.layout_bounds
                };
                out.push((index, b));
            }
            for c in &el.children {
                walk(c, out);
            }
        }
        walk(root, &mut slots);
        slots.sort_by_key(|(i, _)| *i);
        slots
    }

    fn center_in(hole: [f32; 4], slot: [f32; 4]) -> bool {
        let cx = slot[0] + slot[2] * 0.5;
        let cy = slot[1] + slot[3] * 0.5;
        cx >= hole[0] && cy >= hole[1] && cx <= hole[0] + hole[2] && cy <= hole[1] + hole[3]
    }

    fn outset_box(b: [f32; 4], pad: f32) -> [f32; 4] {
        [b[0] - pad, b[1] - pad, b[2] + pad * 2.0, b[3] + pad * 2.0]
    }

    /// Painted SDF extent of the focus ring currently on `el` (spread/blur/offset + border + vertex AA).
    fn painted_chrome_outset(el: &Element) -> f32 {
        crate::app::draw_cull::sdf_outset_shadow_pad(
            el.style.shadow_blur,
            el.style.shadow_spread,
            el.style.shadow_offset,
            el.style.border_width,
        )
    }

    fn find_slot<'a>(root: &'a Element, index: usize) -> Option<&'a Element> {
        fn walk(el: &Element, index: usize) -> Option<&Element> {
            if el.attrs.get("data-slot").map(|s| s.as_str()) == Some("input-otp-slot")
                && el.attrs.get("data-index").and_then(|s| s.parse().ok()) == Some(index)
            {
                return Some(el);
            }
            for c in &el.children {
                if let Some(hit) = walk(c, index) {
                    return Some(hit);
                }
            }
            None
        }
        walk(root, index)
    }

    #[test]
    fn caret_blink_dirties_the_ibeam_not_the_slot() {
        create_root(|| {
            use crate::app::damage::{clear_frame_paint_nodes, take_frame_paint_nodes};
            OTP_CARET_FORCE.with(|c| c.set(Some(true)));
            let focus = use_signal("__rui_button_focus_id", String::new);
            focus.set(input_otp_focus_id("otp-blink"));
            let otp: Element = input_otp("otp-blink").into();
            // Gallery forms page: the OTP sits in a scroll/overflow clip. Blink must
            // not become that clip's island clear.
            let mut root = Element::new()
                .w(420.0)
                .h(80.0)
                .overflow_y("hidden")
                .child(otp);
            let mut tree = crate::layout::layout_tree::LayoutTree::new();
            tree.build_and_layout(&mut root, 420.0, 80.0, [-1.0; 2]);
            let mut collect = crate::core::tree::ReactiveSyncCollect::default();
            crate::core::tree::sync_reactive_elements_subtree_ex(
                &mut root,
                &mut tree,
                &mut LayoutStatus::Clean,
                420.0,
                80.0,
                [0.0; 2],
                false,
                &mut collect,
            );
            let slots = slot_boxes(&root);
            let active = slots[0].1;
            clear_frame_paint_nodes();
            OTP_CARET_FORCE.with(|c| c.set(Some(false)));
            let mut collect = crate::core::tree::ReactiveSyncCollect::default();
            crate::core::tree::sync_reactive_elements_subtree_ex(
                &mut root,
                &mut tree,
                &mut LayoutStatus::Clean,
                420.0,
                80.0,
                [0.0; 2],
                false,
                &mut collect,
            );
            OTP_CARET_FORCE.with(|c| c.set(None));
            assert!(
                !collect.page_layout_changed,
                "caret blink must not FullClear the page"
            );
            let dirty = take_frame_paint_nodes();
            assert!(
                !dirty.is_empty(),
                "hiding the caret must dirty the i-beam so the stroke leaves the retain buffer"
            );
            for d in &dirty {
                assert!(
                    d.bounds[2] <= 12.0 && d.bounds[3] <= 28.0,
                    "caret blink cleared the slot (seams / focus ring), not the i-beam\n dirty={d:?} slot={active:?}"
                );
                assert!(
                    d.bounds[0] >= active[0] + 4.0
                        && d.bounds[0] + d.bounds[2] <= active[0] + active[2] - 4.0
                        && d.bounds[1] >= active[1] - 0.5
                        && d.bounds[1] + d.bounds[3] <= active[1] + active[3] + 0.5,
                    "caret blink hole is not inside the cell, so it clips the ring or the shared border\n dirty={d:?} slot={active:?}"
                );
            }
        });
    }

    #[test]
    fn active_move_dirties_both_ring_halos_not_a_bystander_slot() {
        create_root(|| {
            use crate::app::damage::{clear_frame_paint_nodes, take_frame_paint_nodes};
            let focus = use_signal("__rui_button_focus_id", String::new);
            focus.set(input_otp_focus_id("otp-ring-move"));
            let value = use_signal("otp-ring-move__value", || "123".to_owned());
            let otp: Element = input_otp("otp-ring-move").value_signal(value).into();
            let mut root = Element::new().w(480.0).h(80.0).child(otp);
            let active = use_signal(&otp_active_key("otp-ring-move"), || 0usize);
            active.set(2);
            let mut tree = crate::layout::layout_tree::LayoutTree::new();
            tree.build_and_layout(&mut root, 480.0, 80.0, [-1.0; 2]);
            let mut collect = crate::core::tree::ReactiveSyncCollect::default();
            crate::core::tree::sync_reactive_elements_subtree_ex(
                &mut root,
                &mut tree,
                &mut LayoutStatus::Clean,
                480.0,
                80.0,
                [0.0; 2],
                false,
                &mut collect,
            );
            let leaving = find_slot(&root, 2).expect("slot 2");
            let ring_pad = painted_chrome_outset(leaving);
            assert!(
                ring_pad > 4.0,
                "focused OTP slot must be wearing the outset ring, pad={ring_pad}"
            );
            let boxes = slot_boxes(&root);
            clear_frame_paint_nodes();
            active.set(3);
            let mut collect = crate::core::tree::ReactiveSyncCollect::default();
            crate::core::tree::sync_reactive_elements_subtree_ex(
                &mut root,
                &mut tree,
                &mut LayoutStatus::Clean,
                480.0,
                80.0,
                [0.0; 2],
                false,
                &mut collect,
            );
            let dirty = take_frame_paint_nodes();
            let holes: Vec<[f32; 4]> = dirty.iter().map(|d| d.bounds).collect();
            let old_ring = outset_box(boxes[2].1, ring_pad);
            let entered = find_slot(&root, 3).expect("slot 3");
            let new_pad = painted_chrome_outset(entered);
            let new_ring = outset_box(boxes[3].1, new_pad.max(ring_pad));
            let corners = |b: [f32; 4]| {
                [
                    [b[0] + 0.5, b[1] + 0.5],
                    [b[0] + b[2] - 0.5, b[1] + 0.5],
                    [b[0] + 0.5, b[1] + b[3] - 0.5],
                    [b[0] + b[2] - 0.5, b[1] + b[3] - 0.5],
                ]
            };
            let covers_pt = |h: [f32; 4], p: [f32; 2]| {
                p[0] >= h[0] && p[1] >= h[1] && p[0] <= h[0] + h[2] && p[1] <= h[1] + h[3]
            };
            assert!(
                corners(old_ring)
                    .iter()
                    .all(|p| holes.iter().any(|h| covers_pt(*h, *p))),
                "leaving slot's focus ring is outside the dirty hole (residue)\n ring={old_ring:?} holes={holes:?}"
            );
            assert!(
                corners(new_ring)
                    .iter()
                    .all(|p| holes.iter().any(|h| covers_pt(*h, *p))),
                "entering slot's focus ring is outside the dirty hole\n ring={new_ring:?} holes={holes:?}"
            );
            assert!(
                holes.iter().all(|h| h[3] <= boxes[2].1[3] + ring_pad * 2.0 + 4.0),
                "active move cleared a column, not the ring\n holes={holes:?}"
            );
            for index in [0usize, 1, 4, 5] {
                assert!(
                    holes.iter().all(|h| !center_in(*h, boxes[index].1)),
                    "active move dirtied bystander slot {index} {:?}: holes={holes:?}",
                    boxes[index].1
                );
            }
        });
    }

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

    /// Backspace moves focus onto the previous slot and dirties the slot that
    /// just blurred, which sits on the new ring's right. That leave-wipe must
    /// not clear the focused ring's right outset.
    #[test]
    fn delete_must_not_clip_the_focused_slot_right_ring() {
        create_root(|| {
            use crate::app::damage::{clear_frame_paint_nodes, take_frame_paint_nodes, PaintDirtyKind};
            OTP_CARET_FORCE.with(|c| c.set(Some(true)));
            let value = use_signal("otp-delete-ring-value", String::new);
            let focus = use_signal("__rui_button_focus_id", String::new);
            focus.set(String::new());
            let otp: Element = input_otp("otp-delete-ring")
                .value_signal(value)
                .separator_at(3)
                .into();
            let mut root = Element::new()
                .w(480.0)
                .h(80.0)
                .overflow_y("hidden")
                .child(otp);
            root.style.bg_color = [1.0, 1.0, 1.0, 1.0];
            let mut tree = crate::layout::layout_tree::LayoutTree::new();
            tree.build_and_layout(&mut root, 480.0, 80.0, [-1.0; 2]);
            tree.set_collect_skip_allowed(true);
            focus.set(input_otp_focus_id("otp-delete-ring"));
            value.set("12".into());
            use_signal(&otp_active_key("otp-delete-ring"), || 0usize).set(2usize);
            clear_frame_paint_nodes();
            let mut collect = crate::core::tree::ReactiveSyncCollect::default();
            crate::core::tree::sync_reactive_elements_subtree_ex(
                &mut root,
                &mut tree,
                &mut LayoutStatus::Clean,
                480.0,
                80.0,
                [0.0; 2],
                false,
                &mut collect,
            );
            let layout_dirty: std::collections::HashSet<_> =
                collect.layout_dirty.into_iter().collect();
            tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
            let _ = take_frame_paint_nodes();
            let before =
                tree.build_and_layout_fast(&mut root, 480.0, 80.0, [-1.0; 2], false, true);
            let last_present = crate::app::draw_cull::last_present_visual_slots(
                &before.node_paints,
                &before.instances,
                &[],
                &before.text_nodes,
                &before.input_chrome_instances,
            );

            value.set("1".into());
            use_signal(&otp_active_key("otp-delete-ring"), || 0usize).set(1usize);
            clear_frame_paint_nodes();
            let mut collect = crate::core::tree::ReactiveSyncCollect::default();
            crate::core::tree::sync_reactive_elements_subtree_ex(
                &mut root,
                &mut tree,
                &mut LayoutStatus::Clean,
                480.0,
                80.0,
                [0.0; 2],
                false,
                &mut collect,
            );
            let layout_dirty: std::collections::HashSet<_> =
                collect.layout_dirty.into_iter().collect();
            tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
            let mut prepared = take_frame_paint_nodes();
            let frame = tree.build_and_layout_fast(&mut root, 480.0, 80.0, [-1.0; 2], false, true);
            crate::app::draw_cull::prepare_node_present_dirties(
                &mut prepared,
                &frame.node_paints,
                &last_present,
                &frame.instances,
                &[],
                &[],
                1.0,
                None,
            );
            OTP_CARET_FORCE.with(|c| c.set(None));
            let slots = slot_boxes(&root);
            let slot = |index: usize| {
                slots
                    .iter()
                    .find(|(i, _)| *i == index)
                    .map(|(_, b)| *b)
                    .unwrap_or_else(|| panic!("slot {index}: {slots:?}"))
            };
            let active = slot(1);
            let ring = frame
                .instances
                .iter()
                .find(|inst| inst.shadow_spread >= 2.0)
                .expect("ring");
            let paint = crate::app::draw_cull::instance_paint_bounds(ring);
            let active_right = active[0] + active[2];
            assert!(
                paint[0] + paint[2] >= active_right + 2.0,
                "ring quad has no right shadow past the focused slot\n paint={paint:?} active={active:?}"
            );
            let wipe_of = |slot: [f32; 4]| -> Option<[f32; 4]> {
                prepared.iter().find_map(|d| {
                    if d.kind != PaintDirtyKind::Node || !center_in(d.bounds, slot) {
                        return None;
                    }
                    let prev = crate::app::draw_cull::last_present_prev_for_dirty(
                        d.kind,
                        d.key,
                        &last_present,
                    );
                    crate::app::draw_cull::node_present_dest_reset_full(
                        d.key,
                        d.bounds,
                        &frame.node_paints,
                        &frame.instances,
                        &[],
                        &[],
                        1.0,
                        prev,
                    )
                    .map(|reset| reset.rect)
                })
            };
            let active_wipe = wipe_of(active).expect("focused slot dest-reset");
            assert!(
                active_wipe[0] + active_wipe[2] >= paint[0] + paint[2] - 0.5,
                "focused slot dest-reset clips its own right ring\n wipe={active_wipe:?} paint={paint:?}"
            );
            let hx = active_right + 1.0;
            let hy = active[1] + active[3] * 0.5;
            let foreign: Vec<_> = prepared
                .iter()
                .filter(|d| d.kind == PaintDirtyKind::Node && !center_in(d.bounds, active))
                .filter_map(|d| {
                    let prev = crate::app::draw_cull::last_present_prev_for_dirty(
                        d.kind,
                        d.key,
                        &last_present,
                    );
                    let rect = crate::app::draw_cull::node_present_dest_reset_full(
                        d.key,
                        d.bounds,
                        &frame.node_paints,
                        &frame.instances,
                        &[],
                        &[],
                        1.0,
                        prev,
                    )
                    .map(|reset| reset.rect)?;
                    let inside = hx >= rect[0]
                        && hy >= rect[1]
                        && hx < rect[0] + rect[2]
                        && hy < rect[1] + rect[3];
                    inside.then_some((d.bounds, rect))
                })
                .collect();
            assert!(
                foreign.is_empty(),
                "a neighbor wipe clears the focused ring's right edge\n halo=({hx},{hy}) paint={paint:?} foreign={foreign:?}\n prepared={prepared:?}"
            );
        });
    }

    /// After "111" the caret sits on the first slot of the next group. That
    /// ring's right edge overlaps the following cell, and each cell just left
    /// still has its right-edge ring in the framebuffer. The dest-reset wipe
    /// has to cover both fringes; clipping them off is the missing right
    /// shadow and the gray line left on every typed cell.
    #[test]
    fn typed_group_keeps_the_ring_right_edge_and_clears_the_old_one() {
        create_root(|| {
            use crate::app::damage::{clear_frame_paint_nodes, take_frame_paint_nodes, PaintDirtyKind};
            let value = use_signal("otp-right-edge-value", String::new);
            let focus = use_signal("__rui_button_focus_id", String::new);
            focus.set(String::new());
            let otp: Element = input_otp("otp-right-edge")
                .value_signal(value)
                .separator_at(3)
                .into();
            let mut root = Element::new()
                .w(480.0)
                .h(80.0)
                .overflow_y("hidden")
                .child(otp);
            root.style.bg_color = [0.95, 0.95, 0.95, 1.0];
            let mut tree = crate::layout::layout_tree::LayoutTree::new();
            tree.build_and_layout(&mut root, 480.0, 80.0, [-1.0; 2]);
            tree.set_collect_skip_allowed(true);
            focus.set(input_otp_focus_id("otp-right-edge"));
            value.set("11".into());
            use_signal(&otp_active_key("otp-right-edge"), || 0usize).set(2usize);
            clear_frame_paint_nodes();
            let mut collect = crate::core::tree::ReactiveSyncCollect::default();
            crate::core::tree::sync_reactive_elements_subtree_ex(
                &mut root,
                &mut tree,
                &mut LayoutStatus::Clean,
                480.0,
                80.0,
                [0.0; 2],
                false,
                &mut collect,
            );
            let layout_dirty: std::collections::HashSet<_> =
                collect.layout_dirty.into_iter().collect();
            tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
            let _ = take_frame_paint_nodes();
            let focused =
                tree.build_and_layout_fast(&mut root, 480.0, 80.0, [-1.0; 2], false, true);
            let last_present = crate::app::draw_cull::last_present_visual_slots(
                &focused.node_paints,
                &focused.instances,
                &[],
                &focused.text_nodes,
                &focused.input_chrome_instances,
            );

            value.set("111".into());
            use_signal(&otp_active_key("otp-right-edge"), || 0usize).set(3usize);
            clear_frame_paint_nodes();
            let mut collect = crate::core::tree::ReactiveSyncCollect::default();
            crate::core::tree::sync_reactive_elements_subtree_ex(
                &mut root,
                &mut tree,
                &mut LayoutStatus::Clean,
                480.0,
                80.0,
                [0.0; 2],
                false,
                &mut collect,
            );
            let layout_dirty: std::collections::HashSet<_> =
                collect.layout_dirty.into_iter().collect();
            tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
            let mut prepared = take_frame_paint_nodes();
            let frame = tree.build_and_layout_fast(&mut root, 480.0, 80.0, [-1.0; 2], false, true);
            crate::app::draw_cull::prepare_node_present_dirties(
                &mut prepared,
                &frame.node_paints,
                &last_present,
                &frame.instances,
                &[],
                &[],
                1.0,
                None,
            );
            let slots = slot_boxes(&root);
            let slot = |index: usize| {
                slots
                    .iter()
                    .find(|(i, _)| *i == index)
                    .map(|(_, b)| *b)
                    .unwrap_or_else(|| panic!("slot {index}: {slots:?}"))
            };
            let left = slot(2);
            let active = slot(3);
            let ring = frame
                .instances
                .iter()
                .find(|inst| inst.shadow_spread >= 2.0)
                .expect("ring");
            let paint = crate::app::draw_cull::instance_paint_bounds(ring);
            assert!(
                paint[0] + paint[2] >= active[0] + active[2] + 2.0,
                "ring quad has no right shadow past the active slot\n paint={paint:?} active={active:?}"
            );
            let wipe_covering = |slot: [f32; 4]| -> Option<[f32; 4]> {
                prepared.iter().find_map(|d| {
                    if d.kind != PaintDirtyKind::Node || !center_in(d.bounds, slot) {
                        return None;
                    }
                    let prev = crate::app::draw_cull::last_present_prev_for_dirty(
                        d.kind,
                        d.key,
                        &last_present,
                    );
                    crate::app::draw_cull::node_present_dest_reset_full(
                        d.key,
                        d.bounds,
                        &frame.node_paints,
                        &frame.instances,
                        &[],
                        &[],
                        1.0,
                        prev,
                    )
                    .map(|reset| reset.rect)
                })
            };
            let active_wipe = wipe_covering(active).expect("active slot dest-reset");
            assert!(
                active_wipe[0] + active_wipe[2] >= paint[0] + paint[2] - 0.5,
                "dest-reset clips the ring's right shadow\n wipe={active_wipe:?} paint={paint:?}"
            );
            assert!(
                crate::app::draw_cull::node_present_restore_underlay(
                    prepared
                        .iter()
                        .find(|d| d.kind == PaintDirtyKind::Node && center_in(d.bounds, active))
                        .expect("active dirty")
                        .key,
                    prepared
                        .iter()
                        .find(|d| d.kind == PaintDirtyKind::Node && center_in(d.bounds, active))
                        .expect("active dirty")
                        .bounds,
                    &frame.node_paints,
                ),
                "clearing the right shadow must redraw the next slot's border under the ring"
            );
            let leave_wipe = wipe_covering(left).expect("leaving slot dest-reset");
            assert!(
                leave_wipe[0] + leave_wipe[2] >= left[0] + left[2] + 2.0,
                "leaving slot does not erase the ring left on its right edge\n wipe={leave_wipe:?} slot={left:?}"
            );
        });
    }

    /// Empty first slot (no paint-nudge, no left neighbor) with the caret up.
    /// Gallery pixels: halo ~190 (two 50% composites; one is ~215) and the
    /// `bg-input/30` fill ~242 instead of ~248. A later caret blink must not
    /// src-over that quad again.
    #[test]
    fn empty_first_slot_ring_is_one_composite_and_blink_does_not_restack_it() {
        create_root(|| {
            use crate::app::damage::{
                clear_frame_paint_nodes, paint_dirty_has_keyed_slot, paint_dirty_matches_present_slot,
                take_frame_paint_nodes,
            };
            use crate::layout::layout_tree::PaintOp;
            OTP_CARET_FORCE.with(|c| c.set(Some(true)));
            let focus = use_signal("__rui_button_focus_id", String::new);
            focus.set(String::new());
            let otp: Element = input_otp("otp-empty-ring").separator_at(3).into();
            let mut root = Element::new()
                .w(480.0)
                .h(80.0)
                .overflow_y("hidden")
                .child(otp);
            root.style.bg_color = [1.0, 1.0, 1.0, 1.0];
            let mut tree = crate::layout::layout_tree::LayoutTree::new();
            tree.build_and_layout(&mut root, 480.0, 80.0, [-1.0; 2]);
            tree.set_collect_skip_allowed(true);
            focus.set(input_otp_focus_id("otp-empty-ring"));
            use_signal(&otp_active_key("otp-empty-ring"), || 0usize).set(0usize);
            clear_frame_paint_nodes();
            let mut collect = crate::core::tree::ReactiveSyncCollect::default();
            crate::core::tree::sync_reactive_elements_subtree_ex(
                &mut root,
                &mut tree,
                &mut LayoutStatus::Clean,
                480.0,
                80.0,
                [0.0; 2],
                false,
                &mut collect,
            );
            let layout_dirty: std::collections::HashSet<_> =
                collect.layout_dirty.into_iter().collect();
            tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
            let mut prepared = take_frame_paint_nodes();
            let frame = tree.build_and_layout_fast(&mut root, 480.0, 80.0, [-1.0; 2], false, true);
            crate::app::draw_cull::prepare_node_present_dirties(
                &mut prepared,
                &frame.node_paints,
                &[],
                &frame.instances,
                &[],
                &[],
                1.0,
                None,
            );
            let ring_idxs: Vec<usize> = frame
                .instances
                .iter()
                .enumerate()
                .filter(|(_, inst)| inst.shadow_spread >= 2.0)
                .map(|(i, _)| i)
                .collect();
            assert_eq!(ring_idxs.len(), 1, "empty slot collected {ring_idxs:?}");
            let ring_idx = ring_idxs[0];
            let draws = |dirty: &crate::app::damage::PaintDirty,
                         paints: &[crate::layout::layout_tree::NodePaint],
                         instances: &[crate::renderer::pipeline::InstanceInput],
                         ops: &[PaintOp],
                         prev: Option<[f32; 4]>,
                         idx: usize|
             -> (usize, String) {
                let Some(reset) = crate::app::draw_cull::node_present_dest_reset_full(
                    dirty.key,
                    dirty.bounds,
                    paints,
                    instances,
                    &[],
                    &[],
                    1.0,
                    prev,
                ) else {
                    let has_keyed = paint_dirty_has_keyed_slot(
                        dirty.key,
                        dirty.bounds,
                        paints.iter().map(|p| (p.key, p.bounds)),
                    );
                    let mut n = 0usize;
                    for p in paints {
                        if p.overlay
                            || !paint_dirty_matches_present_slot(
                                dirty.key,
                                dirty.bounds,
                                p.key,
                                p.bounds,
                                has_keyed,
                            )
                        {
                            continue;
                        }
                        n += crate::app::draw_cull::slot_paint_ops(ops, p.instance, (0, 0))
                            .filter(|op| match op {
                                PaintOp::Rects { start, count } => {
                                    let i = idx as u32;
                                    i >= *start && i < *start + *count
                                }
                                PaintOp::Textured { .. } => false,
                            })
                            .count();
                    }
                    return (n, format!("no-reset ops={n}"));
                };
                let has_keyed = paint_dirty_has_keyed_slot(
                    dirty.key,
                    dirty.bounds,
                    paints.iter().map(|p| (p.key, p.bounds)),
                );
                let hit = |range: (usize, usize)| {
                    crate::app::draw_cull::slot_paint_ops(ops, range, (0, 0)).any(|op| match op {
                        PaintOp::Rects { start, count } => {
                            let i = idx as u32;
                            i >= start && i < start + count
                        }
                        PaintOp::Textured { .. } => false,
                    })
                };
                let mut why = String::new();
                let mut n = usize::from(idx >= reset.replay.0 && idx < reset.replay.1);
                if n > 0 {
                    why.push_str(&format!("replay={:?} ", reset.replay));
                }
                if crate::app::draw_cull::node_present_restore_underlay(
                    dirty.key,
                    dirty.bounds,
                    paints,
                ) {
                    for p in paints {
                        if p.overlay
                            || paint_dirty_matches_present_slot(
                                dirty.key,
                                dirty.bounds,
                                p.key,
                                p.bounds,
                                has_keyed,
                            )
                            || !crate::app::draw_cull::node_present_underlay_paint(
                                dirty.key,
                                dirty.bounds,
                                p,
                                reset.rect,
                                paints,
                                instances,
                            )
                        {
                            continue;
                        }
                        if hit(p.instance) {
                            n += 1;
                            why.push_str(&format!(
                                "underlay key={} inst={:?} ",
                                p.key, p.instance
                            ));
                        }
                    }
                }
                for p in paints {
                    if !p.overlay
                        && paint_dirty_matches_present_slot(
                            dirty.key,
                            dirty.bounds,
                            p.key,
                            p.bounds,
                            has_keyed,
                        )
                        && hit(p.instance)
                    {
                        n += 1;
                        why.push_str(&format!("own key={} inst={:?} ", p.key, p.instance));
                    }
                }
                (n, why)
            };
            let hits: Vec<_> = prepared
                .iter()
                .filter_map(|d| {
                    let (n, why) = draws(
                        d,
                        &frame.node_paints,
                        &frame.instances,
                        &frame.paint_ops,
                        None,
                        ring_idx,
                    );
                    (n > 0).then_some((d.kind, d.bounds, n, why))
                })
                .collect();
            let total: usize = hits.iter().map(|(_, _, n, _)| n).sum();
            assert_eq!(
                total, 1,
                "empty first slot src-overs the ring\n draws={total} hits={hits:?} caret_kids={}",
                find_slot(&root, 0).map(|s| s.children.len()).unwrap_or(0)
            );

            let last_present = crate::app::draw_cull::last_present_visual_slots(
                &frame.node_paints,
                &frame.instances,
                &[],
                &frame.text_nodes,
                &frame.input_chrome_instances,
            );
            clear_frame_paint_nodes();
            OTP_CARET_FORCE.with(|c| c.set(Some(false)));
            let mut collect = crate::core::tree::ReactiveSyncCollect::default();
            crate::core::tree::sync_reactive_elements_subtree_ex(
                &mut root,
                &mut tree,
                &mut LayoutStatus::Clean,
                480.0,
                80.0,
                [0.0; 2],
                false,
                &mut collect,
            );
            let layout_dirty: std::collections::HashSet<_> =
                collect.layout_dirty.into_iter().collect();
            tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
            let mut blink_prepared = take_frame_paint_nodes();
            let blinked =
                tree.build_and_layout_fast(&mut root, 480.0, 80.0, [-1.0; 2], false, true);
            crate::app::draw_cull::prepare_node_present_dirties(
                &mut blink_prepared,
                &blinked.node_paints,
                &last_present,
                &blinked.instances,
                &[],
                &[],
                1.0,
                None,
            );
            OTP_CARET_FORCE.with(|c| c.set(None));
            let slot_box = find_slot(&root, 0)
                .map(|s| s.layout_bounds)
                .expect("slot");
            assert!(
                !blink_prepared.is_empty(),
                "hiding the caret must dirty the stroke"
            );
            for d in &blink_prepared {
                assert!(
                    d.bounds[0] >= slot_box[0] + 4.0
                        && d.bounds[0] + d.bounds[2] <= slot_box[0] + slot_box[2] - 4.0
                        && d.bounds[1] >= slot_box[1] - 0.5
                        && d.bounds[1] + d.bounds[3] <= slot_box[1] + slot_box[3] + 0.5,
                    "caret blink hole reaches the focus ring\n dirty={d:?} slot={slot_box:?}"
                );
                if d.kind == crate::app::damage::PaintDirtyKind::Region {
                    assert!(
                        !crate::app::draw_cull::region_replays_own_chrome_only(
                            d.key,
                            d.bounds,
                            &blinked.node_paints,
                        ),
                        "caret blink src-overs the slot fill\n dirty={d:?}"
                    );
                } else {
                    let prev = crate::app::draw_cull::last_present_prev_for_dirty(
                        d.kind,
                        d.key,
                        &last_present,
                    );
                    let (n, why) = draws(
                        d,
                        &blinked.node_paints,
                        &blinked.instances,
                        &blinked.paint_ops,
                        prev,
                        blinked
                            .instances
                            .iter()
                            .position(|inst| inst.shadow_spread >= 2.0)
                            .expect("ring"),
                    );
                    assert_eq!(
                        n, 0,
                        "caret blink src-overs the focus ring onto retain\n dirty={d:?} {why}"
                    );
                }
            }
        });
    }

    /// Slot fill is `bg-input/30`. Src-over of that quad twice darkens the cell
    /// and the focus ring. Blur must clear the ring and paint the translucent
    /// fill once, or the darkened cell stays as a gray patch.
    #[test]
    fn focus_ring_is_painted_once_and_blur_clears_it() {
        create_root(|| {
            use crate::app::damage::{
                clear_frame_paint_nodes, paint_dirty_has_keyed_slot, paint_dirty_matches_present_slot,
                take_frame_paint_nodes, PaintDirtyKind,
            };
            use crate::layout::layout_tree::PaintOp;
            let value = use_signal("otp-darken-value", String::new);
            let focus = use_signal("__rui_button_focus_id", String::new);
            focus.set(String::new());
            let otp: Element = input_otp("otp-darken")
                .value_signal(value)
                .separator_at(3)
                .into();
            let mut root = Element::new()
                .w(480.0)
                .h(80.0)
                .overflow_y("hidden")
                .child(otp);
            root.style.bg_color = [0.95, 0.95, 0.95, 1.0];
            let mut tree = crate::layout::layout_tree::LayoutTree::new();
            tree.build_and_layout(&mut root, 480.0, 80.0, [-1.0; 2]);
            tree.set_collect_skip_allowed(true);
            focus.set(input_otp_focus_id("otp-darken"));
            value.set("111111".into());
            use_signal(&otp_active_key("otp-darken"), || 0usize).set(5usize);
            clear_frame_paint_nodes();
            let mut collect = crate::core::tree::ReactiveSyncCollect::default();
            crate::core::tree::sync_reactive_elements_subtree_ex(
                &mut root,
                &mut tree,
                &mut LayoutStatus::Clean,
                480.0,
                80.0,
                [0.0; 2],
                false,
                &mut collect,
            );
            let layout_dirty: std::collections::HashSet<_> =
                collect.layout_dirty.into_iter().collect();
            tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
            let mut prepared = take_frame_paint_nodes();
            let frame = tree.build_and_layout_fast(&mut root, 480.0, 80.0, [-1.0; 2], false, true);
            crate::app::draw_cull::prepare_node_present_dirties(
                &mut prepared,
                &frame.node_paints,
                &[],
                &frame.instances,
                &[],
                &[],
                1.0,
                None,
            );
            let slots = slot_boxes(&root);
            let active = slots
                .iter()
                .find(|(i, _)| *i == 5)
                .map(|(_, b)| *b)
                .expect("slot 5");
            let ring_idxs: Vec<usize> = frame
                .instances
                .iter()
                .enumerate()
                .filter(|(_, inst)| inst.shadow_spread >= 2.0)
                .map(|(i, _)| i)
                .collect();
            assert_eq!(
                ring_idxs.len(),
                1,
                "focus collected {} ring quads at {:?}",
                ring_idxs.len(),
                ring_idxs
                    .iter()
                    .map(|i| frame.instances[*i].rect_pos)
                    .collect::<Vec<_>>()
            );
            let ring_idx = ring_idxs[0];
            let ring_paint = crate::app::draw_cull::instance_paint_bounds(&frame.instances[ring_idx]);
            let draws_index = |dirty: &crate::app::damage::PaintDirty,
                               paints: &[crate::layout::layout_tree::NodePaint],
                               instances: &[crate::renderer::pipeline::InstanceInput],
                               ops: &[PaintOp],
                               idx: usize|
             -> usize {
                let prev = None;
                let Some(reset) = crate::app::draw_cull::node_present_dest_reset_full(
                    dirty.key,
                    dirty.bounds,
                    paints,
                    instances,
                    &[],
                    &[],
                    1.0,
                    prev,
                ) else {
                    // No CLEAR. The slot quad is still submitted and src-overs
                    // whatever the previous pass left, including its own ring.
                    let has_keyed = paint_dirty_has_keyed_slot(
                        dirty.key,
                        dirty.bounds,
                        paints.iter().map(|p| (p.key, p.bounds)),
                    );
                    let mut n = 0;
                    if dirty.kind == PaintDirtyKind::Region {
                        let b = crate::app::draw_cull::instance_paint_bounds(&instances[idx]);
                        if crate::app::damage::intersect_rects(dirty.bounds, b).is_some() {
                            n += 1;
                        }
                    }
                    for p in paints {
                        if p.overlay
                            || !paint_dirty_matches_present_slot(
                                dirty.key,
                                dirty.bounds,
                                p.key,
                                p.bounds,
                                has_keyed,
                            )
                        {
                            continue;
                        }
                        n += crate::app::draw_cull::slot_paint_ops(ops, p.instance, (0, 0))
                            .filter(|op| match op {
                                PaintOp::Rects { start, count } => {
                                    let i = idx as u32;
                                    i >= *start && i < *start + *count
                                }
                                PaintOp::Textured { .. } => false,
                            })
                            .count();
                    }
                    return n;
                };
                let has_keyed = paint_dirty_has_keyed_slot(
                    dirty.key,
                    dirty.bounds,
                    paints.iter().map(|p| (p.key, p.bounds)),
                );
                let op_count = |range: (usize, usize)| -> usize {
                    crate::app::draw_cull::slot_paint_ops(ops, range, (0, 0))
                        .filter(|op| match op {
                            PaintOp::Rects { start, count } => {
                                let i = idx as u32;
                                i >= *start && i < *start + *count
                            }
                            PaintOp::Textured { .. } => false,
                        })
                        .count()
                };
                let mut n = 0;
                // `draw_sdf_range` submits replay indices directly, then the slot
                // paint op submits the same quad again.
                if idx >= reset.replay.0 && idx < reset.replay.1 {
                    n += 1;
                }
                if crate::app::draw_cull::node_present_restore_underlay(
                    dirty.key,
                    dirty.bounds,
                    paints,
                ) {
                    for p in paints {
                        if p.overlay
                            || paint_dirty_matches_present_slot(
                                dirty.key,
                                dirty.bounds,
                                p.key,
                                p.bounds,
                                has_keyed,
                            )
                            || !crate::app::draw_cull::node_present_underlay_paint(
                                dirty.key,
                                dirty.bounds,
                                p,
                                reset.rect,
                                paints,
                                instances,
                            )
                        {
                            continue;
                        }
                        if op_count(p.instance) > 0 {
                            n += op_count(p.instance);
                        }
                    }
                }
                for p in paints {
                    if p.overlay
                        || !paint_dirty_matches_present_slot(
                            dirty.key,
                            dirty.bounds,
                            p.key,
                            p.bounds,
                            has_keyed,
                        )
                    {
                        continue;
                    }
                    n += op_count(p.instance);
                }
                n
            };
            let focus_hits: Vec<_> = prepared
                .iter()
                .filter_map(|d| {
                    let n = draws_index(
                        d,
                        &frame.node_paints,
                        &frame.instances,
                        &frame.paint_ops,
                        ring_idx,
                    );
                    let reset = crate::app::draw_cull::node_present_dest_reset_full(
                        d.key,
                        d.bounds,
                        &frame.node_paints,
                        &frame.instances,
                        &[],
                        &[],
                        1.0,
                        None,
                    )
                    .map(|r| (r.rect, r.replay));
                    (n > 0).then_some((d.kind, d.bounds, n, reset))
                })
                .collect();
            let focus_draws: usize = focus_hits.iter().map(|(_, _, n, _)| *n).sum();
            assert_eq!(
                focus_draws, 1,
                "focus src-overs the translucent slot and the ring\n draws={focus_draws} hits={focus_hits:?} ring={ring_paint:?}"
            );
            let focus_dirty = prepared
                .iter()
                .find(|d| d.kind == PaintDirtyKind::Node && center_in(d.bounds, active))
                .expect("focused slot dirty");
            let focus_wipe = crate::app::draw_cull::node_present_dest_reset_full(
                focus_dirty.key,
                focus_dirty.bounds,
                &frame.node_paints,
                &frame.instances,
                &[],
                &[],
                1.0,
                None,
            )
            .expect("focus dest-reset")
            .rect;
            let focus_covers = focus_wipe[0] <= ring_paint[0] + 0.5
                && focus_wipe[1] <= ring_paint[1] + 0.5
                && focus_wipe[0] + focus_wipe[2] >= ring_paint[0] + ring_paint[2] - 0.5
                && focus_wipe[1] + focus_wipe[3] >= ring_paint[1] + ring_paint[3] - 0.5;
            assert!(
                focus_covers,
                "focus clear misses the ring halo\n wipe={focus_wipe:?} ring={ring_paint:?}"
            );

            let last_present = crate::app::draw_cull::last_present_visual_slots(
                &frame.node_paints,
                &frame.instances,
                &[],
                &frame.text_nodes,
                &frame.input_chrome_instances,
            );
            focus.set(String::new());
            clear_frame_paint_nodes();
            let mut collect = crate::core::tree::ReactiveSyncCollect::default();
            crate::core::tree::sync_reactive_elements_subtree_ex(
                &mut root,
                &mut tree,
                &mut LayoutStatus::Clean,
                480.0,
                80.0,
                [0.0; 2],
                false,
                &mut collect,
            );
            let layout_dirty: std::collections::HashSet<_> =
                collect.layout_dirty.into_iter().collect();
            tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
            let mut prepared = take_frame_paint_nodes();
            let blurred = tree.build_and_layout_fast(&mut root, 480.0, 80.0, [-1.0; 2], false, true);
            crate::app::draw_cull::prepare_node_present_dirties(
                &mut prepared,
                &blurred.node_paints,
                &last_present,
                &blurred.instances,
                &[],
                &[],
                1.0,
                None,
            );
            let slots = slot_boxes(&root);
            let active = slots
                .iter()
                .find(|(i, _)| *i == 5)
                .map(|(_, b)| *b)
                .expect("slot 5 after blur");
            let blur_dirty = prepared
                .iter()
                .find(|d| d.kind == PaintDirtyKind::Node && center_in(d.bounds, active))
                .expect("blur dirty");
            let fill_idx = blurred
                .instances
                .iter()
                .position(|inst| {
                    let b = [inst.rect_pos[0], inst.rect_pos[1], inst.rect_size[0], inst.rect_size[1]];
                    center_in(b, active) && inst.color[3] > 0.05 && inst.color[3] < 0.9
                })
                .expect("translucent slot fill");
            let blur_draws = draws_index(
                blur_dirty,
                &blurred.node_paints,
                &blurred.instances,
                &blurred.paint_ops,
                fill_idx,
            );
            assert_eq!(
                blur_draws, 1,
                "blur src-overs the translucent fill; the cell stays gray\n draws={blur_draws}"
            );
            let prev = crate::app::draw_cull::last_present_prev_for_dirty(
                blur_dirty.kind,
                blur_dirty.key,
                &last_present,
            );
            let wipe = crate::app::draw_cull::node_present_dest_reset_full(
                blur_dirty.key,
                blur_dirty.bounds,
                &blurred.node_paints,
                &blurred.instances,
                &[],
                &[],
                1.0,
                prev,
            )
            .expect("blur dest-reset")
            .rect;
            let covers = wipe[0] <= ring_paint[0] + 0.5
                && wipe[1] <= ring_paint[1] + 0.5
                && wipe[0] + wipe[2] >= ring_paint[0] + ring_paint[2] - 0.5
                && wipe[1] + wipe[3] >= ring_paint[1] + ring_paint[3] - 0.5;
            assert!(
                covers,
                "blur leaves the focus ring\n wipe={wipe:?} ring={ring_paint:?}"
            );
        });
    }

    /// Typing moves the ring. The redraw of that focus must dest-reset the
    /// slot's own SDF. A layout-box Region window-clears and page-band-replays,
    /// which paints the ring stroke off the active cell.
    #[test]
    fn typed_digit_focus_redraw_is_the_slot_ring_not_a_page_band_clear() {
        create_root(|| {
            use crate::app::damage::{clear_frame_paint_nodes, take_frame_paint_nodes, PaintDirtyKind};
            let value = use_signal("otp-type-ring-value", String::new);
            let focus = use_signal("__rui_button_focus_id", String::new);
            focus.set(String::new());
            let otp: Element = input_otp("otp-type-ring")
                .value_signal(value)
                .separator_at(3)
                .into();
            let mut root = Element::new()
                .w(480.0)
                .h(80.0)
                .overflow_y("hidden")
                .child(otp);
            // Page fill so dest-reset has a backdrop. Without it the halo Node
            // is promoted to a window-clear Region.
            root.style.bg_color = [0.95, 0.95, 0.95, 1.0];
            let mut tree = crate::layout::layout_tree::LayoutTree::new();
            tree.build_and_layout(&mut root, 480.0, 80.0, [-1.0; 2]);
            tree.set_collect_skip_allowed(true);
            clear_frame_paint_nodes();
            focus.set(input_otp_focus_id("otp-type-ring"));
            let mut collect = crate::core::tree::ReactiveSyncCollect::default();
            crate::core::tree::sync_reactive_elements_subtree_ex(
                &mut root,
                &mut tree,
                &mut LayoutStatus::Clean,
                480.0,
                80.0,
                [0.0; 2],
                false,
                &mut collect,
            );
            let layout_dirty: std::collections::HashSet<_> =
                collect.layout_dirty.into_iter().collect();
            tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
            let _ = take_frame_paint_nodes();
            let focused =
                tree.build_and_layout_fast(&mut root, 480.0, 80.0, [-1.0; 2], false, true);
            let last_present = crate::app::draw_cull::last_present_visual_slots(
                &focused.node_paints,
                &focused.instances,
                &[],
                &focused.text_nodes,
                &focused.input_chrome_instances,
            );

            value.set("4".into());
            use_signal(&otp_active_key("otp-type-ring"), || 0usize).set(1usize);
            clear_frame_paint_nodes();
            let mut collect = crate::core::tree::ReactiveSyncCollect::default();
            crate::core::tree::sync_reactive_elements_subtree_ex(
                &mut root,
                &mut tree,
                &mut LayoutStatus::Clean,
                480.0,
                80.0,
                [0.0; 2],
                false,
                &mut collect,
            );
            let layout_dirty: std::collections::HashSet<_> =
                collect.layout_dirty.into_iter().collect();
            tree.mark_dirty_nodes_in_tree(&root, &layout_dirty);
            let dirty = take_frame_paint_nodes();
            let frame = tree.build_and_layout_fast(&mut root, 480.0, 80.0, [-1.0; 2], false, true);
            let slots = slot_boxes(&root);
            let active = slots
                .iter()
                .find(|(i, _)| *i == 1)
                .map(|(_, b)| *b)
                .expect("active slot");
            let rings: Vec<_> = frame
                .instances
                .iter()
                .filter(|inst| inst.shadow_spread >= 2.0)
                .map(|inst| (inst.rect_pos, inst.rect_size, inst.shadow_spread, inst.border_color))
                .collect();
            assert_eq!(
                rings.len(),
                1,
                "focus ring SDF must exist only on the active slot\n rings={rings:?}\n slots={slots:?}"
            );
            let (pos, size, _, _) = rings[0];
            assert!(
                (pos[0] - active[0]).abs() <= 2.0
                    && (pos[1] - active[1]).abs() <= 2.0
                    && (size[0] - active[2]).abs() <= 4.0
                    && (size[1] - active[3]).abs() <= 4.0,
                "ring quad is not the active slot\n quad=({pos:?}, {size:?}) active={active:?}"
            );
            let mut prepared = dirty.clone();
            crate::app::draw_cull::prepare_node_present_dirties(
                &mut prepared,
                &frame.node_paints,
                &last_present,
                &frame.instances,
                &[],
                &[],
                1.0,
                None,
            );
            let page_band: Vec<_> = prepared
                .iter()
                .filter(|d| {
                    d.kind == PaintDirtyKind::Region
                        && !crate::app::draw_cull::region_replays_own_chrome_only(
                            d.key,
                            d.bounds,
                            &frame.node_paints,
                        )
                })
                .map(|d| d.bounds)
                .collect();
            assert!(
                page_band.is_empty(),
                "typing window-clears and page-band-replays the focus ring off the cell\n holes={page_band:?}\n slots={slots:?}\n dirty={dirty:?}"
            );
            let ring = frame
                .instances
                .iter()
                .find(|inst| inst.shadow_spread >= 2.0)
                .expect("ring");
            let paint = crate::app::draw_cull::instance_paint_bounds(ring);
            assert!(
                prepared.iter().any(|d| {
                    d.kind == PaintDirtyKind::Node
                        && d.bounds[0] <= paint[0] + 0.5
                        && d.bounds[1] <= paint[1] + 0.5
                        && d.bounds[0] + d.bounds[2] >= paint[0] + paint[2] - 0.5
                        && d.bounds[1] + d.bounds[3] >= paint[1] + paint[3] - 0.5
                }),
                "focus redraw does not dest-reset the ring SDF\n paint={paint:?}\n prepared={prepared:?}"
            );
            for (index, bounds) in slots.iter().filter(|(i, _)| *i >= 3) {
                assert!(
                    prepared.iter().all(|d| !center_in(d.bounds, *bounds)),
                    "focus redraw covers slot {index} {bounds:?}: prepared={prepared:?}"
                );
            }
            // Screenshot after typing "1": the caret stays in slot 1, but a full
            // ring-colored outline is painted on slot 2 and later joined borders
            // are left blank. The ring quad (VS translation included) has to
            // contain the caret, and the active slot must not open a page-stack
            // band that also owns later groups — that composite is what leaves
            // the stroke off the cell.
            let caret = frame
                .instances
                .iter()
                .find(|inst| inst.rect_size[0] <= 2.0 && inst.rect_size[1] >= 8.0)
                .expect("caret line");
            let caret_c = [
                caret.rect_pos[0] + caret.rect_size[0] * 0.5 + caret.transform[4],
                caret.rect_pos[1] + caret.rect_size[1] * 0.5 + caret.transform[5],
            ];
            let ring_vis = [
                pos[0] + ring.transform[4],
                pos[1] + ring.transform[5],
                size[0],
                size[1],
            ];
            assert!(
                caret_c[0] >= ring_vis[0]
                    && caret_c[0] <= ring_vis[0] + ring_vis[2]
                    && caret_c[1] >= ring_vis[1]
                    && caret_c[1] <= ring_vis[1] + ring_vis[3],
                "focus ring is not on the caret's slot\n caret={caret_c:?} ring={ring_vis:?} xf={:?}\n slots={slots:?}",
                ring.transform
            );
            let ring_idx = frame
                .instances
                .iter()
                .position(|inst| inst.shadow_spread >= 2.0)
                .expect("ring index");
            let bands = frame.overlay_split.page_stack_band_ranges();
            for band in &bands {
                if ring_idx < band.instance.0 || ring_idx >= band.instance.1 {
                    continue;
                }
                for (index, bounds) in slots.iter().filter(|(i, _)| *i >= 3) {
                    let swallowed = frame.instances[band.instance.0..band.instance.1]
                        .iter()
                        .any(|inst| {
                            center_in(
                                [inst.rect_pos[0], inst.rect_pos[1], inst.rect_size[0], inst.rect_size[1]],
                                *bounds,
                            )
                        });
                    assert!(
                        !swallowed,
                        "active slot's page band also owns slot {index}; replay paints the ring off the cell\n band={:?} slots={slots:?}",
                        band.instance
                    );
                }
            }
            let ring_owner = frame
                .node_paints
                .iter()
                .filter(|p| ring_idx >= p.instance.0 && ring_idx < p.instance.1)
                .min_by_key(|p| p.instance.1 - p.instance.0)
                .expect("ring owner");
            assert!(
                !ring_owner.layered,
                "active otp slot is a page-stack layer, so dest-reset clears joined borders\n owner_bounds={:?} owner_inst={:?}",
                ring_owner.bounds,
                ring_owner.instance
            );
            for d in &prepared {
                if d.kind != PaintDirtyKind::Node {
                    continue;
                }
                let prev = crate::app::draw_cull::last_present_prev_for_dirty(
                    d.kind,
                    d.key,
                    &last_present,
                );
                let Some(reset) = crate::app::draw_cull::node_present_dest_reset_full(
                    d.key,
                    d.bounds,
                    &frame.node_paints,
                    &frame.instances,
                    &[],
                    &[],
                    1.0,
                    prev,
                ) else {
                    continue;
                };
                let wipe = [
                    reset.rect[0],
                    reset.rect[1],
                    reset.rect[2],
                    reset.rect[3],
                ];
                assert!(
                    wipe[2] <= active[2] + 24.0 && wipe[3] <= active[3] + 24.0,
                    "focus dest-reset wipe is wider than the slot ring\n wipe={wipe:?} active={active:?} dirty={d:?}"
                );
                for (index, bounds) in slots.iter().filter(|(i, _)| *i >= 3) {
                    assert!(
                        !center_in(wipe, *bounds),
                        "focus dest-reset clears slot {index}\n wipe={wipe:?} slot={bounds:?}"
                    );
                }
            }
        });
    }

    /// Gallery nests OTP in a column (`VStack` / `align-items: stretch`). With
    /// `w("auto")` the focusable root fills the cross axis — a full-width AABB
    /// while the slots only occupy the left. Anything that paints or dirties
    /// that box (a11y focus rect, accidental root ring, row-wide wipe) looks
    /// like「聚焦是整行」instead of the active slot's visual range.
    #[test]
    fn focusable_root_hugs_slot_row_not_parent_cross_axis() {
        create_root(|| {
            let otp: Element = input_otp("otp-hug-row")
                .max_length(6)
                .separator_at(3)
                .into();
            let mut root = vstack()
                .w(480.0)
                .p(16.0)
                .child(otp)
                .into();
            let mut tree = crate::layout::layout_tree::LayoutTree::new();
            tree.build_and_layout(&mut root, 480.0, 120.0, [-1.0; 2]);
            fn find_root(el: &Element) -> Option<&Element> {
                if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_ROOT) {
                    return Some(el);
                }
                el.children.iter().find_map(find_root)
            }
            let otp_root = find_root(&root).expect("otp root");
            let slots = slot_boxes(&root);
            assert!(slots.len() >= 6, "slots={slots:?}");
            let left = slots.iter().map(|(_, b)| b[0]).fold(f32::INFINITY, f32::min);
            let right = slots
                .iter()
                .map(|(_, b)| b[0] + b[2])
                .fold(f32::NEG_INFINITY, f32::max);
            let top = slots.iter().map(|(_, b)| b[1]).fold(f32::INFINITY, f32::min);
            let bottom = slots
                .iter()
                .map(|(_, b)| b[1] + b[3])
                .fold(f32::NEG_INFINITY, f32::max);
            let visual = [left, top, right - left, bottom - top];
            let shell = otp_root.layout_bounds;
            assert!(
                shell[2] <= visual[2] + 8.0,
                "focusable OTP root stretches to the column width (整行), not the slot row\n shell={shell:?} visual_row={visual:?} parent_w=480"
            );
            assert!(
                shell[3] <= visual[3] + 8.0,
                "focusable OTP root is taller than the slot row\n shell={shell:?} visual_row={visual:?}"
            );
            assert!(
                (shell[0] - visual[0]).abs() <= 2.0 && (shell[1] - visual[1]).abs() <= 2.0,
                "focusable OTP root origin drifts from the slot row\n shell={shell:?} visual_row={visual:?}"
            );
        });
    }
}
