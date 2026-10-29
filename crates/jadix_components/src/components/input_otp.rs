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
//! - **必填稳定 `id`**（多实例非受控）；空 → `__jadix_input_otp_rt_N`。
//! - `focus_id` 为 `input-otp:{id}`；键盘落在根上；根壳 `w(fit-content)` 贴合槽位行（避免列布局 stretch 成整行焦点盒）。
//! - 默认仅接受 ASCII 数字；`pattern="alnum"` 可字母数字。
//! - `<InputOTP*>` 走通用 PascalCase → `*Props`；RSX `maxLength` 由宏映到 `max_length`。

#[cfg(all(test, feature = "product-tests"))]
use std::cell::Cell;
use std::sync::OnceLock;

use crate::authoring::widgets::{caret_blink_visible, restart_caret_blink};
use crate::components::base::SlotChrome;
use crate::core::reactivity::{
    create_context, create_effect, provide_context, use_context, use_signal, with_child_owner,
    Context, Signal,
};
use crate::core::tree::{Element, LayoutStatus};
use crate::impl_simple_slot_builder;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box};
use crate::style::{
    apply_focus_ring_shadow_color, focus_ring_border_color, focus_ring_color,
    focus_ring_destructive_color, parse_color, parse_text_color,
};
use crate::tokens;
use crate::utils::controlled::resolve_string_signal;
use crate::utils::instance_id::resolve_instance_id;
use crate::utils::lucide_glyph;
use crate::utils::take_fragment;
use crate::utils::text_node::{build_icon_text_node, build_text_node, TextNodeConfig};
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

/// Group 内边位置（供圆角 / 重叠描边复用）。
const ATTR_OTP_EDGE: &str = "jadix_otp_edge";
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

#[cfg(all(test, feature = "product-tests"))]
thread_local! {
    /// Tests pin the blink phase. Production always reads the shared caret clock.
    static OTP_CARET_FORCE: Cell<Option<bool>> = const { Cell::new(None) };
}

fn caret_blink_on() -> bool {
    #[cfg(all(test, feature = "product-tests"))]
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
    crate::authoring::widgets::shadows::sdf_outset_shadow_pad(
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
    let b = crate::core::tree::visual_paint_aabb(line, slot.paint_scroll_offset());
    // Unlaid, or the line stretched to the cell: a cell-sized hole is the seam/ring bug.
    if b[2] <= 0.5 || b[2] > 8.0 || b[3] <= 0.5 {
        return;
    }
    crate::app::damage::push_frame_paint_key(crate::app::damage::paint_element_key(line), b);
}

/// Slot box expanded by the SDF shadow the quad actually paints.
fn push_slot_ring_node(el: &Element, pad: f32) -> bool {
    let b = crate::core::tree::visual_paint_aabb(el, el.paint_scroll_offset());
    if b[2] <= 0.5 || b[3] <= 0.5 || pad <= 0.0 {
        return false;
    }
    crate::app::damage::push_frame_paint_key(
        crate::app::damage::paint_element_key(el),
        [b[0] - pad, b[1] - pad, b[2] + pad * 2.0, b[3] + pad * 2.0],
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

#[expect(
    clippy::too_many_arguments,
    reason = "The existing rendering adapter keeps its independent geometry, state and callback parameters"
)]
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
    let focused = use_signal("__jadix_button_focus_id", String::new).peek();
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
        if el.attrs.get("jadix_z_paint_only").map(|s| s.as_str()) != Some("1") {
            el.set_attr("jadix_z_paint_only", "1");
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
        if el.attrs.remove("jadix_z_paint_only").is_some() {
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
    el.set_attr(
        "jadix_otp_caret_wake",
        if want_caret { "true" } else { "false" },
    );
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
        el.set_attr("jadix_halo_dirty", "1");
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
    while !el.attrs.contains_key(ATTR_SLOT)
        && el.children.len() == 1
        && el.text.as_ref().is_none_or(|t| t.is_empty())
        && el.on_click.is_none()
        && el.focus_id.is_none()
        && el.attrs.get("jadix_fragment").map(|s| s.as_str()) != Some("1")
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
    root.set_attr(crate::authoring::focus::ATTR_BLUR_ON_OUTSIDE_PRESS, "true");
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
                restart_caret_blink();
                use_signal("__jadix_button_focus_id", String::new).set(fid.clone());
                use_signal("__jadix_focus_visible", || false).set(true);
                crate::app::runtime::request_redraw();
            })
            .on_key_down(move |ev| {
                let handled = handle_otp_key(&ev, &ctx_key);
                if handled {
                    restart_caret_blink();
                }
                handled
            })
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
            g.set_attr("jadix_fragment", "1");
            frag_kids.push(InputOTPGroup(InputOTPGroupProps {
                children: Some(g.children(std::mem::take(&mut group_kids))),
                base: None,
            }));
            frag_kids.push(InputOTPSeparator(InputOTPSeparatorProps {
                children: None,
                base: None,
            }));
        }
        group_kids.push(InputOTPSlot(InputOTPSlotProps {
            index: Some(i as i32),
            children: None,
            base: None,
        }));
    }
    if !group_kids.is_empty() {
        let mut g = Element::new();
        g.set_attr("jadix_fragment", "1");
        frag_kids.push(InputOTPGroup(InputOTPGroupProps {
            children: Some(g.children(group_kids)),
            base: None,
        }));
    }
    let mut frag = Element::new();
    frag.set_attr("jadix_fragment", "1");
    frag.children(frag_kids)
}

// ── RSX parts ────────────────────────────────────────────────────────────────

crate::__component_contract! {
InputOTPProps [];
/// 根：焦点壳 + 上下文；子节点为 Group / Separator / Slot。
#[allow(non_snake_case)]
pub fn InputOTP(
identity [] id: String,
plain [] max_length: Option<i32>,
plain [] value: Option<Signal<String>>,
default [] disabled: bool,
default [] invalid: bool,
plain [] pattern: Option<String>,
plain [] children: Option<Element>,
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
}

crate::__component_contract! {
InputOTPGroupProps [];
/// 连续槽容器（`gap-0`；子 Slot 拼成连体条）。
#[allow(non_snake_case)]
pub fn InputOTPGroup(
plain [] children: Option<Element>,
) -> Element {
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
}

crate::__component_contract! {
InputOTPSlotProps [];
/// 单字符槽。`index` 从 0 起。
///
/// 默认均匀边框；[`InputOTPGroup`] 写入 `jadix_otp_edge` / 圆角 / paint-nudge（对位 shadcn
/// `first:` / `last:` + ButtonGroup 连体描边）。内容与焦点态由 `reactive_sync` 每帧同步。
#[allow(non_snake_case)]
pub fn InputOTPSlot(
plain [] index: Option<i32>,
plain [] children: Option<Element>,
) -> Element {
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
    let focused = use_signal("__jadix_button_focus_id", String::new).peek();
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
        cell.set_attr("jadix_z_paint_only", "1");
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
            restart_caret_blink();
            active_click.set(index);
            use_signal("__jadix_button_focus_id", String::new).set(focus_key.clone());
            use_signal("__jadix_focus_visible", || false).set(true);
            crate::app::runtime::request_redraw();
        });
    }

    let sync_fid = fid;
    cell = cell.reactive_sync(move |el, layout| {
        let status = sync_otp_slot_paint(
            el, index, max_length, value_sig, active_sig, &sync_fid, disabled, invalid,
        );
        if el.attrs.get("jadix_otp_caret_wake").map(String::as_str) == Some("true") {
            layout.request_caret_blink();
        }
        status
    });

    cell
}
}

crate::__component_contract! {
InputOTPSeparatorProps [];
/// 段间分隔（Minus 图标）。
#[allow(non_snake_case)]
pub fn InputOTPSeparator(
plain [] children: Option<Element>,
) -> Element {
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

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/input_otp.rs"]
mod tests;
