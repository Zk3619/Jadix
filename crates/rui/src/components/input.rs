use crate::core::reactivity::use_signal;
use crate::style::InputCaretDraw;
use crate::core::tree::{parse_color, parse_text_color, Element, IntoDimension, IntoF32};
use crate::primitives::r#box::{hstack, BoxBuilder};
use crate::primitives::interactive::interactive;
use crate::components::text_node::{build_text_node, TextNodeConfig};

/// `focus_id` 前缀；与 `Button` 的普通 id 区分。
pub const INPUT_FOCUS_PREFIX: &str = "input:";
/// 全局：当前会话是否收到过 IME `Enabled`（用于区分直输与组字）。
pub const IME_ACTIVE_SIGNAL: &str = "__rui_ime_active";

pub fn input_value_signal_key(id: &str) -> String { format!("__rui_input_value_{}", id) }
pub fn input_preedit_signal_key(id: &str) -> String { format!("__rui_input_preedit_{}", id) }
pub fn input_preedit_sel_signal_key(id: &str) -> String { format!("__rui_input_preedit_sel_{}", id) }
pub fn input_caret_signal_key(id: &str) -> String { format!("__rui_input_caret_{}", id) }
pub fn input_selection_signal_key(id: &str) -> String { format!("__rui_input_selection_{}", id) }
pub fn input_overwrite_signal_key(id: &str) -> String { format!("__rui_input_overwrite_{}", id) }
pub fn input_ime_anchor_signal_key(id: &str) -> String { format!("__rui_input_ime_anchor_{}", id) }

fn focus_id_for_input(id: &str) -> String { format!("{INPUT_FOCUS_PREFIX}{id}") }

pub struct InputBuilder {
    id: String,
    container: BoxBuilder,
    placeholder: String,
    disabled: bool,
    invalid: bool,
    text_size_px: f32,
    font_weight: u16,
    line_height_px: Option<f32>,
    leading_scale: Option<f32>,
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
        text_size_px: 16.0, // 1rem
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
    pub fn text_leading_none(self) -> Self { self.text_leading(1.0) }
    pub fn text_leading_tight(self) -> Self { self.text_leading(1.25) }
    pub fn text_leading_normal(self) -> Self { self.text_leading(1.5) }
    pub fn text_leading_relaxed(self) -> Self { self.text_leading(1.625) }
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
        let focus_key = "__rui_button_focus_id";
        let focus_visible_key = "__rui_focus_visible";
        let focused = use_signal(focus_key, String::new);
        let focus_visible = use_signal(focus_visible_key, || false);

        let fid = focus_id_for_input(&builder.id);
        let is_focused = focused.get() == fid && focus_visible.get() && !builder.disabled;

        let value_sig = use_signal(&input_value_signal_key(&builder.id), String::new);
        let preedit_sig = use_signal(&input_preedit_signal_key(&builder.id), String::new);
        let preedit_sel_sig = use_signal(&input_preedit_sel_signal_key(&builder.id), || Option::<(usize, usize)>::None);
        let caret_sig = use_signal(&input_caret_signal_key(&builder.id), || None::<usize>);
        let text_sel_sig = use_signal(&input_selection_signal_key(&builder.id), || None::<(usize, usize)>);
        let overwrite_sig = use_signal(&input_overwrite_signal_key(&builder.id), || false);
        let ime_anchor_sig = use_signal(&input_ime_anchor_signal_key(&builder.id), || None::<usize>);

        let committed = value_sig.get();
        let preedit = preedit_sig.get();
        let preedit_sel = preedit_sel_sig.get();
        let ime_anchor = ime_anchor_sig.get();

        let show_placeholder = committed.is_empty() && preedit.is_empty();
        let display_text = if show_placeholder {
            builder.placeholder.clone()
        } else if preedit.is_empty() {
            committed.clone()
        } else {
            let mut a = ime_anchor.unwrap_or(committed.len()).min(committed.len());
            a = committed.floor_char_boundary(a);
            format!("{}{}{}", &committed[..a], preedit, &committed[a..])
        };

        let mut text_color = if show_placeholder { parse_text_color("muted-foreground") } else { parse_text_color("foreground") };
        if builder.disabled {
            text_color[3] = (text_color[3] as f32 * 0.5) as u8;
        }

        let mut border_c = if builder.invalid { parse_color("destructive") } else if is_focused { parse_color("border-ring") } else { parse_color("input") };
        let mut bg = parse_color("transparent");
        if builder.disabled {
            border_c[3] *= 0.5;
            bg[3] *= 0.5;
        }

        let mut container = builder.container.bg(bg).border(border_c, 1.0);
        if is_focused {
            let shadow_color = if builder.invalid { parse_color("destructive").with_alpha(0.4) } else { parse_color("ring") };
            container = container.shadow(shadow_color, 3.0);
        } else if !builder.disabled {
            container = container.shadow([0.0, 0.0, 0.0, 0.06], 1.0);
        }

        let caret_byte = if is_focused {
            if show_placeholder { 0 }
            else if !preedit.is_empty() {
                let mut a = ime_anchor.unwrap_or(committed.len()).min(committed.len());
                a = committed.floor_char_boundary(a);
                let end = preedit_sel.map(|(_, e)| e).unwrap_or_else(|| preedit.len()).min(preedit.len());
                a + preedit.floor_char_boundary(end)
            } else {
                let len = display_text.len();
                let ix = text_sel_sig.get().and_then(|(a, b)| if a.min(b) < a.max(b) { Some(a.max(b)) } else { None })
                    .or_else(|| caret_sig.get()).unwrap_or(len);
                display_text.floor_char_boundary(ix.min(len))
            }
        } else { 0 };

        let mut text_el = build_text_node(display_text.clone(), TextNodeConfig {
            size_px: builder.text_size_px,
            weight: builder.font_weight,
            color: text_color,
            underline: false,
            center: false,
            line_height_px: builder.line_height_px,
            leading_scale: builder.leading_scale.or(Some(1.5)),
            font_family: None,
        }).w("100%");

        if is_focused {
            let mut pc = parse_color("primary");
            pc[3] *= 0.35;
            let selection_bg = [(pc[0] * 255.0) as u8, (pc[1] * 255.0) as u8, (pc[2] * 255.0) as u8, (pc[3] * 255.0) as u8];
            let sel_draw = if preedit.is_empty() { text_sel_sig.get().and_then(|(a, b)| if a.min(b) < a.max(b) { Some((a.min(b), a.max(b))) } else { None }) } else { None };
            let overwrite_highlight = if overwrite_sig.get() && preedit.is_empty() && sel_draw.is_none() {
                let ix = display_text.floor_char_boundary(caret_byte.min(display_text.len()));
                if ix < display_text.len() { Some((ix, ix + display_text[ix..].chars().next().unwrap().len_utf8())) } else { None }
            } else { None };
            text_el.text_style.input_caret = Some(InputCaretDraw { caret_byte_index: caret_byte, selection: sel_draw, selection_bg, overwrite_highlight });
        }

        let mut el: Element = container.into();
        el = if builder.disabled { el.cursor_not_allowed() } else { el.cursor_text() };
        
        let disabled = builder.disabled;
        let id_for_click = fid.clone();
        interactive(el.child(text_el))
            .focus_id(fid)
            .focusable(!disabled)
            .on_click(move || {
                if !disabled {
                    use_signal(focus_key, String::new).set(id_for_click.clone());
                    use_signal(focus_visible_key, || false).set(true);
                }
            })
            .into()
    }
}

trait ColorExt { fn with_alpha(self, alpha: f32) -> [f32; 4]; }
impl ColorExt for [f32; 4] { fn with_alpha(mut self, alpha: f32) -> [f32; 4] { self[3] = alpha; self } }

impl crate::core::tree::IntoElementList for InputBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}
