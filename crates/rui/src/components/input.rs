//! # Input
//!
//! 单行文本输入的 **样式层**：IME、选区、caret、placeholder、invalid，
//! 以及 HTML 风格的 [`InputType`]。信号键 / 受控绑定 / focus 前缀见 [`crate::utils::text_field`]。
//! 值与光标状态按 `id` 隔离。
//!
//! ## 用法
//! ```ignore
//! <Input id="email" type="email" clearable=true />
//! <Input id="pw" type="password" password_toggle=true clearable=true placeholder="密码" />
//! <Input id="n" type="number" number_steppers=true />
//! <Input id="q" type="search" clearable=true />
//! ```
//!
//! ## `type`（对齐 HTML `<input type>` 文本族）
//! | 值 | 行为 |
//! |----|------|
//! | `text`（默认） | 明文 |
//! | `password` | 显示为 `•`，底层仍存原文；**禁用 IME**、仅 ASCII；可选 `password_toggle` |
//! | `number` | 仅允许数字、可选 `-` / `.`；非法非空 → 自动 invalid；可选 `number_steppers` 增减触发器 |
//! | `tel` | 过滤电话字符；非空格式失败 → 自动 invalid |
//! | `email` / `url` | 宽松字符过滤；非空格式失败 → 自动 invalid |
//! | `search` | 同 text；空 placeholder 默认 `Search…` |
//!
//! ## 可选尾饰
//! - `clearable`：有内容时显示叉号，点击清空（缺省 `false`）。
//! - 编辑快捷键（Ctrl/Cmd）：全选 A、复制 C、粘贴 V、剪切 X；Shift+方向键扩展选区。
//! - `password_toggle`：仅 `type=password`；切换掩码 / 明文（缺省 `false`）。
//! - `number_steppers`：仅 `type=number`；右侧上下增减触发器（solid-ui `NumberField` 风格）；`step` 默认 `1`；可长按连发（缺省 `false`）。
//!
//! 选择器类（color / file / date / …）见独立占位组件，不进本 `type`。
//!
//! ## 注意
//! - **必须稳定唯一 `id`**。
//! - `focus_id` 为 `input:{id}`。
//! - 显式 `invalid=true` 与类型校验叠加。
//! - 单行编辑默认 [`crate::style::TextOverflow::Hidden`]（原生 `<input>`：裁切溢出，光标/选区可滚出）。
//!   `truncate` / `text-overflow: ellipsis|clip|newline` 用于 Label、Button 等不可编辑文本，**不会**作用在 Input 上。

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::app::pointer_events::{EventPhase, PointerEvent};
use crate::components::internal::text_edit::{build_text_edit_element, TextEditBuildParams};
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::{parse_text_color, Element, IntoCssDim, IntoF32, LayoutStatus};
use crate::primitives::r#box::{hstack, r#box, BoxBuilder};
use crate::utils::lucide::lucide_glyph;
use crate::utils::text_field::commit_value;
use crate::utils::text_node::build_icon_text_node;
use rui_macros::component;
use winit::event::MouseButton;

pub use crate::utils::text_field::{
    bind_controlled_value, input_caret_signal_key, input_focus_id as focus_id_for_input,
    input_ime_anchor_signal_key, input_overwrite_signal_key, input_password_reveal_signal_key,
    input_preedit_sel_signal_key, input_preedit_signal_key, input_selection_signal_key,
    input_type_signal_key, input_value_signal_key, INPUT_FOCUS_PREFIX,
};

pub(crate) use crate::utils::text_field::{
    input_clear_template_id, input_number_dec_template_id, input_number_inc_template_id,
    input_reveal_template_id, input_text_template_id,
};

/// 长按步进：首次按下后延迟再开始连发。
const NUMBER_STEP_HOLD_DELAY: Duration = Duration::from_millis(400);
/// 长按连发间隔。
const NUMBER_STEP_HOLD_INTERVAL: Duration = Duration::from_millis(50);

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
pub const IME_ACTIVE_SIGNAL: &str = "__rui_ime_active";

/// 密码掩码字符（与多数桌面 UI 一致）。
const PASSWORD_MASK: char = '•';

/// 仅参与 cosmic/glyphon 排版，不计入 `Element::text`；行尾薄空格使拉丁度量参与行盒，减轻纯中文 IME 拼音阶段的垂直抖动。
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

/// 过滤 `type="email"`：本地部分与域名常用字符（不含空白）。
pub fn filter_email_insert(_existing: &str, insert: &str) -> String {
    insert
        .chars()
        .filter(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '%' | '+' | '-' | '!')
        })
        .collect()
}

/// 过滤 `type="url"`：可打印 ASCII，去掉空白与控制符。
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
    let ime_anchor = use_signal(&input_ime_anchor_signal_key(rest), || None::<usize>).peek();
    let real_display = if preedit.is_empty() {
        committed
    } else {
        let mut a = ime_anchor.unwrap_or(committed.len()).min(committed.len());
        a = committed.floor_char_boundary(a);
        format!("{}{}{}", &committed[..a], preedit, &committed[a..])
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
    real_display.floor_char_boundary(ix.min(real_display.len()))
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

pub struct InputBuilder {
    id: String,
    container: BoxBuilder,
    placeholder: String,
    disabled: bool,
    invalid: bool,
    input_type: InputType,
    clearable: bool,
    password_toggle: bool,
    /// `type=number` 时显示增减触发器。
    number_steppers: bool,
    /// `type=number` 步进量；默认 `1`。
    step: f64,
    text_size_px: f32,
    font_weight: u16,
    line_height_px: Option<f32>,
    leading_scale: Option<f32>,
    value: Option<Signal<String>>,
    /// CSS `text-align`（默认 `start`）。
    text_align: crate::style::TextAlign,
}

/// RSX `<Input>`：见模块文档。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Input(
    id: String,
    placeholder: String,
    disabled: bool,
    invalid: bool,
    clearable: bool,
    password_toggle: bool,
    number_steppers: bool,
    input_type: Option<String>,
    step: Option<f32>,
    text_size: Option<f32>,
    value: Option<Signal<String>>,
    truncate: bool,
    text_overflow: Option<String>,
    mut on_click: crate::core::tree::Handler,
    mut on_key_down: crate::core::tree::KeyHandler,
    children: Option<Element>,
) -> Element {
    let mut builder = input(&id)
        .placeholder(placeholder)
        .disabled(disabled)
        .invalid(invalid)
        .clearable(clearable)
        .password_toggle(password_toggle)
        .number_steppers(number_steppers);
    let _ = (truncate, text_overflow);
    if let Some(t) = input_type {
        builder = builder.input_type(t);
    }
    if let Some(s) = step {
        builder = builder.step(s);
    }

    if let Some(s) = text_size {
        builder = builder.text_size(s);
    }

    if let Some(sig) = value {
        builder = builder.value_signal(sig);
    }

    if let Some(c) = children {
        builder = builder.child(c);
    }

    let mut el: Element = builder.into();
    if !disabled {
        let mut old_click = el.on_click.clone();
        el = el.on_click(move || {
            if let Some(ref mut rc) = old_click {
                rc.borrow_mut()();
            }
            (on_click.0)(());
        });

        let mut old_key = el.on_key_down.clone();
        el = el.on_key_down(move |ev| {
            let mut handled = false;
            if let Some(ref mut rc) = old_key {
                handled = rc.borrow_mut()(ev.clone());
            }
            if handled {
                return true;
            }
            (on_key_down.0)(ev)
        });
    }

    el
}

pub fn input(id: &str) -> InputBuilder {
    InputBuilder {
        id: id.to_string(),
        container: hstack()
            .h("2.25rem")
            .w("100%")
            .min_w("0")
            .rounded("0.375rem")
            .px("0.75rem")
            .py("0.25rem")
            .items_center()
            .overflow("hidden"),
        placeholder: String::new(),
        disabled: false,
        invalid: false,
        input_type: InputType::Text,
        clearable: false,
        password_toggle: false,
        number_steppers: false,
        step: 1.0,
        // shadcn/ui Input：`text-sm`；类名通常不含 font-medium → 400。
        text_size_px: 0.875 * 16.0,
        font_weight: 400,
        line_height_px: None,
        leading_scale: None,
        value: None,
        text_align: crate::style::TextAlign::Start,
    }
}

impl InputBuilder {
    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = s.into();
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
    pub fn invalid(mut self, v: bool) -> Self {
        self.invalid = v;
        self
    }
    /// HTML `type`：[`InputType`] 或 `"password"` / `"number"` 等字符串。
    pub fn input_type(mut self, t: impl Into<InputType>) -> Self {
        self.input_type = t.into();
        self
    }
    /// 有内容时显示清除按钮。
    pub fn clearable(mut self, v: bool) -> Self {
        self.clearable = v;
        self
    }
    /// `type=password` 时显示睁眼/闭眼切换。
    pub fn password_toggle(mut self, v: bool) -> Self {
        self.password_toggle = v;
        self
    }
    /// `type=number` 时显示右侧上下增减触发器。
    pub fn number_steppers(mut self, v: bool) -> Self {
        self.number_steppers = v;
        self
    }
    /// `type=number` 增减步进；≤0 时回退为 `1`。
    pub fn step(mut self, v: impl IntoF32) -> Self {
        let s = v.into_f32() as f64;
        self.step = if s.is_finite() && s > 0.0 { s } else { 1.0 };
        self
    }
    pub fn w(mut self, width: impl IntoCssDim) -> Self {
        self.container = self.container.w(width);
        self
    }
    pub fn h(mut self, height: impl IntoCssDim) -> Self {
        self.container = self.container.h(height);
        self
    }
    pub fn rounded(mut self, radius: impl IntoF32) -> Self {
        self.container = self.container.rounded(radius);
        self
    }
    pub fn flex_grow(mut self, grow: impl crate::core::tree::IntoF32Raw) -> Self {
        // Default Input is `w(100%)` for solo fields. In a row with siblings (e.g. Button),
        // that percentage claims the full line and can crush neighbors to an empty outline.
        // Opting into flex_grow means share free space instead of forcing full width.
        self.container = self.container.flex_grow(grow).w("auto").min_w("0");
        self
    }
    pub fn flex_shrink(mut self, shrink: impl crate::core::tree::IntoF32Raw) -> Self {
        self.container = self.container.flex_shrink(shrink);
        self
    }
    pub fn text_size(mut self, size: impl IntoF32) -> Self {
        self.text_size_px = size.into_f32();
        self
    }

    pub fn text_line_height(mut self, px: impl IntoF32) -> Self {
        self.line_height_px = Some(px.into_f32());
        self.leading_scale = None;
        self
    }
    pub fn text_leading(mut self, scale: impl IntoF32) -> Self {
        self.leading_scale = Some(scale.into_f32());
        self.line_height_px = None;
        self
    }
    pub fn text_leading_none(self) -> Self {
        self.text_leading(1.0)
    }
    pub fn text_leading_tight(self) -> Self {
        self.text_leading(1.25)
    }
    pub fn text_leading_normal(self) -> Self {
        self.text_leading(1.5)
    }
    pub fn text_leading_relaxed(self) -> Self {
        self.text_leading(1.625)
    }
    /// 受控值：见 [`crate::utils::text_field::bind_controlled_value`]。
    pub fn value_signal(mut self, s: Signal<String>) -> Self {
        self.value = Some(s);
        self
    }
    /// CSS `text-align`（如 `"center"` / `"start"` / `"justify"`）。
    pub fn text_align(mut self, align: impl AsRef<str>) -> Self {
        self.text_align = crate::style::TextAlign::parse(align.as_ref());
        self
    }
    /// 覆盖默认水平内边距（默认 `0.75rem`）。
    pub fn px(mut self, p: impl IntoCssDim) -> Self {
        self.container = self.container.px(p);
        self
    }
    /// Ignored: single-line Input always uses [`crate::style::TextOverflow::Hidden`].
    /// Use [`crate::components::button::ButtonBuilder::truncate`] / Label for ellipsis.
    pub fn truncate(self, on: bool) -> Self {
        let _ = on;
        self
    }
    /// Ignored: single-line Input always uses [`crate::style::TextOverflow::Hidden`].
    pub fn text_overflow(self, _mode: impl Into<crate::style::TextOverflow>) -> Self {
        self
    }
    pub fn font_weight(mut self, weight: impl IntoF32) -> Self {
        self.font_weight = weight.into_f32().round() as u16;
        self
    }

    pub fn child(self, _child: impl Into<Element>) -> Self {
        // Input currently does not support arbitrary children
        self
    }

    pub fn children(self, _children: impl IntoIterator<Item = Element>) -> Self {
        self
    }
}

pub(crate) fn lucide_named(name: &str) -> String {
    lucide_glyph(name).unwrap_or_else(|| "?".into())
}

pub(crate) fn input_adornment_button(
    template_id: String,
    glyph: String,
    disabled: bool,
    on_click: impl FnMut() + 'static,
) -> Element {
    let icon = build_icon_text_node(glyph)
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

/// solid-ui `NumberFieldIncrement/DecrementTrigger`：`size-4` 上下箭头。
pub(crate) fn input_number_step_button(
    template_id: String,
    glyph: String,
    disabled: bool,
    field_id: String,
    focus_id: String,
    delta: f64,
) -> Element {
    let icon = build_icon_text_node(glyph)
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
            use_signal("__rui_button_focus_id", String::new).set(fid_down.clone());
            use_signal("__rui_focus_visible", || false).set(true);
            begin_number_step_hold(id_down.clone(), delta);
            crate::app::set_pointer_capture(capture_id.clone());
            e.stop_propagation();
        })
        .on_pointer_up(move |_e: &PointerEvent| {
            end_number_step_hold();
        })
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

pub(crate) fn set_adornment_icon_glyph(btn: &mut Element, glyph: &str) -> bool {
    let Some(icon) = btn.children.first_mut() else {
        return false;
    };
    if icon.text.as_deref() == Some(glyph)
        && icon.text_style.font_family.as_deref() == Some("lucide")
    {
        return false;
    }
    icon.text = Some(glyph.to_string());
    icon.text_style.font_family = Some("lucide".into());
    true
}

impl From<InputBuilder> for Element {
    fn from(builder: InputBuilder) -> Element {
        // 空 id 会使 value/caret 等信号键相同，多个 Input 会共用一份状态；RSX 宏会为省略 id 的标签生成唯一名，此处兜底手写 `input("")` 等路径。
        let field_id = if builder.id.is_empty() {
            static RT_EMPTY_INPUT_ID: AtomicU64 = AtomicU64::new(0);
            format!(
                "__rui_input_rt_{}",
                RT_EMPTY_INPUT_ID.fetch_add(1, Ordering::Relaxed)
            )
        } else {
            builder.id
        };

        build_text_edit_element(TextEditBuildParams {
            field_id,
            placeholder: builder.placeholder,
            disabled: builder.disabled,
            invalid: builder.invalid,
            input_type: builder.input_type,
            clearable: builder.clearable,
            password_toggle: builder.password_toggle,
            number_steppers: builder.number_steppers,
            step: builder.step,
            text_size_px: builder.text_size_px,
            font_weight: builder.font_weight,
            line_height_px: builder.line_height_px,
            leading_scale: builder.leading_scale,
            value: builder.value,
            text_align: builder.text_align,
            single_line_container: Some(builder.container),
            multiline: false,
            rows: 1,
            resizable: false,
            resize_max_h: 0.0,
            wrap_mode: crate::utils::text_field::TextareaWrapMode::Word,
        })
    }
}

/// Solid-style in-place bind for `<Input invalid={sig.get()}>`.
pub fn apply_bound_input_invalid(el: &mut Element, invalid: bool) -> LayoutStatus {
    let Some(sync) = el.input_paint_sync.as_mut() else {
        return LayoutStatus::Clean;
    };
    if sync.invalid == invalid {
        return LayoutStatus::Clean;
    }
    sync.invalid = invalid;
    LayoutStatus::StyleChanged
}

/// Solid-style in-place bind for `<Input disabled={sig.get()}>`.
pub fn apply_bound_input_disabled(el: &mut Element, disabled: bool) -> LayoutStatus {
    let Some(sync) = el.input_paint_sync.as_mut() else {
        return LayoutStatus::Clean;
    };
    if sync.disabled == disabled {
        return LayoutStatus::Clean;
    }
    sync.disabled = disabled;
    el.focusable = !disabled;
    el.cursor = Some(if disabled {
        crate::core::tree::CursorKind::NotAllowed.into()
    } else {
        crate::core::tree::CursorKind::Text.into()
    });
    LayoutStatus::StyleChanged
}

impl crate::core::tree::IntoElementList for InputBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_disallows_ime_and_filters_to_ascii() {
        assert!(!InputType::Password.allows_ime());
        assert!(InputType::Text.allows_ime());
        assert_eq!(filter_password_insert("", "Ab1!你'好\n\u{7f}"), "Ab1!'");
        assert_eq!(
            filter_insert_for_type(InputType::Password, "", "pāss字"),
            "pss"
        );
    }

    #[test]
    fn text_type_strips_escape_and_other_controls() {
        assert_eq!(
            filter_insert_for_type(InputType::Text, "", "a\u{1b}b\0c\u{7f}"),
            "abc"
        );
        assert_eq!(
            filter_insert_for_type(InputType::Search, "", "hi\u{1b}"),
            "hi"
        );
        assert_eq!(
            filter_insert_for_type(InputType::Text, "", "骄傲\u{1b}3"),
            "骄傲3"
        );
        assert_eq!(
            filter_insert_for_type(InputType::Text, "", "line\nkeep"),
            "line\nkeep"
        );
    }

    #[test]
    fn input_always_uses_hidden_overflow_even_if_truncate_requested() {
        let el: Element = input("t-ov-hidden")
            .truncate(true)
            .text_overflow("ellipsis")
            .into();
        fn find_text(el: &Element) -> Option<&Element> {
            if el.text.is_some() {
                return Some(el);
            }
            el.children.iter().find_map(find_text)
        }
        let leaf = find_text(&el).expect("input text leaf");
        assert_eq!(leaf.text_style.overflow, crate::style::TextOverflow::Hidden);
        assert!(leaf.text_style.overflow.allows_caret_scroll());
    }

    #[test]
    fn input_value_signal_seeds_store() {
        let sig = use_signal("__rui_wp38_input_val", || "hello".to_string());
        let _el: Element = input("wp38-in").value_signal(sig).into();
        assert_eq!(
            use_signal(&input_value_signal_key("wp38-in"), String::new).peek(),
            "hello"
        );
        sig.set("world".into());
        let pulled =
            crate::utils::text_field::pull_controlled_into_store("wp38-in", "hello", false);
        assert_eq!(pulled.as_deref(), Some("world"));
    }

    #[test]
    fn number_step_formats_and_commits() {
        let id = "num-step-fmt";
        use_signal(&input_value_signal_key(id), String::new).set("3".into());
        step_number_value(id, 1.0);
        assert_eq!(
            use_signal(&input_value_signal_key(id), String::new).peek(),
            "4"
        );
        step_number_value(id, -2.0);
        assert_eq!(
            use_signal(&input_value_signal_key(id), String::new).peek(),
            "2"
        );
        assert_eq!(format_number_step_result(1.5, 0.5), "1.5");
        assert_eq!(format_number_step_result(2.0, 1.0), "2");
    }

    #[test]
    fn number_input_builds_inc_dec_triggers() {
        let id = "num-step-ui";
        use_signal(&input_value_signal_key(id), String::new).set("0".into());
        let el: Element = input(id)
            .input_type(InputType::Number)
            .number_steppers(true)
            .into();
        let inc_tid = input_number_inc_template_id(id);
        let dec_tid = input_number_dec_template_id(id);
        fn find_tid<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
            if el.template_id.as_deref() == Some(tid) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_tid(c, tid))
        }
        let inc = find_tid(&el, &inc_tid).expect("inc trigger");
        let dec = find_tid(&el, &dec_tid).expect("dec trigger");
        assert!(inc.on_mousedown.is_some());
        assert!(dec.on_mousedown.is_some());
        // Fire down handler once via the stored callback.
        let mut pev = crate::app::pointer_events::PointerEvent::new(
            0.0,
            0.0,
            winit::event::MouseButton::Left,
            1,
        );
        pev.phase = crate::app::pointer_events::EventPhase::Target;
        (*inc.on_mousedown.as_ref().unwrap().borrow_mut())(&pev);
        assert_eq!(
            use_signal(&input_value_signal_key(id), String::new).peek(),
            "1"
        );
        end_number_step_hold();
        (*dec.on_mousedown.as_ref().unwrap().borrow_mut())(&pev);
        assert_eq!(
            use_signal(&input_value_signal_key(id), String::new).peek(),
            "0"
        );
        end_number_step_hold();
    }

    #[test]
    fn number_steppers_opt_in_only() {
        let id = "num-step-opt";
        let el: Element = input(id).input_type(InputType::Number).into();
        fn find_tid(el: &Element, tid: &str) -> bool {
            if el.template_id.as_deref() == Some(tid) {
                return true;
            }
            el.children.iter().any(|c| find_tid(c, tid))
        }
        assert!(!find_tid(&el, &input_number_inc_template_id(id)));
        assert!(!find_tid(&el, &input_number_dec_template_id(id)));
    }
}
