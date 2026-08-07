//! # Input
//!
//! 单行文本输入：IME、选区、caret、placeholder、invalid 态，以及 HTML 风格的 [`InputType`]。
//! 值与光标状态按 `id` 隔离。
//!
//! ## 用法
//! ```ignore
//! <Input id="email" type="email" />
//! <Input id="pw" type="password" placeholder="密码" />
//! <Input id="n" type="number" />
//! <Input id="q" type="search" />
//! ```
//!
//! ## `type`（对齐 HTML `<input type>` 文本族）
//! | 值 | 行为 |
//! |----|------|
//! | `text`（默认） | 明文 |
//! | `password` | 显示为 `•`，底层仍存原文 |
//! | `number` | 仅允许数字、可选 `-` / `.`；非法非空 → 自动 invalid |
//! | `tel` | 过滤电话字符；非空格式失败 → 自动 invalid |
//! | `email` / `url` | 宽松字符过滤；非空格式失败 → 自动 invalid |
//! | `search` | 同 text；空 placeholder 默认 `Search…` |
//!
//! 选择器类（color / file / date / …）见独立占位组件，不进本 `type`。
//!
//! ## 注意
//! - **必须稳定唯一 `id`**。
//! - `focus_id` 为 `input:{id}`。
//! - 显式 `invalid=true` 与类型校验叠加。

use std::sync::atomic::{AtomicU64, Ordering};

use crate::core::reactivity::use_signal;
use crate::core::tree::{
    parse_color, parse_text_color, Element, InputPaintSync, IntoDimension, IntoF32, LayoutStatus,
};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, BoxBuilder};
use crate::style::InputCaretDraw;
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use rui_macros::component;

/// `focus_id` 前缀；与 `Button` 的普通 id 区分。
pub const INPUT_FOCUS_PREFIX: &str = "input:";
/// 全局：当前会话是否收到过 IME `Enabled`（用于区分直输与组字）。
pub const IME_ACTIVE_SIGNAL: &str = "__rui_ime_active";

/// 密码掩码字符（与多数桌面 UI 一致）。
const PASSWORD_MASK: char = '•';

pub fn input_value_signal_key(id: &str) -> String {
    format!("__rui_input_value_{}", id)
}
pub fn input_preedit_signal_key(id: &str) -> String {
    format!("__rui_input_preedit_{}", id)
}
pub fn input_preedit_sel_signal_key(id: &str) -> String {
    format!("__rui_input_preedit_sel_{}", id)
}
pub fn input_caret_signal_key(id: &str) -> String {
    format!("__rui_input_caret_{}", id)
}
pub fn input_selection_signal_key(id: &str) -> String {
    format!("__rui_input_selection_{}", id)
}
pub fn input_overwrite_signal_key(id: &str) -> String {
    format!("__rui_input_overwrite_{}", id)
}
pub fn input_ime_anchor_signal_key(id: &str) -> String {
    format!("__rui_input_ime_anchor_{}", id)
}
pub fn input_type_signal_key(id: &str) -> String {
    format!("__rui_input_type_{}", id)
}

/// 仅参与 cosmic/glyphon 排版，不计入 `Element::text`；行尾薄空格使拉丁度量参与行盒，减轻纯中文 IME 拼音阶段的垂直抖动。
///
/// 仅在聚焦编辑时挂到 `text_shape_suffix`：失焦且纯 ASCII 走 Basic shaping 时该字位易缺字形，会被画成「乱码框」。
pub const INPUT_LINE_SHAPE_SUFFIX: &str = "\u{2009}";

pub(crate) fn focus_id_for_input(id: &str) -> String {
    format!("{INPUT_FOCUS_PREFIX}{id}")
}

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

/// 按字段 `type` 过滤插入文本。
pub fn filter_insert_for_type(ty: InputType, existing: &str, insert: &str) -> String {
    match ty {
        InputType::Number => filter_number_insert(existing, insert),
        InputType::Tel => filter_tel_insert(existing, insert),
        InputType::Email => filter_email_insert(existing, insert),
        InputType::Url => filter_url_insert(existing, insert),
        InputType::Text | InputType::Password | InputType::Search => insert.to_string(),
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

fn resolve_placeholder(explicit: &str, ty: InputType) -> String {
    if explicit.is_empty() {
        ty.default_placeholder().to_string()
    } else {
        explicit.to_string()
    }
}

fn effective_invalid(explicit: bool, ty: InputType, value: &str) -> bool {
    explicit || !ty.validate_value(value)
}

fn mask_password(s: &str) -> String {
    s.chars().map(|_| PASSWORD_MASK).collect()
}

/// 将「明文 display」上的字节下标映射到掩码串上的字节下标（按 Unicode 标量一一对应）。
fn map_byte_index_to_masked(real: &str, real_byte: usize, masked: &str) -> usize {
    let byte = real.floor_char_boundary(real_byte.min(real.len()));
    let chars = real[..byte].chars().count();
    masked
        .char_indices()
        .nth(chars)
        .map(|(i, _)| i)
        .unwrap_or(masked.len())
}

fn map_byte_range_to_masked(real: &str, lo: usize, hi: usize, masked: &str) -> (usize, usize) {
    (
        map_byte_index_to_masked(real, lo, masked),
        map_byte_index_to_masked(real, hi, masked),
    )
}

trait ColorExt {
    fn with_alpha(self, alpha: f32) -> [f32; 4];
}
impl ColorExt for [f32; 4] {
    fn with_alpha(mut self, alpha: f32) -> [f32; 4] {
        self[3] = alpha;
        self
    }
}

pub struct InputBuilder {
    id: String,
    container: BoxBuilder,
    placeholder: String,
    disabled: bool,
    invalid: bool,
    input_type: InputType,
    text_size_px: f32,
    font_weight: u16,
    line_height_px: Option<f32>,
    leading_scale: Option<f32>,
}

/// RSX `<Input>`：见模块文档。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Input(
    id: String,
    placeholder: String,
    disabled: bool,
    invalid: bool,
    input_type: Option<String>,
    text_size: Option<f32>,
    mut on_click: crate::core::tree::Handler,
    mut on_key_down: crate::core::tree::KeyHandler,
    children: Option<Element>,
) -> Element {
    let mut builder = input(&id)
        .placeholder(placeholder)
        .disabled(disabled)
        .invalid(invalid);
    if let Some(t) = input_type {
        builder = builder.input_type(t);
    }

    if let Some(s) = text_size {
        builder = builder.text_size(s);
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
            .items_center(),
        placeholder: String::new(),
        disabled: false,
        invalid: false,
        input_type: InputType::Text,
        // shadcn/ui Input：`text-sm`；类名通常不含 font-medium → 400。
        text_size_px: 0.875 * 16.0,
        font_weight: 400,
        line_height_px: None,
        leading_scale: None,
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
    pub fn w(mut self, width: impl IntoDimension) -> Self {
        self.container = self.container.w(width);
        self
    }
    pub fn h(mut self, height: impl IntoDimension) -> Self {
        self.container = self.container.h(height);
        self
    }
    pub fn rounded(mut self, radius: impl IntoF32) -> Self {
        self.container = self.container.rounded(radius);
        self
    }
    pub fn flex_grow(mut self, grow: impl crate::core::tree::IntoF32Raw) -> Self {
        self.container = self.container.flex_grow(grow);
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
            builder.id.clone()
        };

        let focus_key = "__rui_button_focus_id";
        let focus_visible_key = "__rui_focus_visible";
        let focused = use_signal(focus_key, String::new);
        let _ = use_signal(focus_visible_key, || false);

        let fid = focus_id_for_input(&field_id);
        // `peek`：不把焦点/输入值订阅到根 `create_effect`，避免点击输入框、切焦点等触发整树 `rebuild_ui`
        //（会破坏 `<Reactive>` 占位协调）。外观由 `sync_input_paint_subtree` 每帧按 `peek` 刷新。
        //
        // Caret / selection follow DOM focus (`focused == fid`), not `:focus-visible`.
        // Mouse-down clears `__rui_focus_visible` before `on_click`; requiring it here made the
        // caret and selection highlight disappear while the field still accepted typing.
        let is_focused = focused.peek() == fid && !builder.disabled;

        let value_sig = use_signal(&input_value_signal_key(&field_id), String::new);
        let preedit_sig = use_signal(&input_preedit_signal_key(&field_id), String::new);
        let preedit_sel_sig = use_signal(&input_preedit_sel_signal_key(&field_id), || {
            Option::<(usize, usize)>::None
        });
        let caret_sig = use_signal(&input_caret_signal_key(&field_id), || None::<usize>);
        let text_sel_sig = use_signal(&input_selection_signal_key(&field_id), || {
            None::<(usize, usize)>
        });
        let overwrite_sig = use_signal(&input_overwrite_signal_key(&field_id), || false);
        let ime_anchor_sig = use_signal(&input_ime_anchor_signal_key(&field_id), || None::<usize>);

        let committed = value_sig.peek();
        let preedit = preedit_sig.peek();
        let preedit_sel = preedit_sel_sig.peek();
        let ime_anchor = ime_anchor_sig.peek();

        let placeholder = resolve_placeholder(&builder.placeholder, builder.input_type);
        let invalid = effective_invalid(builder.invalid, builder.input_type, &committed);

        let show_placeholder = committed.is_empty() && preedit.is_empty();
        let real_display = if show_placeholder {
            placeholder.clone()
        } else if preedit.is_empty() {
            committed.clone()
        } else {
            let mut a = ime_anchor.unwrap_or(committed.len()).min(committed.len());
            a = committed.floor_char_boundary(a);
            format!("{}{}{}", &committed[..a], preedit, &committed[a..])
        };

        let caret_in_real = if is_focused {
            if show_placeholder {
                0
            } else if !preedit.is_empty() {
                let mut a = ime_anchor.unwrap_or(committed.len()).min(committed.len());
                a = committed.floor_char_boundary(a);
                let end = preedit_sel
                    .map(|(_, e)| e)
                    .unwrap_or_else(|| preedit.len())
                    .min(preedit.len());
                a + preedit.floor_char_boundary(end)
            } else {
                let len = real_display.len();
                let ix = text_sel_sig
                    .peek()
                    .and_then(|(a, b)| {
                        if a.min(b) < a.max(b) {
                            Some(a.max(b))
                        } else {
                            None
                        }
                    })
                    .or_else(|| caret_sig.peek())
                    .unwrap_or(len);
                real_display.floor_char_boundary(ix.min(len))
            }
        } else {
            0
        };

        let mask = builder.input_type.is_password() && !show_placeholder;
        let display_text = if mask {
            mask_password(&real_display)
        } else {
            real_display.clone()
        };
        let caret_byte = if mask {
            map_byte_index_to_masked(&real_display, caret_in_real, &display_text)
        } else {
            caret_in_real
        };

        let mut text_color = if show_placeholder {
            parse_text_color("muted-foreground")
        } else {
            parse_text_color("foreground")
        };
        if builder.disabled {
            text_color[3] = (text_color[3] as f32 * 0.5) as u8;
        }

        let mut border_c = if invalid {
            parse_color("destructive")
        } else if is_focused {
            parse_color("border-ring")
        } else {
            parse_color("input")
        };
        let mut bg = parse_color("transparent");
        if builder.disabled {
            border_c[3] *= 0.5;
            bg[3] *= 0.5;
        }

        let mut container = builder.container.bg(bg).border(border_c, 1.0);
        if is_focused {
            let shadow_color = if invalid {
                parse_color("destructive").with_alpha(0.4)
            } else {
                crate::style::focus_ring_color()
            };
            container = container.shadow(shadow_color, crate::style::FOCUS_RING_BLUR);
        } else if !builder.disabled {
            container = container.shadow([0.0, 0.0, 0.0, 0.06], 1.0);
        }

        let mut text_el = build_text_node(
            display_text.clone(),
            TextNodeConfig {
                size_px: builder.text_size_px,
                weight: builder.font_weight,
                color: text_color,
                underline: false,
                underline_offset_px: None,
                center: false,
                line_height_px: builder.line_height_px,
                leading_scale: builder.leading_scale,
                font_family: None,
            },
        )
        .w("100%");
        text_el.text_shape_suffix = if is_focused {
            Some(INPUT_LINE_SHAPE_SUFFIX.to_string())
        } else {
            None
        };
        text_el.template_id = Some(format!("__rui_input:{}:text", field_id));

        if is_focused {
            let mut pc = parse_color("primary");
            pc[3] *= 0.35;
            let selection_bg = [
                (pc[0] * 255.0) as u8,
                (pc[1] * 255.0) as u8,
                (pc[2] * 255.0) as u8,
                (pc[3] * 255.0) as u8,
            ];
            let sel_draw = if preedit.is_empty() {
                text_sel_sig.peek().and_then(|(a, b)| {
                    if a.min(b) < a.max(b) {
                        let lo = a.min(b);
                        let hi = a.max(b);
                        Some(if mask {
                            map_byte_range_to_masked(&real_display, lo, hi, &display_text)
                        } else {
                            (lo, hi)
                        })
                    } else {
                        None
                    }
                })
            } else {
                None
            };
            let overwrite_highlight =
                if overwrite_sig.peek() && preedit.is_empty() && sel_draw.is_none() {
                    let ix =
                        real_display.floor_char_boundary(caret_in_real.min(real_display.len()));
                    if ix < real_display.len() {
                        let end = ix + real_display[ix..].chars().next().unwrap().len_utf8();
                        Some(if mask {
                            map_byte_range_to_masked(&real_display, ix, end, &display_text)
                        } else {
                            (ix, end)
                        })
                    } else {
                        None
                    }
                } else {
                    None
                };
            text_el.text_style.input_caret = Some(InputCaretDraw {
                caret_byte_index: caret_byte,
                selection: sel_draw,
                selection_bg,
                overwrite_highlight,
            });
        }

        let mut el: Element = container.into();
        el = if builder.disabled {
            el.cursor_not_allowed()
        } else {
            el.cursor_text()
        };

        let type_key = input_type_signal_key(&field_id);
        use_signal(&type_key, || builder.input_type.as_str().to_string())
            .set(builder.input_type.as_str().to_string());

        let sync = InputPaintSync {
            base_id: field_id.clone(),
            placeholder,
            invalid: builder.invalid,
            disabled: builder.disabled,
            text_size_px: builder.text_size_px,
            font_weight: builder.font_weight,
            line_height_px: builder.line_height_px,
            leading_scale: builder.leading_scale,
            input_type: builder.input_type.as_str().to_string(),
        };

        let disabled = builder.disabled;
        let id_for_click = fid.clone();
        let mut out: Element = interactive(el.child(text_el))
            .focus_id(fid)
            .focusable(!disabled)
            .on_click(move || {
                if !disabled {
                    use_signal(focus_key, String::new).set(id_for_click.clone());
                    use_signal(focus_visible_key, || false).set(true);
                }
            })
            .into();
        out.template_id = Some(field_id);
        out.input_paint_sync = Some(sync);
        out
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
        crate::core::tree::CursorKind::NotAllowed
    } else {
        crate::core::tree::CursorKind::Text
    });
    LayoutStatus::StyleChanged
}

/// 每帧将 Input 的显示状态与全局/每字段信号对齐（全部 `peek`，不触发根 `rebuild_ui`）。
pub(crate) fn sync_input_paint_subtree(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    if let Some(desc) = el.input_paint_sync.clone() {
        sync_one_input_paint(el, &desc, layout_tree, impact);
    }
    for child in &mut el.children {
        sync_input_paint_subtree(child, layout_tree, impact);
    }
}

fn sync_one_input_paint(
    el: &mut Element,
    desc: &InputPaintSync,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    let focus_key = "__rui_button_focus_id";
    let focused = use_signal(focus_key, String::new);
    let fid = focus_id_for_input(&desc.base_id);
    // Same as builder path: caret/selection use DOM focus, not `:focus-visible`.
    let is_focused = focused.peek() == fid && !desc.disabled;

    let committed = use_signal(&input_value_signal_key(&desc.base_id), String::new).peek();
    let preedit = use_signal(&input_preedit_signal_key(&desc.base_id), String::new).peek();
    let preedit_sel = use_signal(&input_preedit_sel_signal_key(&desc.base_id), || {
        Option::<(usize, usize)>::None
    })
    .peek();
    let caret_sig = use_signal(&input_caret_signal_key(&desc.base_id), || None::<usize>);
    let text_sel_sig = use_signal(&input_selection_signal_key(&desc.base_id), || {
        None::<(usize, usize)>
    });
    let overwrite_sig = use_signal(&input_overwrite_signal_key(&desc.base_id), || false);
    let ime_anchor_sig = use_signal(
        &input_ime_anchor_signal_key(&desc.base_id),
        || None::<usize>,
    );
    let ime_anchor = ime_anchor_sig.peek();

    let show_placeholder = committed.is_empty() && preedit.is_empty();
    let input_ty = InputType::parse(&desc.input_type);
    let placeholder = resolve_placeholder(&desc.placeholder, input_ty);
    let invalid = effective_invalid(desc.invalid, input_ty, &committed);
    let real_display = if show_placeholder {
        placeholder
    } else if preedit.is_empty() {
        committed.clone()
    } else {
        let mut a = ime_anchor.unwrap_or(committed.len()).min(committed.len());
        a = committed.floor_char_boundary(a);
        format!("{}{}{}", &committed[..a], preedit, &committed[a..])
    };

    let mask = input_ty.is_password() && !show_placeholder;

    let mut text_color = if show_placeholder {
        parse_text_color("muted-foreground")
    } else {
        parse_text_color("foreground")
    };
    if desc.disabled {
        text_color[3] = (text_color[3] as f32 * 0.5) as u8;
    }

    let mut border_c = if invalid {
        parse_color("destructive")
    } else if is_focused {
        parse_color("border-ring")
    } else {
        parse_color("input")
    };
    let mut bg = parse_color("transparent");
    if desc.disabled {
        border_c[3] *= 0.5;
        bg[3] *= 0.5;
    }

        let (want_shadow_color, want_shadow_blur) = if is_focused {
            let sc = if invalid {
                parse_color("destructive").with_alpha(0.4)
            } else {
                crate::style::focus_ring_color()
            };
            (sc, crate::style::FOCUS_RING_BLUR)
        } else if !desc.disabled {
            ([0.0, 0.0, 0.0, 0.06], 1.0)
        } else {
            ([0.0, 0.0, 0.0, 0.0], 0.0)
        };

    let caret_in_real = if is_focused {
        if show_placeholder {
            0
        } else if !preedit.is_empty() {
            let mut a = ime_anchor.unwrap_or(committed.len()).min(committed.len());
            a = committed.floor_char_boundary(a);
            let end = preedit_sel
                .map(|(_, e)| e)
                .unwrap_or_else(|| preedit.len())
                .min(preedit.len());
            a + preedit.floor_char_boundary(end)
        } else {
            let len = real_display.len();
            let ix = text_sel_sig
                .peek()
                .and_then(|(a, b)| {
                    if a.min(b) < a.max(b) {
                        Some(a.max(b))
                    } else {
                        None
                    }
                })
                .or_else(|| caret_sig.peek())
                .unwrap_or(len);
            real_display.floor_char_boundary(ix.min(len))
        }
    } else {
        0
    };

    let display_text = if mask {
        mask_password(&real_display)
    } else {
        real_display.clone()
    };
    let caret_byte = if mask {
        map_byte_index_to_masked(&real_display, caret_in_real, &display_text)
    } else {
        caret_in_real
    };

    let mut container_layout = false;
    let mut container_style = false;

    if el.style.border_color != border_c {
        el.style.border_color = border_c;
        container_style = true;
    }
    if el.style.bg_color != bg {
        el.style.bg_color = bg;
        container_style = true;
    }
    if (el.style.shadow_blur - want_shadow_blur).abs() > 0.01
        || (el.style.shadow_color[0] - want_shadow_color[0]).abs() > 0.001
        || (el.style.shadow_color[1] - want_shadow_color[1]).abs() > 0.001
        || (el.style.shadow_color[2] - want_shadow_color[2]).abs() > 0.001
        || (el.style.shadow_color[3] - want_shadow_color[3]).abs() > 0.001
    {
        el.style.shadow_color = want_shadow_color;
        el.style.shadow_blur = want_shadow_blur;
        container_layout = true;
    }

    let mut text_layout = false;
    let mut text_style = false;

    if el.children.is_empty() {
        if container_layout {
            layout_tree.update_taffy_node_style(el);
            *impact = impact.merge(LayoutStatus::LayoutChanged);
        } else if container_style {
            layout_tree.update_taffy_node_style(el);
            *impact = impact.merge(LayoutStatus::StyleChanged);
        }
        return;
    }

    {
        let text_el = &mut el.children[0];
        if text_el.text.as_deref() != Some(display_text.as_str()) {
            text_el.text = Some(display_text.clone());
            text_layout = true;
        }
        let want_suffix = if is_focused {
            Some(INPUT_LINE_SHAPE_SUFFIX.to_string())
        } else {
            None
        };
        if text_el.text_shape_suffix != want_suffix {
            text_el.text_shape_suffix = want_suffix;
            text_layout = true;
        }
        let want_tid = Some(format!("__rui_input:{}:text", desc.base_id));
        if text_el.template_id != want_tid {
            text_el.template_id = want_tid;
            text_layout = true;
        }
        if text_el.text_style.color != text_color {
            text_el.text_style.color = text_color;
            text_style = true;
        }

        let caret_draw = if is_focused {
            let mut pc = parse_color("primary");
            pc[3] *= 0.35;
            let selection_bg = [
                (pc[0] * 255.0) as u8,
                (pc[1] * 255.0) as u8,
                (pc[2] * 255.0) as u8,
                (pc[3] * 255.0) as u8,
            ];
            let sel_draw = if preedit.is_empty() {
                text_sel_sig.peek().and_then(|(a, b)| {
                    if a.min(b) < a.max(b) {
                        let lo = a.min(b);
                        let hi = a.max(b);
                        Some(if mask {
                            map_byte_range_to_masked(&real_display, lo, hi, &display_text)
                        } else {
                            (lo, hi)
                        })
                    } else {
                        None
                    }
                })
            } else {
                None
            };
            let overwrite_highlight =
                if overwrite_sig.peek() && preedit.is_empty() && sel_draw.is_none() {
                    let ix =
                        real_display.floor_char_boundary(caret_in_real.min(real_display.len()));
                    if ix < real_display.len() {
                        let end = ix + real_display[ix..].chars().next().unwrap().len_utf8();
                        Some(if mask {
                            map_byte_range_to_masked(&real_display, ix, end, &display_text)
                        } else {
                            (ix, end)
                        })
                    } else {
                        None
                    }
                } else {
                    None
                };
            Some(InputCaretDraw {
                caret_byte_index: caret_byte,
                selection: sel_draw,
                selection_bg,
                overwrite_highlight,
            })
        } else {
            None
        };

        if text_el.text_style.input_caret != caret_draw {
            text_el.text_style.input_caret = caret_draw;
            // Caret/selection is paint chrome — StyleChanged, not flex LayoutChanged.
            text_style = true;
        }
    }

    if container_layout || container_style {
        layout_tree.update_taffy_node_style(el);
    }
    if let Some(text_el) = el.children.get_mut(0) {
        if text_el.layout_node.is_some() && (text_layout || text_style) {
            layout_tree.update_taffy_node_style(text_el);
        }
    }

    let layout_hit = container_layout || text_layout;
    let style_hit = !layout_hit && (container_style || text_style);
    if layout_hit {
        *impact = impact.merge(LayoutStatus::LayoutChanged);
    } else if style_hit {
        *impact = impact.merge(LayoutStatus::StyleChanged);
    }
}

impl crate::core::tree::IntoElementList for InputBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}
