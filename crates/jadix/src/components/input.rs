//! # Input
//!
//! 单行文本输入的 **样式层**：IME、选区、caret、placeholder、invalid，
//! and HTML-style [`InputType`]. See [`crate::utils::text_field`] for signal key / controlled binding / focus prefixes.
//! 值与光标状态按 `id` 隔离。
//!
//! ## Usage
//! ```ignore
//! <Input id="email" type="email" clearable=true />
//! <Input id="pw" type="password" password_toggle=true clearable=true placeholder="password" />
//! <Input id="n" type="number" number_steppers=true />
//! <Input id="q" type="search" clearable=true />
//! <Input id="accent" caret_color="#111" selection_background="selection-background" selection_foreground="selection-foreground" />
//! ```
//!
//! ## `type`（对齐 HTML `<input type>` 文本族）
//! | value | behavior |
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
//! - Editing shortcut keys (Ctrl/Cmd): Select all A, copy C, paste V, cut X, undo Z, redo Y or Shift+Z; Shift + arrow keys to expand the selection. Tab into the field selects all.
//! - **Alt+click** adds extra carets (click an extra caret again to drop it). Alt+drag across lines places one caret per hard line at the press column. Typing / Backspace / Delete / arrows apply to every caret. Esc or a click without Alt returns to a single caret.
//! - `password_toggle`: `type=password` only; toggle mask/plaintext (default `false`).
//! - `number_steppers`：仅 `type=number`；右侧上下增减触发器（solid-ui `NumberField` 风格）；`step` 默认 `1`；可长按连发（缺省 `false`）。
//!
//! ## Caret / 选区色
//! - `caret_color`：CSS `caret-color`。默认主题 token `caret`（与 `foreground` 同值），**不**跟 placeholder 的 muted 色。
//! - `selection_background`：选区高亮填充，按传入色原样使用（含 alpha）。默认主题 token
//!   `selection-background`（别名 `selection`）。
//! - `selection_foreground`：选中文字前景色（CSS `::selection { color }`），按传入色原样使用。
//!   默认主题 token `selection-foreground`，与 `selection-background` 成对相反。
//!   Builder 与 RSX chrome（`is_base`）均可设；会转发进 [`crate::core::tree::TextEditPaintSync`]。
//!
//! ## 字号 / 焦点环
//! - `text_size`：容器 `text_style` + 内层 `input_paint_sync.text_size_px`（`class("text-lg")` 同路径）。
//! - `ring_width`：粗细（逻辑 px；Hard 为 spread，Soft 为 blur）。默认主题 [`crate::style::focus_ring_width`]（3）。
//! - `ring_color`：颜色。默认主题 token `ring`。也可用 `class("ring-2 ring-primary")`。
//!   均走 Element chrome，不进 `#[component]` 签名。
//!
//! 选择器类（color / file / date / …）见独立占位组件，不进本 `type`。
//!
//! ## Notes
//! - **必须稳定唯一 `id`**。
//! - `focus_id` is `input:{id}`.
//! - Explicit `invalid=true` overlaps with type checking.
//! - 单行编辑默认 [`crate::style::TextOverflow::Hidden`]（原生 `<input>`：裁切溢出，光标/选区可滚出）。
//!   `truncate` / `text-overflow: ellipsis|clip|newline` 用于 Label、Button 等不可编辑文本，**不会**作用在 Input 上。
//!
//! ## 细粒度（N.7）
//!
//! - 受控 [`InputBuilder::value_signal`]：paint 用 `peek` + [`crate::utils::text_field::pull_controlled_into_store`]。
//!   **焦点内**或 **IME 合成未提交**时，外部 `set` 不覆盖 store、不抢光标 / 选区。
//! - Caret、选区、焦点环走 [`crate::core::tree::TextEditPaintSync`]（每帧 peek），不是 `bind_text` / `bind_class`。
//!   Hover chrome 同理：[`crate::core::tree::ReactiveSync`] / paint peek，禁止每帧新建 bind。
//! - 禁止用叶子 bind 驱动 caret；禁止每键击重建 Input 子树。

use std::sync::atomic::{AtomicU64, Ordering};

use crate::components::base::SlotChrome;
use crate::utils::text_edit::{build_text_edit_element, TextEditBuildParams};
use crate::core::reactivity::Signal;
#[cfg(test)]
use crate::core::reactivity::use_signal;
use crate::core::tree::{
    Element, IntoCssDim, IntoF32, IntoNodeSize, LayoutStatus,
};
use crate::primitives::r#box::{hstack, BoxBuilder};
use jadix_macros::component;

pub use crate::utils::text_field::{
    bind_controlled_value, input_caret_signal_key, input_extra_carets_signal_key,
    input_focus_id as focus_id_for_input, input_ime_anchor_signal_key, input_overwrite_signal_key,
    input_password_reveal_signal_key, input_preedit_marks_signal_key, input_preedit_sel_signal_key,
    input_preedit_signal_key, input_selection_signal_key, input_type_signal_key,
    input_value_signal_key, INPUT_FOCUS_PREFIX,
};

#[cfg(test)]
use crate::utils::text_field::{input_number_dec_template_id, input_number_inc_template_id};

pub use crate::utils::text_edit::field::{IME_ACTIVE_SIGNAL, INPUT_LINE_SHAPE_SUFFIX, InputType, filter_number_insert, filter_tel_insert, filter_email_insert, filter_url_insert, filter_password_insert, strip_control_insert, filter_insert_for_type, format_number_step_result, step_number_value, map_masked_byte_index_to_real, caret_index_from_display_hit};
pub(crate) use crate::utils::text_edit::field::{lucide_named, input_adornment_button};
#[cfg(test)]
use crate::utils::text_edit::field::end_number_step_hold;

pub struct InputBuilder {
    id: String,
    container: BoxBuilder,
    placeholder: String,
    disabled: bool,
    invalid: bool,
    input_type: InputType,
    clearable: bool,
    password_toggle: bool,
    /// Show step triggers when `type=number`.
    number_steppers: bool,
    /// `type=number` 步进量；默认 `1`。
    step: f64,
    text_size_px: f32,
    font_weight: u16,
    line_height_px: Option<f32>,
    leading_scale: Option<f32>,
    value: Option<Signal<String>>,
    /// CSS `text-align` (default `start`).
    text_align: crate::style::TextAlign,
    /// CSS `caret-color`. `None` → theme token `caret`.
    caret_color: Option<[u8; 4]>,
    /// Selection highlight. `None` → theme token `selection-background`.
    selection_background: Option<[u8; 4]>,
    /// Selected glyph color. `None` → theme token `selection-foreground`.
    selection_foreground: Option<[u8; 4]>,
    selection_on_blur: crate::utils::text_field::SelectionOnBlur,
    /// 内嵌于 InputAdornment / Command：无自身边框与焦点环。
    plain_chrome: bool,
    chrome: SlotChrome,
    #[cfg(feature = "text-undo")]
    undo: crate::utils::text_edit_history::TextUndoConfig,
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
    value: Option<Signal<String>>,
    truncate: bool,
    text_overflow: Option<String>,
    selection_on_blur: Option<String>,
    undo_enabled: Option<bool>,
    max_undo_entries: Option<u32>,
    max_undo_memory: Option<u32>,
    typing_merge_timeout_ms: Option<u32>,
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
    if let Some(policy) = selection_on_blur {
        builder = builder.selection_on_blur(policy);
    }
    if let Some(t) = input_type {
        builder = builder.input_type(t);
    }
    if let Some(s) = step {
        builder = builder.step(s);
    }

    if let Some(sig) = value {
        builder = builder.value_signal(sig);
    }
    #[cfg(feature = "text-undo")]
    {
        if let Some(v) = undo_enabled {
            builder = builder.undo_enabled(v);
        }
        if let Some(n) = max_undo_entries {
            builder = builder.max_undo_entries(n as usize);
        }
        if let Some(n) = max_undo_memory {
            builder = builder.max_undo_memory(n as usize);
        }
        if let Some(ms) = typing_merge_timeout_ms {
            builder = builder.typing_merge_timeout_ms(ms as u64);
        }
    }
    #[cfg(not(feature = "text-undo"))]
    let _ = (
        undo_enabled,
        max_undo_entries,
        max_undo_memory,
        typing_merge_timeout_ms,
    );

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
        // shadcn/ui Input: `text-sm`; class names usually do not contain font-medium → 400.
        text_size_px: 0.875 * 16.0,
        font_weight: 400,
        line_height_px: None,
        leading_scale: None,
        value: None,
        text_align: crate::style::TextAlign::Start,
        caret_color: None,
        selection_background: None,
        selection_foreground: None,
        selection_on_blur: crate::utils::text_field::SelectionOnBlur::Clear,
        plain_chrome: false,
        chrome: SlotChrome::default(),
        #[cfg(feature = "text-undo")]
        undo: crate::utils::text_edit_history::TextUndoConfig::input_default(),
    }
}

impl InputBuilder {
    crate::impl_slot_chrome_methods!();

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
    /// `type=number` increases or decreases step; when ≤0, it falls back to `1`.
    pub fn step(mut self, v: impl IntoF32) -> Self {
        let s = v.into_f32() as f64;
        self.step = if s.is_finite() && s > 0.0 { s } else { 1.0 };
        self
    }
    #[track_caller]
    pub fn w(mut self, width: impl IntoNodeSize) -> Self {
        self.container = self.container.w(width);
        self
    }
    #[track_caller]
    pub fn h(mut self, height: impl IntoNodeSize) -> Self {
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
        self.container = self
            .container
            .flex_grow(grow)
            .w("auto")
            .min_w("0")
            .flex_basis(0.0);
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
    pub fn text_leading(mut self, scale: impl crate::core::tree::IntoF32Raw) -> Self {
        self.leading_scale = Some(scale.into_f32_raw());
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
    /// Controlled value: paint peek + [`crate::utils::text_field::bind_controlled_value`].
    /// While focused or IME composing, an external `set` does not steal caret (N.7).
    pub fn value_signal(mut self, s: Signal<String>) -> Self {
        self.value = Some(s);
        self
    }

    #[cfg(feature = "text-undo")]
    pub fn undo_enabled(mut self, enabled: bool) -> Self {
        self.undo.enabled = enabled;
        self
    }

    #[cfg(feature = "text-undo")]
    pub fn max_undo_entries(mut self, n: usize) -> Self {
        self.undo.max_entries = n;
        self
    }

    #[cfg(feature = "text-undo")]
    pub fn max_undo_memory(mut self, bytes: usize) -> Self {
        self.undo.max_bytes = bytes;
        self
    }

    #[cfg(feature = "text-undo")]
    pub fn typing_merge_timeout_ms(mut self, ms: u64) -> Self {
        self.undo.typing_merge = std::time::Duration::from_millis(ms);
        self
    }
    /// CSS `text-align` (like `"center"` / `"start"` / `"justify"`).
    pub fn text_align(mut self, align: impl AsRef<str>) -> Self {
        self.text_align = crate::style::TextAlign::parse(align.as_ref());
        self
    }
    /// CSS `caret-color`. Tokens / hex / `[u8; 4]`. Default theme token `caret`
    /// (independent of placeholder muted color).
    pub fn caret_color(mut self, color: impl crate::style::IntoTextColor) -> Self {
        self.caret_color = Some(color.into_text_color());
        self
    }
    /// Selection highlight fill. Tokens / hex / `[u8; 4]` used as-is (including alpha).
    /// Default is theme token `selection-background` (alias `selection`).
    pub fn selection_background(mut self, color: impl crate::style::IntoTextColor) -> Self {
        self.selection_background = Some(color.into_text_color());
        self
    }
    /// Selected glyph color (CSS `::selection { color }`). Tokens / hex / `[u8; 4]`
    /// used as-is. Default is theme token `selection-foreground` (inverse of
    /// `selection-background`).
    pub fn selection_foreground(mut self, color: impl crate::style::IntoTextColor) -> Self {
        self.selection_foreground = Some(color.into_text_color());
        self
    }

    /// `clear` (default) clears on blur; `retain` keeps a muted selection.
    pub fn selection_on_blur(
        mut self,
        policy: impl Into<crate::utils::text_field::SelectionOnBlur>,
    ) -> Self {
        self.selection_on_blur = policy.into();
        self
    }
    /// Focus-ring thickness in logical px (Hard spread / Soft blur). Overrides the theme default.
    pub fn ring_width(mut self, width: impl IntoF32) -> Self {
        self.container = self.container.ring_width(width);
        self
    }
    /// Focus-ring color. Tokens / hex / `[f32; 4]`. Overrides theme token `ring`.
    pub fn ring_color(mut self, color: impl crate::style::IntoPaintColor) -> Self {
        self.container = self.container.ring_color(color);
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
    /// Ignored: placeholder uses [`TextOverflow::editable_placeholder`]; committed
    /// value uses [`TextOverflow::editable_single_line`] (caret pan).
    pub fn text_overflow(self, _mode: impl Into<crate::style::TextOverflow>) -> Self {
        self
    }
    pub fn font_weight(mut self, weight: impl crate::core::tree::IntoF32Raw) -> Self {
        self.font_weight = weight.into_f32_raw().round() as u16;
        self
    }

    /// 去边框/阴影/焦点环，供 [`crate::components::input_adornment`] / Command 外壳使用。
    pub fn plain_chrome(mut self, v: bool) -> Self {
        self.plain_chrome = v;
        if v {
            self.container = self
                .container
                .rounded(0.0)
                .w("auto")
                .h("100%")
                .flex_grow(1.0)
                .min_w("0")
                .px("0.75rem")
                .py("0.25rem");
        }
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
                "__jadix_input_rt_{}",
                RT_EMPTY_INPUT_ID.fetch_add(1, Ordering::Relaxed)
            )
        } else {
            builder.id
        };

        let chrome = builder.chrome;
        chrome.apply(build_text_edit_element(TextEditBuildParams {
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
            caret_color: builder.caret_color,
            selection_bg: builder.selection_background,
            selection_foreground: builder.selection_foreground,
            selection_on_blur: builder.selection_on_blur,
            single_line_container: Some(builder.container),
            multiline: false,
            rows: 1,
            resizable: false,
            resize_max_h: 0.0,
            wrap_mode: crate::utils::text_field::TextareaWrapMode::Word,
            plain_chrome: builder.plain_chrome,
            drop_undo_on_unmount: true,
            undo: {
                #[cfg(feature = "text-undo")]
                {
                    builder.undo
                }
                #[cfg(not(feature = "text-undo"))]
                {
                    crate::utils::text_edit_history::TextUndoConfig::input_default()
                }
            },
        }))
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
/// Alias of [`crate::primitives::interactive::apply_disabled`].
pub fn apply_bound_input_disabled(el: &mut Element, disabled: bool) -> LayoutStatus {
    crate::primitives::interactive::apply_disabled(el, disabled)
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

    fn find_input_text_leaf(el: &Element) -> Option<&Element> {
        if el.text.is_some() {
            return Some(el);
        }
        el.children.iter().find_map(find_input_text_leaf)
    }

    #[test]
    fn input_placeholder_uses_ellipsis_overflow() {
        let el: Element = input("t-ph-ellipsis")
            .placeholder("Placeholder…")
            .truncate(true)
            .text_overflow("hidden")
            .into();
        let leaf = find_input_text_leaf(&el).expect("input text leaf");
        assert_eq!(
            leaf.text_style.overflow,
            crate::style::TextOverflow::Ellipsis,
            "empty field shows placeholder with trailing … when narrow"
        );
        assert!(!leaf.text_style.overflow.allows_caret_scroll());
    }

    #[test]
    fn input_with_value_uses_hidden_overflow_for_caret_pan() {
        let sig = use_signal("__jadix_input_val_overflow", || "hello".to_string());
        let el: Element = input("t-val-hidden").value_signal(sig).into();
        let leaf = find_input_text_leaf(&el).expect("input text leaf");
        assert_eq!(leaf.text_style.overflow, crate::style::TextOverflow::Hidden);
        assert!(leaf.text_style.overflow.allows_caret_scroll());
    }

    #[test]
    fn input_value_signal_seeds_store() {
        let sig = use_signal("__jadix_wp38_input_val", || "hello".to_string());
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

    #[test]
    fn ring_width_and_color_are_authored_on_chrome() {
        let el: Element = input("ring-style")
            .ring_width(5.0)
            .ring_color("primary")
            .into();
        assert_eq!(el.style.focus_ring_width, Some(5.0));
        let want = crate::style::parse_color("primary");
        let got = el.style.focus_ring_color.expect("ring color");
        assert!((got[0] - want[0]).abs() < 0.001);
        assert!((got[1] - want[1]).abs() < 0.001);
        assert!((got[2] - want[2]).abs() < 0.001);
    }

    #[test]
    fn ring_utility_class_is_authored_on_chrome() {
        let el: Element = input("ring-class").class("ring-2 ring-primary").into();
        assert_eq!(el.style.focus_ring_width, Some(2.0));
        let want = crate::style::parse_color("primary");
        let got = el.style.focus_ring_color.expect("ring color");
        assert!((got[0] - want[0]).abs() < 0.001);
    }

    #[test]
    fn chrome_text_size_forwards_into_paint_sync() {
        let el: Element = input("sz-fwd").into();
        let el = el.text_size(20.0);
        let sync = el.input_paint_sync.as_ref().expect("paint sync");
        assert!(
            (sync.text_size_px - 20.0).abs() < 0.01,
            "Element::text_size must update inner glyphs, not only container text_style"
        );
        assert_eq!(el.text_style.size, Some(20.0));
    }

    #[test]
    fn text_size_utility_class_forwards_into_paint_sync() {
        let el: Element = input("sz-class").class("text-lg").into();
        let sync = el.input_paint_sync.as_ref().expect("paint sync");
        assert!(
            (sync.text_size_px - 18.0).abs() < 0.01,
            "class text-lg (1.125rem) must land on input_paint_sync"
        );
    }

    #[test]
    fn chrome_caret_color_forwards_into_paint_sync() {
        let el: Element = input("caret-fwd").into();
        let el = el.caret_color("#111111");
        let want = crate::core::tree::parse_text_color("#111111");
        let got = el
            .input_paint_sync
            .as_ref()
            .expect("paint sync")
            .caret_color
            .expect("caret");
        assert_eq!(got, want);
    }
}
