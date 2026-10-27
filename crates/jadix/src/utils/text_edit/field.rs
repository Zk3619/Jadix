use crate::app::pointer_events::{EventPhase, PointerEvent};
use crate::core::reactivity::use_signal;
use crate::core::tree::{parse_text_color, Element};
use crate::primitives::r#box::{r#box, vstack};
use crate::utils::icon_provider::glyph_for_name as lucide_glyph;
use crate::utils::text_field::*;
use crate::utils::text_node::build_lucide_icon_text_node;
use std::cell::RefCell;
use std::time::{Duration, Instant};
use winit::event::MouseButton;

/// 长按步进：首次按下后延迟再开始连发。
const NUMBER_STEP_HOLD_DELAY: Duration = Duration::from_millis(400);
/// Long press the burst interval.
const NUMBER_STEP_HOLD_INTERVAL: Duration = Duration::from_millis(50);
/// 增减触发器竖向间距（略负值，收紧 chevron 视觉间隙）。
const NUMBER_STEPPER_STACK_GAP: &str = "-0.125rem";

struct NumberStepHold {
    field_id: String,
    delta: f64,
    started: Instant,
    last_fire: Instant,
}

thread_local! {
    static NUMBER_STEP_HOLD: RefCell<Option<NumberStepHold>> = const { RefCell::new(None) };
}

/// 全局：当前会话是否收到过 IME `Enabled`（用于区分直输与组字）。
pub const IME_ACTIVE_SIGNAL: &str = "__jadix_ime_active";

/// Password mask characters (consistent with most desktop UIs).
const PASSWORD_MASK: char = '•';

/// Only participates in cosmic/glyphon typesetting, not included in `Element::text`; thin spaces at the end of lines enable Latin metrics to participate in line boxes, reducing vertical jitter in the pinyin stage of pure Chinese IME.
///
/// Historical shaping probe (U+2009). No longer attached on focus: `input_caret` already
/// forces Advanced shaping, and the thin space pulled Latin fallbacks that shifted CJK baselines.
#[allow(dead_code)]
pub const INPUT_LINE_SHAPE_SUFFIX: &str = "\u{2009}";

/// HTML `<input type="…">` 的文本族子集。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputType {
    #[default]
    Text,
    Password,
    Email,
    Number,
    Search,
    Tel,
    Url,
}

impl InputType {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "password" => Self::Password,
            "email" => Self::Email,
            "number" => Self::Number,
            "search" => Self::Search,
            "tel" => Self::Tel,
            "url" => Self::Url,
            _ => Self::Text,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Password => "password",
            Self::Email => "email",
            Self::Number => "number",
            Self::Search => "search",
            Self::Tel => "tel",
            Self::Url => "url",
        }
    }

    pub fn is_password(self) -> bool {
        matches!(self, Self::Password)
    }

    pub fn is_number(self) -> bool {
        matches!(self, Self::Number)
    }

    pub fn is_tel(self) -> bool {
        matches!(self, Self::Tel)
    }

    /// Whether OS IME (CJK composition) is allowed while this field is focused.
    /// Password fields force ASCII and must not open a candidate window.
    pub fn allows_ime(self) -> bool {
        !matches!(self, Self::Password)
    }

    /// 空 placeholder 时的默认提示文案。
    pub fn default_placeholder(self) -> &'static str {
        match self {
            Self::Email => "name@example.com",
            Self::Tel => "Phone number",
            Self::Url => "https://",
            Self::Search => "Search…",
            Self::Password => "Password",
            Self::Number => "0",
            Self::Text => "",
        }
    }

    /// 非空值是否通过本类型的宽松格式校验（空串视为通过）。
    pub fn validate_value(self, value: &str) -> bool {
        let v = value.trim();
        if v.is_empty() {
            return true;
        }
        match self {
            Self::Email => validate_email(v),
            Self::Tel => validate_tel(v),
            Self::Url => validate_url(v),
            Self::Number => validate_number(v),
            Self::Text | Self::Password | Self::Search => true,
        }
    }
}

impl From<&str> for InputType {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for InputType {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

/// 过滤 `type="number"` 可插入文本：数字、至多一个前导 `-`、至多一个 `.`。
pub fn filter_number_insert(existing: &str, insert: &str) -> String {
    let mut out = String::new();
    let mut has_dot = existing.contains('.');
    let allow_minus_at_start = existing.is_empty();
    for (i, ch) in insert.chars().enumerate() {
        match ch {
            '0'..='9' => out.push(ch),
            '.' if !has_dot => {
                has_dot = true;
                out.push(ch);
            }
            '-' if allow_minus_at_start
                && i == 0
                && out.is_empty()
                && !existing.starts_with('-') =>
            {
                out.push(ch);
            }
            _ => {}
        }
    }
    out
}

/// 过滤 `type="tel"`：数字与常见电话符号。
pub fn filter_tel_insert(_existing: &str, insert: &str) -> String {
    insert
        .chars()
        .filter(|c| matches!(c, '0'..='9' | '+' | '-' | ' ' | '(' | ')' | '.' | '*'))
        .collect()
}

/// Filter `type="email"`: common characters in local part and domain name (excluding spaces).
pub fn filter_email_insert(_existing: &str, insert: &str) -> String {
    insert
        .chars()
        .filter(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '%' | '+' | '-' | '!')
        })
        .collect()
}

/// Filter `type="url"`: print ASCII, remove whitespace and control characters.
pub fn filter_url_insert(_existing: &str, insert: &str) -> String {
    insert.chars().filter(|c| c.is_ascii_graphic()).collect()
}

/// 过滤 `type="password"`：仅非控制 ASCII（禁用 IME 后的兜底）。
pub fn filter_password_insert(_existing: &str, insert: &str) -> String {
    insert
        .chars()
        .filter(|c| c.is_ascii() && !c.is_ascii_control())
        .collect()
}

/// 去掉 Cc 控制符（Esc `U+001B`、NUL、TAB、CR、DEL…）。保留 `\n`（Textarea / 粘贴多行）。
pub fn strip_control_insert(insert: &str) -> String {
    insert
        .chars()
        .filter(|&c| c == '\n' || !c.is_control())
        .collect()
}

/// 按字段 `type` 过滤插入文本。
pub fn filter_insert_for_type(ty: InputType, existing: &str, insert: &str) -> String {
    match ty {
        InputType::Number => filter_number_insert(existing, insert),
        InputType::Tel => filter_tel_insert(existing, insert),
        InputType::Email => filter_email_insert(existing, insert),
        InputType::Url => filter_url_insert(existing, insert),
        InputType::Password => filter_password_insert(existing, insert),
        InputType::Text | InputType::Search => strip_control_insert(insert),
    }
}

fn validate_email(v: &str) -> bool {
    let Some((local, domain)) = v.split_once('@') else {
        return false;
    };
    if local.is_empty() || domain.is_empty() || local.contains(' ') || domain.contains(' ') {
        return false;
    }
    if !domain.contains('.') || domain.starts_with('.') || domain.ends_with('.') {
        return false;
    }
    local
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-' | '!'))
        && domain
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
}

fn validate_tel(v: &str) -> bool {
    let digits = v.chars().filter(|c| c.is_ascii_digit()).count();
    digits >= 3
        && v.chars()
            .all(|c| matches!(c, '0'..='9' | '+' | '-' | ' ' | '(' | ')' | '.' | '*'))
}

fn validate_url(v: &str) -> bool {
    let lower = v.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("ftp://")
    {
        return v.len() > 8 && !v.chars().any(|c| c.is_whitespace());
    }
    if v.contains(' ') || !v.contains('.') {
        return false;
    }
    let host = v.split('/').next().unwrap_or(v);
    host.contains('.')
        && !host.starts_with('.')
        && !host.ends_with('.')
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
}

fn validate_number(v: &str) -> bool {
    let mut chars = v.chars().peekable();
    if chars.peek() == Some(&'-') {
        chars.next();
    }
    let rest: String = chars.collect();
    if rest.is_empty() {
        return false;
    }
    let mut seen_dot = false;
    for c in rest.chars() {
        match c {
            '0'..='9' => {}
            '.' if !seen_dot => seen_dot = true,
            _ => return false,
        }
    }
    true
}

/// 将步进后的数值格式化为输入框字符串（整步优先整数，否则去掉尾随零）。
pub fn format_number_step_result(value: f64, step: f64) -> String {
    if !value.is_finite() {
        return String::new();
    }
    let step_is_int = step.is_finite() && (step - step.round()).abs() < 1e-9;
    let value_is_int = (value - value.round()).abs() < 1e-9;
    if step_is_int && value_is_int {
        format!("{}", value.round() as i64)
    } else {
        let s = format!("{value:.10}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// `type=number` 按 `delta`（通常为 ±step）增减当前值并写回 store / 受控 Signal。
pub fn step_number_value(field_id: &str, delta: f64) {
    if !delta.is_finite() || delta == 0.0 {
        return;
    }
    let store = use_signal(&input_value_signal_key(field_id), String::new);
    let raw = store.peek();
    let current = raw.trim().parse::<f64>().unwrap_or(0.0);
    let next = current + delta;
    let text = format_number_step_result(next, delta.abs());
    if text == raw {
        return;
    }
    #[cfg(feature = "text-undo")]
    crate::utils::text_edit_history::will_user_edit_field(
        field_id,
        crate::utils::text_edit_history::EditKind::Step,
    );
    commit_value(field_id, text.clone());
    use_signal(&input_preedit_signal_key(field_id), String::new).set(String::new());
    use_signal(&input_preedit_sel_signal_key(field_id), || {
        None::<(usize, usize)>
    })
    .set(None);
    use_signal(&input_selection_signal_key(field_id), || {
        None::<(usize, usize)>
    })
    .set(None);
    use_signal(&input_ime_anchor_signal_key(field_id), || None::<usize>).set(None);
    use_signal(&input_caret_signal_key(field_id), || None::<usize>).set(Some(text.len()));
    crate::utils::text_field::clear_extra_carets(field_id);
    #[cfg(feature = "text-undo")]
    crate::utils::text_edit_history::note_field_after_edit(field_id);
}

fn begin_number_step_hold(field_id: String, delta: f64) {
    let now = Instant::now();
    NUMBER_STEP_HOLD.with(|h| {
        *h.borrow_mut() = Some(NumberStepHold {
            field_id,
            delta,
            started: now,
            last_fire: now,
        });
    });
}

/// 结束增减触发器的长按连发（指针抬起 / 失焦）。
pub(crate) fn end_number_step_hold() {
    NUMBER_STEP_HOLD.with(|h| {
        *h.borrow_mut() = None;
    });
    crate::app::release_pointer_capture();
}

/// 推进长按连发；有步进时返回 `true`（需重绘）。
pub(crate) fn tick_number_step_hold() -> bool {
    NUMBER_STEP_HOLD.with(|h| {
        let mut slot = h.borrow_mut();
        let Some(hold) = slot.as_mut() else {
            return false;
        };
        let now = Instant::now();
        if now.duration_since(hold.started) < NUMBER_STEP_HOLD_DELAY {
            return false;
        }
        if now.duration_since(hold.last_fire) < NUMBER_STEP_HOLD_INTERVAL {
            return false;
        }
        hold.last_fire = now;
        let id = hold.field_id.clone();
        let delta = hold.delta;
        drop(slot);
        step_number_value(&id, delta);
        true
    })
}

/// 下一次长按连发唤醒时刻。
pub(crate) fn number_step_hold_next_wake_at() -> Option<Instant> {
    NUMBER_STEP_HOLD.with(|h| {
        let slot = h.borrow();
        let hold = slot.as_ref()?;
        let after_delay = hold.started + NUMBER_STEP_HOLD_DELAY;
        let now = Instant::now();
        if now < after_delay {
            Some(after_delay)
        } else {
            Some(hold.last_fire + NUMBER_STEP_HOLD_INTERVAL)
        }
    })
}

pub(crate) fn resolve_placeholder(explicit: &str, ty: InputType) -> String {
    if explicit.is_empty() {
        ty.default_placeholder().to_string()
    } else {
        explicit.to_string()
    }
}

pub(crate) fn effective_invalid(explicit: bool, ty: InputType, value: &str) -> bool {
    explicit || !ty.validate_value(value)
}

pub(crate) fn mask_password(s: &str) -> String {
    s.chars().map(|_| PASSWORD_MASK).collect()
}

/// 将「明文 display」上的字节下标映射到掩码串上的字节下标（按 Unicode 标量一一对应）。
pub(crate) fn map_byte_index_to_masked(real: &str, real_byte: usize, masked: &str) -> usize {
    let byte = real.floor_char_boundary(real_byte.min(real.len()));
    let chars = real[..byte].chars().count();
    masked
        .char_indices()
        .nth(chars)
        .map(|(i, _)| i)
        .unwrap_or(masked.len())
}

/// 将掩码 display 上的字节下标映射回明文（按 Unicode 标量一一对应）。
pub fn map_masked_byte_index_to_real(real: &str, masked_byte: usize) -> usize {
    let masked = mask_password(real);
    let byte = masked.floor_char_boundary(masked_byte.min(masked.len()));
    let chars = masked[..byte].chars().count();
    real.char_indices()
        .nth(chars)
        .map(|(i, _)| i)
        .unwrap_or(real.len())
}

/// Hit-test on the painted Input string → caret index in committed/real display space.
///
/// Password fields paint `•` while the caret signal stays in plaintext bytes; without this
/// remap, clicking / dragging stores masked indices and toggling reveal misplaces the caret.
pub fn caret_index_from_display_hit(rest: &str, display_ix: usize) -> usize {
    let committed = use_signal(&input_value_signal_key(rest), String::new).peek();
    let preedit = use_signal(&input_preedit_signal_key(rest), String::new).peek();
    if committed.is_empty() && preedit.is_empty() {
        return 0;
    }
    let anchors = crate::utils::text_field::field_compose_anchors(rest, &committed);
    let real_display = if preedit.is_empty() {
        committed.clone()
    } else {
        crate::utils::text_multi_cursor::splice_preedit(&committed, &anchors, &preedit)
    };
    let ty = InputType::parse(
        &use_signal(&input_type_signal_key(rest), || {
            InputType::Text.as_str().to_string()
        })
        .peek(),
    );
    let reveal = use_signal(&input_password_reveal_signal_key(rest), || false).peek();
    let ix = if ty.is_password() && !reveal {
        map_masked_byte_index_to_real(&real_display, display_ix)
    } else {
        display_ix
    };
    if preedit.is_empty() {
        real_display.floor_char_boundary(ix.min(real_display.len()))
    } else {
        crate::utils::text_multi_cursor::unsplice_display_hit(&committed, &anchors, &preedit, ix)
    }
}

pub(crate) fn map_byte_range_to_masked(
    real: &str,
    lo: usize,
    hi: usize,
    masked: &str,
) -> (usize, usize) {
    (
        map_byte_index_to_masked(real, lo, masked),
        map_byte_index_to_masked(real, hi, masked),
    )
}

pub fn lucide_named(name: &str) -> String {
    lucide_glyph(name).unwrap_or_else(|| "?".into())
}

pub fn input_adornment_button(
    template_id: String,
    icon_name: &str,
    disabled: bool,
    on_click: impl FnMut() + 'static,
) -> Element {
    let icon = build_lucide_icon_text_node(icon_name)
        .text_size(14.0)
        .text_color(parse_text_color("muted-foreground"));
    let mut btn: Element = r#box()
        .flex_shrink(0.0)
        .w("1.25rem")
        .h("1.25rem")
        .min_w("1.25rem")
        .items_center()
        .justify_center()
        .rounded("0.25rem")
        .into();
    btn = btn.child(icon);
    btn.template_id = Some(template_id);
    if disabled {
        btn.cursor_not_allowed()
    } else {
        btn.cursor_kind(crate::core::tree::CursorKind::Pointer)
            .on_click(on_click)
    }
}

/// solid-ui `NumberFieldIncrement/DecrementTrigger`: `size-4` Up and down arrows.
pub(crate) fn input_number_step_button(
    template_id: String,
    icon_name: &str,
    disabled: bool,
    field_id: String,
    focus_id: String,
    delta: f64,
) -> Element {
    let icon = build_lucide_icon_text_node(icon_name)
        .text_size(16.0)
        .text_color(parse_text_color("muted-foreground"))
        .pointer_events_none();
    let mut btn: Element = r#box()
        .flex_shrink(0.0)
        .w("1rem")
        .h("1rem")
        .min_w("1rem")
        .items_center()
        .justify_center()
        .into();
    btn = btn.child(icon);
    btn.template_id = Some(template_id.clone());
    btn.focus_id = Some(template_id.clone());
    if disabled {
        return btn.cursor_not_allowed();
    }
    let capture_id = template_id;
    let id_down = field_id;
    let fid_down = focus_id;
    btn.cursor_kind(crate::core::tree::CursorKind::Pointer)
        .on_pointer_down(move |e: &PointerEvent| {
            if e.button != MouseButton::Left {
                return;
            }
            if e.phase == EventPhase::Capture {
                return;
            }
            step_number_value(&id_down, delta);
            use_signal("__jadix_button_focus_id", String::new).set(fid_down.clone());
            use_signal("__jadix_focus_visible", || false).set(true);
            begin_number_step_hold(id_down.clone(), delta);
            crate::app::set_pointer_capture(capture_id.clone());
            e.stop_propagation();
        })
        .on_pointer_up(move |_e: &PointerEvent| {
            end_number_step_hold();
        })
}

/// 上下增减触发器列（solid-ui `NumberField` 右侧）。
pub(crate) fn input_number_steppers_column(inc: Element, dec: Element) -> Element {
    vstack()
        .flex_shrink(0.0)
        .items_center()
        .justify_center()
        .gap(NUMBER_STEPPER_STACK_GAP)
        .child(inc)
        .child(dec)
        .into()
}

pub(crate) fn set_adornment_visible(btn: &mut Element, visible: bool) -> bool {
    let mut changed = false;
    let want_opacity = if visible { 1.0 } else { 0.0 };
    if (btn.style.opacity - want_opacity).abs() > 0.001 {
        btn.style.opacity = want_opacity;
        changed = true;
    }
    if visible {
        if btn.pointer_events_mode() != crate::core::tree::PointerEvents::Auto {
            btn.attrs.remove(crate::core::tree::ATTR_POINTER_EVENTS);
            changed = true;
        }
    } else if btn.pointer_events_mode() != crate::core::tree::PointerEvents::None {
        btn.set_attr(
            crate::core::tree::ATTR_POINTER_EVENTS,
            crate::core::tree::PointerEvents::None.as_str(),
        );
        changed = true;
    }
    // Keep a stable 1.25rem slot so clear/eye gap never jumps when visibility flips.
    let w = taffy::style::Dimension::length(1.25 * 16.0);
    if btn.style.size.width != w {
        btn.style.size.width = w;
        btn.style.min_size.width = w;
        changed = true;
    }
    changed
}

pub(crate) fn set_adornment_icon_name(btn: &mut Element, name: &str) -> bool {
    let Some(icon) = btn.children.first_mut() else {
        return false;
    };
    let glyph = lucide_glyph(name).unwrap_or_else(|| "?".into());
    if icon.text.as_deref() == Some(glyph.as_str())
        && icon.text_style.font_family.as_deref() == Some("lucide")
        && icon
            .get_attr(crate::utils::ATTR_ICON_NAME)
            .map(String::as_str)
            == Some(name)
    {
        return false;
    }
    icon.text = Some(glyph);
    icon.text_style.font_family = Some("lucide".into());
    icon.set_attr(crate::utils::ATTR_ICON_NAME, name);
    true
}
