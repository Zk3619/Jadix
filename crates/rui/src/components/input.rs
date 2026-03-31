use std::sync::atomic::{AtomicU64, Ordering};

use crate::core::reactivity::use_signal;
use crate::style::InputCaretDraw;
use crate::core::tree::{
    parse_color, parse_text_color, Element, InputPaintSync, IntoDimension, IntoF32, LayoutStatus,
};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::r#box::{hstack, BoxBuilder};
use crate::primitives::interactive::interactive;
use crate::components::text_node::{build_text_node, TextNodeConfig};
use rui_macros::component;

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

/// 仅参与 cosmic/glyphon 排版，不计入 `Element::text`；行尾薄空格使拉丁度量参与行盒，减轻纯中文 IME 拼音阶段的垂直抖动。
///
/// 仅在聚焦编辑时挂到 `text_shape_suffix`：失焦且纯 ASCII 走 Basic shaping 时该字位易缺字形，会被画成「乱码框」。
pub const INPUT_LINE_SHAPE_SUFFIX: &str = "\u{2009}";

fn focus_id_for_input(id: &str) -> String { format!("{INPUT_FOCUS_PREFIX}{id}") }

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
    text_size_px: f32,
    font_weight: u16,
    line_height_px: Option<f32>,
    leading_scale: Option<f32>,
}

/// Input component for single-line text.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Input(
    id: String,
    placeholder: String,
    disabled: bool,
    invalid: bool,
    text_size: Option<f32>,
    mut on_click: crate::core::tree::Handler,
    mut on_key_down: crate::core::tree::KeyHandler,
    children: Option<Element>,
) -> Element {
    let mut builder = input(&id)
        .placeholder(placeholder)
        .disabled(disabled)
        .invalid(invalid);
    
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
            (&mut on_click.0)(());
        });

        let mut old_key = el.on_key_down.clone();
        el = el.on_key_down(move |ev| {
            let mut handled = false;
            if let Some(ref mut rc) = old_key {
                 handled = rc.borrow_mut()(ev.clone());
            }
            if handled { return true; }
            (&mut on_key_down.0)(ev)
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
        let focus_visible = use_signal(focus_visible_key, || false);

        let fid = focus_id_for_input(&field_id);
        // `peek`：不把焦点/输入值订阅到根 `create_effect`，避免点击输入框、切焦点等触发整树 `rebuild_ui`
        //（会破坏 `<Reactive>` 占位协调）。外观由 `sync_input_paint_subtree` 每帧按 `peek` 刷新。
        let is_focused = focused.peek() == fid && focus_visible.peek() && !builder.disabled;

        let value_sig = use_signal(&input_value_signal_key(&field_id), String::new);
        let preedit_sig = use_signal(&input_preedit_signal_key(&field_id), String::new);
        let preedit_sel_sig = use_signal(&input_preedit_sel_signal_key(&field_id), || Option::<(usize, usize)>::None);
        let caret_sig = use_signal(&input_caret_signal_key(&field_id), || None::<usize>);
        let text_sel_sig = use_signal(&input_selection_signal_key(&field_id), || None::<(usize, usize)>);
        let overwrite_sig = use_signal(&input_overwrite_signal_key(&field_id), || false);
        let ime_anchor_sig = use_signal(&input_ime_anchor_signal_key(&field_id), || None::<usize>);

        let committed = value_sig.peek();
        let preedit = preedit_sig.peek();
        let preedit_sel = preedit_sel_sig.peek();
        let ime_anchor = ime_anchor_sig.peek();

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
                let ix = text_sel_sig.peek().and_then(|(a, b)| if a.min(b) < a.max(b) { Some(a.max(b)) } else { None })
                    .or_else(|| caret_sig.peek()).unwrap_or(len);
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
            leading_scale: builder.leading_scale,
            font_family: None,
        }).w("100%");
        text_el.text_shape_suffix = if is_focused {
            Some(INPUT_LINE_SHAPE_SUFFIX.to_string())
        } else {
            None
        };
        text_el.template_id = Some(format!("__rui_input:{}:text", field_id));

        if is_focused {
            let mut pc = parse_color("primary");
            pc[3] *= 0.35;
            let selection_bg = [(pc[0] * 255.0) as u8, (pc[1] * 255.0) as u8, (pc[2] * 255.0) as u8, (pc[3] * 255.0) as u8];
            let sel_draw = if preedit.is_empty() { text_sel_sig.peek().and_then(|(a, b)| if a.min(b) < a.max(b) { Some((a.min(b), a.max(b))) } else { None }) } else { None };
            let overwrite_highlight = if overwrite_sig.peek() && preedit.is_empty() && sel_draw.is_none() {
                let ix = display_text.floor_char_boundary(caret_byte.min(display_text.len()));
                if ix < display_text.len() { Some((ix, ix + display_text[ix..].chars().next().unwrap().len_utf8())) } else { None }
            } else { None };
            text_el.text_style.input_caret = Some(InputCaretDraw { caret_byte_index: caret_byte, selection: sel_draw, selection_bg, overwrite_highlight });
        }

        let mut el: Element = container.into();
        el = if builder.disabled { el.cursor_not_allowed() } else { el.cursor_text() };

        let sync = InputPaintSync {
            base_id: field_id.clone(),
            placeholder: builder.placeholder.clone(),
            invalid: builder.invalid,
            disabled: builder.disabled,
            text_size_px: builder.text_size_px,
            font_weight: builder.font_weight,
            line_height_px: builder.line_height_px,
            leading_scale: builder.leading_scale,
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
    let focus_visible_key = "__rui_focus_visible";
    let focused = use_signal(focus_key, String::new);
    let focus_visible = use_signal(focus_visible_key, || false);
    let fid = focus_id_for_input(&desc.base_id);
    let is_focused = focused.peek() == fid && focus_visible.peek() && !desc.disabled;

    let committed = use_signal(&input_value_signal_key(&desc.base_id), String::new).peek();
    let preedit = use_signal(&input_preedit_signal_key(&desc.base_id), String::new).peek();
    let preedit_sel = use_signal(&input_preedit_sel_signal_key(&desc.base_id), || Option::<(usize, usize)>::None).peek();
    let caret_sig = use_signal(&input_caret_signal_key(&desc.base_id), || None::<usize>);
    let text_sel_sig = use_signal(&input_selection_signal_key(&desc.base_id), || None::<(usize, usize)>);
    let overwrite_sig = use_signal(&input_overwrite_signal_key(&desc.base_id), || false);
    let ime_anchor_sig = use_signal(&input_ime_anchor_signal_key(&desc.base_id), || None::<usize>);
    let ime_anchor = ime_anchor_sig.peek();

    let show_placeholder = committed.is_empty() && preedit.is_empty();
    let display_text = if show_placeholder {
        desc.placeholder.clone()
    } else if preedit.is_empty() {
        committed.clone()
    } else {
        let mut a = ime_anchor.unwrap_or(committed.len()).min(committed.len());
        a = committed.floor_char_boundary(a);
        format!("{}{}{}", &committed[..a], preedit, &committed[a..])
    };

    let mut text_color = if show_placeholder {
        parse_text_color("muted-foreground")
    } else {
        parse_text_color("foreground")
    };
    if desc.disabled {
        text_color[3] = (text_color[3] as f32 * 0.5) as u8;
    }

    let mut border_c = if desc.invalid {
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
        let sc = if desc.invalid {
            parse_color("destructive").with_alpha(0.4)
        } else {
            parse_color("ring")
        };
        (sc, 3.0)
    } else if !desc.disabled {
        ([0.0, 0.0, 0.0, 0.06], 1.0)
    } else {
        ([0.0, 0.0, 0.0, 0.0], 0.0)
    };

    let caret_byte = if is_focused {
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
            let len = display_text.len();
            let ix = text_sel_sig
                .peek()
                .and_then(|(a, b)| if a.min(b) < a.max(b) { Some(a.max(b)) } else { None })
                .or_else(|| caret_sig.peek())
                .unwrap_or(len);
            display_text.floor_char_boundary(ix.min(len))
        }
    } else {
        0
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
                        Some((a.min(b), a.max(b)))
                    } else {
                        None
                    }
                })
            } else {
                None
            };
            let overwrite_highlight = if overwrite_sig.peek() && preedit.is_empty() && sel_draw.is_none() {
                let ix = display_text.floor_char_boundary(caret_byte.min(display_text.len()));
                if ix < display_text.len() {
                    Some((
                        ix,
                        ix + display_text[ix..].chars().next().unwrap().len_utf8(),
                    ))
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
            text_layout = true;
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
