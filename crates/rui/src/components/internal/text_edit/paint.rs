//! 每帧 chrome sync（边框 / 焦点环 / 文本 / caret / 多行高度与滚入视口）。

use crate::components::input::{
    effective_invalid, input_caret_signal_key, input_clear_template_id,
    input_ime_anchor_signal_key, input_overwrite_signal_key, input_password_reveal_signal_key,
    input_preedit_sel_signal_key, input_preedit_signal_key, input_reveal_template_id,
    input_selection_signal_key, input_text_template_id, input_value_signal_key, lucide_named,
    map_byte_index_to_masked, map_byte_range_to_masked, mask_password, resolve_placeholder,
    set_adornment_icon_glyph, set_adornment_visible, InputType,
};
use crate::core::reactivity::use_signal;
use crate::core::tree::{parse_color, parse_text_color, Element, InputPaintSync, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::style::{
    apply_focus_ring_shadow_color, focus_ring_border_color, focus_ring_color,
    focus_ring_destructive_color, focus_ring_params, focus_ring_shadow_matches_color,
    focus_ring_style, InputCaretDraw,
};
use crate::utils::text_field::{
    ensure_caret_visible_scroll_x, ensure_caret_visible_scroll_y_at_line, focus_id_for_text_edit,
    input_scroll_x_signal_key, pull_controlled_into_store, textarea_wrap_width_signal_key,
};

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static INPUT_SCROLL_PREPARE_CALLS: Cell<usize> = const { Cell::new(0) };
}

/// Test hook: how many `text::prepare` calls the single-line caret-scroll path made.
#[cfg(test)]
pub(crate) fn take_input_scroll_prepare_calls() -> usize {
    INPUT_SCROLL_PREPARE_CALLS.with(|c| c.replace(0))
}

/// 每帧将 Input/Textarea 的显示状态与全局/每字段信号对齐（全部 `peek`，不触发根 `rebuild_ui`）。
/// 测试 / 无视口场景：不裁剪（等价于超大 viewport）。
#[cfg(test)]
pub(crate) fn sync_text_edit_paint_subtree(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    sync_text_edit_paint_subtree_in_viewport(el, layout_tree, impact, 10_000.0, 10_000.0, [0.0, 0.0]);
}

/// 与 `sync_reactive_elements_subtree` 相同：完全离屏子树跳过（forms 页多 Input 滚动热路径）。
pub(crate) fn sync_text_edit_paint_subtree_in_viewport(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
    viewport_w: f32,
    viewport_h: f32,
    scroll_acc: [f32; 2],
) {
    if crate::core::tree::element_is_offscreen(el, scroll_acc, viewport_w, viewport_h) {
        return;
    }
    if let Some(desc) = el.input_paint_sync.clone() {
        sync_one_text_edit_paint(el, &desc, layout_tree, impact);
    }
    let child_acc = crate::core::tree::child_scroll_accumulation(el, scroll_acc);
    for child in &mut el.children {
        sync_text_edit_paint_subtree_in_viewport(
            child,
            layout_tree,
            impact,
            viewport_w,
            viewport_h,
            child_acc,
        );
    }
}

fn sync_one_text_edit_paint(
    el: &mut Element,
    desc: &InputPaintSync,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    let focus_key = "__rui_button_focus_id";
    let focused = use_signal(focus_key, String::new);
    let fid = focus_id_for_text_edit(&desc.base_id, desc.multiline);
    let is_focused = focused.peek() == fid && !desc.disabled;

    let mut committed = use_signal(&input_value_signal_key(&desc.base_id), String::new).peek();
    if let Some(updated) = pull_controlled_into_store(&desc.base_id, &committed, is_focused) {
        committed = updated;
    }
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

    let reveal = if desc.password_toggle && input_ty.is_password() {
        use_signal(&input_password_reveal_signal_key(&desc.base_id), || false).peek()
    } else {
        false
    };
    let mask = input_ty.is_password() && !reveal && !show_placeholder;

    let mut text_color = if show_placeholder {
        parse_text_color("muted-foreground")
    } else {
        parse_text_color("foreground")
    };
    if desc.disabled {
        text_color[3] = (text_color[3] as f32 * 0.5) as u8;
    }

    let mut border_c = if desc.plain_chrome {
        [0.0, 0.0, 0.0, 0.0]
    } else if invalid {
        parse_color("destructive")
    } else if is_focused {
        focus_ring_border_color()
    } else {
        parse_color("input")
    };
    let mut bg = parse_color("transparent");
    if desc.disabled {
        border_c[3] *= 0.5;
        bg[3] *= 0.5;
    }

    let ring_color = if invalid {
        focus_ring_destructive_color()
    } else {
        focus_ring_color()
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
    if desc.plain_chrome {
        if el.style.border_width != 0.0 {
            el.style.border_width = 0.0;
            container_layout = true;
        }
        let shadow_on = el.style.shadow_blur > 0.01
            || el.style.shadow_spread.abs() > 0.01
            || el.style.shadow_color[3] > 0.001;
        if shadow_on {
            el.style.shadow_color = [0.0, 0.0, 0.0, 0.0];
            el.style.shadow_blur = 0.0;
            el.style.shadow_spread = 0.0;
            el.style.shadow_offset = [0.0, 0.0];
            el.style.shadow_inset = false;
            container_layout = true;
        }
    } else if is_focused {
        if !focus_ring_shadow_matches_color(el, true, ring_color) {
            let (blur, spread) = focus_ring_params(focus_ring_style());
            let geom_same = (el.style.shadow_blur - blur).abs() <= 0.01
                && (el.style.shadow_spread - spread).abs() <= 0.01
                && el.style.shadow_offset == [0.0, 0.0]
                && !el.style.shadow_inset;
            apply_focus_ring_shadow_color(el, true, false, ring_color);
            // invalid ↔ valid only recolors the ring — paint-only (rapid Mark invalid).
            if geom_same {
                container_style = true;
            } else {
                container_layout = true;
            }
        }
    } else {
        let (want_color, want_blur, want_spread) = if !desc.disabled {
            ([0.0, 0.0, 0.0, 0.06], 1.0, 0.0)
        } else {
            ([0.0, 0.0, 0.0, 0.0], 0.0, 0.0)
        };
        let geom_ok = (el.style.shadow_blur - want_blur).abs() <= 0.01
            && (el.style.shadow_spread - want_spread).abs() <= 0.01
            && el.style.shadow_offset == [0.0, 0.0]
            && !el.style.shadow_inset;
        let color_ok = (el.style.shadow_color[0] - want_color[0]).abs() <= 0.001
            && (el.style.shadow_color[1] - want_color[1]).abs() <= 0.001
            && (el.style.shadow_color[2] - want_color[2]).abs() <= 0.001
            && (el.style.shadow_color[3] - want_color[3]).abs() <= 0.001;
        if !geom_ok || !color_ok {
            el.style.shadow_color = want_color;
            el.style.shadow_blur = want_blur;
            el.style.shadow_offset = [0.0, 0.0];
            el.style.shadow_spread = want_spread;
            el.style.shadow_inset = false;
            if geom_ok {
                container_style = true;
            } else {
                container_layout = true;
            }
        }
    }

    let mut text_layout = false;
    let mut text_style = false;

    if el.children.is_empty() {
        if container_layout {
            layout_tree.mark_element_layout_dirty(el);
            *impact = impact.merge(LayoutStatus::LayoutChanged);
        } else if container_style {
            layout_tree.update_taffy_node_style(el);
            *impact = impact.merge(LayoutStatus::StyleChanged);
        }
        return;
    }

    let text_tid = input_text_template_id(&desc.base_id);
    let text_el = find_descendant_by_template_id_mut(el, &text_tid);
    let Some(text_el) = text_el else {
        if container_layout {
            layout_tree.mark_element_layout_dirty(el);
            *impact = impact.merge(LayoutStatus::LayoutChanged);
        } else if container_style {
            layout_tree.update_taffy_node_style(el);
            *impact = impact.merge(LayoutStatus::StyleChanged);
        }
        return;
    };

    {
        if text_el.text.as_deref() != Some(display_text.as_str()) {
            text_el.text = Some(display_text.clone());
            text_layout = true;
        }
        let want_suffix = None;
        if text_el.text_shape_suffix != want_suffix {
            text_el.text_shape_suffix = want_suffix;
            text_layout = true;
        }
        if text_el.template_id.as_deref() != Some(text_tid.as_str()) {
            text_el.template_id = Some(text_tid.clone());
            text_layout = true;
        }
        if text_el.text_style.color != text_color {
            text_el.text_style.color = text_color;
            text_style = true;
        }
        if desc.multiline {
            use crate::style::WhiteSpace;
            let want_ws = if desc.wrap_mode.soft_wrap() {
                WhiteSpace::Normal
            } else {
                WhiteSpace::Nowrap
            };
            if text_el.text_style.white_space != want_ws {
                text_el.text_style.white_space = want_ws;
                text_el.attrs.insert(
                    "white_space".to_string(),
                    if want_ws == WhiteSpace::Nowrap {
                        "nowrap".to_string()
                    } else {
                        "normal".to_string()
                    },
                );
                text_layout = true;
            }
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
            text_el.text_style.input_caret = caret_draw.clone();
            text_style = true;
        }

        let want_scroll_x = if !desc.multiline && text_el.text_style.overflow.allows_caret_scroll()
        {
            let sx_sig = use_signal(&input_scroll_x_signal_key(&desc.base_id), || 0.0_f32);
            let current = sx_sig.peek();
            // Unfocused / empty placeholder: caret scroll is idle. Skip `text::prepare`
            // (gallery forms has ~10 on-screen Inputs — shaping every chrome frame caused
            // hitch while Textarea multiline never entered this path).
            if !is_focused || show_placeholder {
                if current.abs() > 0.05 {
                    sx_sig.set(0.0);
                }
                0.0
            } else {
                #[cfg(test)]
                INPUT_SCROLL_PREPARE_CALLS.with(|c| c.set(c.get().saturating_add(1)));
                let viewport_w = text_el.layout_bounds[2];
                let lh = crate::utils::text_field::text_edit_line_box_px(
                    desc.text_size_px,
                    desc.line_height_px,
                    desc.leading_scale,
                );
                let prepared =
                    crate::text::prepare(&display_text, desc.text_size_px, desc.font_weight, lh);
                let content_w = crate::text::intrinsic_width(&prepared);
                let raw_anchor = caret_sig.peek().unwrap_or(caret_in_real);
                let mut anchor_byte = if mask {
                    map_byte_index_to_masked(&real_display, raw_anchor, &display_text)
                } else {
                    raw_anchor.min(display_text.len())
                };
                anchor_byte = display_text.floor_char_boundary(anchor_byte.min(display_text.len()));
                #[cfg(test)]
                INPUT_SCROLL_PREPARE_CALLS.with(|c| c.set(c.get().saturating_add(1)));
                let prefix = crate::text::prepare(
                    &display_text[..anchor_byte.min(display_text.len())],
                    desc.text_size_px,
                    desc.font_weight,
                    lh,
                );
                let anchor_x = crate::text::intrinsic_width(&prefix);
                let next =
                    ensure_caret_visible_scroll_x(anchor_x, content_w, viewport_w, current);
                if (next - current).abs() > 0.05 {
                    sx_sig.set(next);
                }
                next
            }
        } else {
            0.0
        };
        if (text_el.text_style.scroll_x - want_scroll_x).abs() > 0.05 {
            text_el.text_style.scroll_x = want_scroll_x;
            text_style = true;
        }

        // Pin absolute line box every frame (cascade must not re-inject leading-none).
        let line_box = crate::utils::text_field::text_edit_line_box_px(
            desc.text_size_px,
            desc.line_height_px,
            desc.leading_scale,
        );
        if text_el.text_style.line_height_px != Some(line_box)
            || text_el.text_style.line_height_scale.is_some()
        {
            text_el.text_style.line_height_px = Some(line_box);
            text_el.text_style.line_height_scale = None;
            text_el.attrs.insert("line_height".into(), "1".into());
            text_el.attrs.insert("leading".into(), "1".into());
            text_layout = true;
        }
        // Leaf height: multiline = content (+ bottom ink); single-line = one line box
        // (centering in Input). Never track ScrollArea/Textarea chrome height.
        // Soft-wrap: measure at live leaf / content width. Scroll wrap: unbounded avail
        // so height stays hard-`\n` only.
        let want_h = if desc.multiline {
            let soft = desc.wrap_mode.soft_wrap();
            let avail = if soft {
                let w = text_el.layout_bounds[2];
                if w > 1.0 {
                    w
                } else {
                    320.0
                }
            } else {
                10_000.0
            };
            if soft {
                use_signal(&textarea_wrap_width_signal_key(&desc.base_id), || 0.0_f32)
                    .set_silent(avail);
            }
            crate::utils::text_field::text_edit_content_height_px_wrap(
                &display_text,
                desc.text_size_px,
                desc.font_weight,
                line_box,
                avail,
                desc.wrap_mode,
            )
        } else {
            line_box
        };
        let cur_h = crate::layout::dim_length_px(text_el.style.size.height).unwrap_or(-1.0);
        if (cur_h - want_h).abs() > 0.5 {
            text_el.style.size.height = taffy::style::Dimension::length(want_h);
            text_el.style.min_size.height = taffy::style::Dimension::length(want_h);
            if desc.multiline {
                text_el.style.flex_shrink = 0.0;
            }
            text_layout = true;
        }
        if desc.multiline && !desc.wrap_mode.soft_wrap() {
            let prepared = crate::text::prepare(
                &display_text,
                desc.text_size_px,
                desc.font_weight,
                line_box,
            );
            let content_w = crate::text::intrinsic_width(&prepared).max(1.0);
            let cur_w = crate::layout::dim_length_px(text_el.style.size.width).unwrap_or(-1.0);
            if (cur_w - content_w).abs() > 0.5 {
                text_el.style.size.width = taffy::style::Dimension::length(content_w);
                text_layout = true;
            }
        }
        if !desc.multiline && text_el.text_style.overflow.nowrap() {
            if (text_el.style.flex_shrink - 1.0).abs() > f32::EPSILON {
                text_el.style.flex_shrink = 1.0;
                text_layout = true;
            }
            let w = crate::layout::dim_length_px(text_el.style.min_size.width);
            if !w.is_some_and(|v| v <= 0.5) {
                text_el.style.min_size.width = taffy::style::Dimension::length(0.0);
                text_layout = true;
            }
        }
    }

    let adorn_layout = false;
    let mut adorn_style = false;
    if desc.clearable {
        let clear_tid = input_clear_template_id(&desc.base_id);
        if let Some(btn) = find_descendant_by_template_id_mut(el, &clear_tid) {
            let show = (!committed.is_empty() || !preedit.is_empty()) && !desc.disabled;
            if set_adornment_visible(btn, show) {
                adorn_style = true;
            }
        }
    }
    if desc.password_toggle {
        let reveal_tid = input_reveal_template_id(&desc.base_id);
        if let Some(btn) = find_descendant_by_template_id_mut(el, &reveal_tid) {
            let glyph = if reveal {
                lucide_named("eye-off")
            } else {
                lucide_named("eye")
            };
            if set_adornment_icon_glyph(btn, &glyph) {
                adorn_style = true;
            }
            if set_adornment_visible(btn, is_focused && !desc.disabled) {
                adorn_style = true;
            }
        }
    }

    if desc.multiline {
        let h_key = crate::utils::text_field::textarea_height_signal_key(&desc.base_id);
        let want_h = use_signal(&h_key, || desc.resize_min_h)
            .peek()
            .clamp(desc.resize_min_h, desc.resize_max_h);
        let cur_h = crate::layout::dim_length_px(el.style.size.height).unwrap_or(-1.0);
        if (cur_h - want_h).abs() > 0.5 {
            el.style.size.height = taffy::style::Dimension::length(want_h);
            container_layout = true;
        }

        if let Some(rest) = desc.scroll_rest.as_deref() {
            let line_height = crate::utils::text_field::text_edit_line_box_px(
                desc.text_size_px,
                desc.line_height_px,
                desc.leading_scale,
            );
            let vh_key = crate::core::scroll_viewport::scroll_viewport_h_signal_key(rest);
            let sy_key = crate::core::scroll_viewport::scroll_offset_y_signal_key(rest);
            let sx_key = crate::core::scroll_viewport::scroll_offset_x_signal_key(rest);
            let ch_key = crate::core::scroll_viewport::scroll_content_h_signal_key(rest);
            let vh = use_signal(&vh_key, || 0.0_f32).peek();
            let sy = use_signal(&sy_key, || 0.0_f32).peek();
            let sx = use_signal(&sx_key, || 0.0_f32).peek();
            let ch = use_signal(&ch_key, || 0.0_f32).peek();
            let vp_fid = crate::core::scroll_viewport::focus_id_for_scroll_viewport(rest);
            let mut vp_h_layout = 0.0_f32;
            for child in el.children.iter() {
                if child.focus_id.as_deref() == Some(vp_fid.as_str()) {
                    vp_h_layout = child.layout_bounds[3];
                    break;
                }
            }
            let text_tid = input_text_template_id(&desc.base_id);
            let text_h_layout = find_descendant_by_template_id_ref(el, &text_tid)
                .map(|t| t.layout_bounds[3])
                .unwrap_or(0.0);
            // Content height from the string — do NOT trust a short leaf box alone.
            // Soft-wrap uses live leaf width; scroll-wrap stays hard-`\n` only.
            let soft = desc.wrap_mode.soft_wrap();
            let measure_avail = if soft {
                find_descendant_by_template_id_ref(el, &text_tid)
                    .map(|t| t.layout_bounds[2])
                    .filter(|w| *w > 1.0)
                    .unwrap_or(320.0)
            } else {
                10_000.0
            };
            let content_h_text = crate::utils::text_field::text_edit_content_height_px_wrap(
                &display_text,
                desc.text_size_px,
                desc.font_weight,
                line_height,
                measure_avail,
                desc.wrap_mode,
            );
            // Prefer the live scrollport box. Avoid `vh.max(...)` — a stale huge viewport
            // signal would make content_fits=true and clear sy every frame.
            let vh_eff = if vp_h_layout > 1.0 {
                vp_h_layout
            } else if vh > 1.0 {
                vh
            } else {
                (el.layout_bounds[3] - line_height * 0.25).max(0.0)
            };
            let ch_eff = content_h_text.max(text_h_layout);
            let _ = ch; // post-render metrics own content_h; do not raise the signal here —
                        // scrollbar prepass runs before layout collect and would desync thumb vs sy clamp.
            if vh_eff > 1.0 && (vh - vh_eff).abs() > 0.5 {
                use_signal(&vh_key, || 0.0_f32).set_silent(vh_eff);
            }
            let mut scroll_content_h = ch_eff;
            let mut caret_insets = crate::utils::text_field::TextareaCaretScrollInsets::default();
            for child in el.children.iter() {
                if child.focus_id.as_deref() == Some(vp_fid.as_str()) {
                    let region_h = if vp_h_layout > 1.0 {
                        vp_h_layout
                    } else {
                        vh_eff
                    };
                    let (pad_top, pad_bottom) =
                        crate::utils::text_field::textarea_scrollport_padding_y(
                            child,
                            child.layout_bounds[1],
                            region_h,
                        );
                    scroll_content_h = pad_top + ch_eff + pad_bottom;
                    caret_insets = crate::utils::text_field::TextareaCaretScrollInsets {
                        pad_top,
                        pad_bottom,
                        ink_bottom: crate::utils::text_field::text_edit_ink_pad_px(
                            desc.text_size_px,
                        ),
                    };
                    break;
                }
            }
            // Post-render publish wins when larger; never shrink below live text measure.
            // After Enter, `ch` is still one frame stale — replacing scroll_content_h made
            // max_sy too small until the user typed and collect refreshed the signal.
            if ch > vh_eff + 0.5 {
                scroll_content_h = scroll_content_h.max(ch);
            }
            let max_sy = (scroll_content_h - vh_eff).max(0.0);
            let content_fits = vh_eff > 1.0 && scroll_content_h <= vh_eff + 0.5;
            // Only pull scroll to the caret when the caret *moves* (typing / arrows).
            // Running this every focused frame clamps sy to caret_top — wheel/scrollbar
            // cannot travel past the caret (gallery: "只能滚动极小值").
            let last_caret_key = format!("__rui_ta_scroll_caret_{}", desc.base_id);
            let last_caret = use_signal(&last_caret_key, || None::<usize>).peek();
            let caret_moved = last_caret != Some(caret_byte);
            if caret_moved {
                use_signal(&last_caret_key, || None::<usize>).set_silent(Some(caret_byte));
            }

            // wrap=off: keep the caret in the horizontal viewport — but never while
            // drag-select edge auto-scroll owns `sx` (that pair caused scroll bounce).
            let mut want_sx = sx;
            if !soft
                && is_focused
                && caret_moved
                && !crate::utils::text_field::text_drag_select_active()
            {
                let cw_key = crate::core::scroll_viewport::scroll_content_w_signal_key(rest);
                let vw_key = crate::core::scroll_viewport::scroll_viewport_w_signal_key(rest);
                let mut vp_w_layout = 0.0_f32;
                let mut content_w_layout = 0.0_f32;
                for child in el.children.iter() {
                    if child.focus_id.as_deref() == Some(vp_fid.as_str()) {
                        vp_w_layout = child.layout_bounds[2];
                        break;
                    }
                }
                if let Some(text_leaf) = find_descendant_by_template_id_ref(el, &text_tid) {
                    content_w_layout = text_leaf.layout_bounds[2];
                }
                let vw_sig = use_signal(&vw_key, || 0.0_f32).peek();
                let cw_sig = use_signal(&cw_key, || 0.0_f32).peek();
                let viewport_w = if vp_w_layout > 1.0 {
                    vp_w_layout
                } else {
                    vw_sig
                };
                let content_w = content_w_layout.max(cw_sig).max(1.0);
                if viewport_w > 4.0 && content_w > viewport_w + 0.5 {
                    let (_, local) =
                        crate::utils::text_field::caret_line_and_local(&display_text, caret_byte);
                    let line_start = caret_byte.saturating_sub(local);
                    let prefix = &display_text[line_start..caret_byte.min(display_text.len())];
                    let anchor_x = crate::text::measure_prefix_advance(
                        prefix,
                        desc.text_size_px,
                        desc.font_weight,
                        line_height,
                    );
                    want_sx =
                        ensure_caret_visible_scroll_x(anchor_x, content_w, viewport_w, sx);
                }
            }

            let new_sy = if content_fits && sy > 0.5 {
                Some(0.0)
            } else if sy > max_sy + 0.5 {
                Some(max_sy)
            } else if is_focused
                && caret_moved
                && vh_eff >= line_height * 0.5
                && !crate::utils::text_field::text_drag_select_active()
            {
                let visual = if soft {
                    let model = crate::text::build_visual_lines(
                        &display_text,
                        desc.text_size_px,
                        desc.font_weight,
                        line_height,
                        measure_avail,
                        desc.wrap_mode.to_line_break_source(),
                    );
                    let i = crate::text::caret_visual_line_index(&model.ranges, caret_byte);
                    Some((i, model.ranges.len().max(1)))
                } else {
                    None
                };
                ensure_caret_visible_scroll_y_at_line(
                    &display_text,
                    caret_byte,
                    visual,
                    line_height,
                    vh_eff,
                    sy,
                    max_sy,
                    caret_insets,
                )
            } else if max_sy < 0.5 && sy > 0.5 {
                Some(0.0)
            } else {
                None
            };
            let sx_changed = (want_sx - sx).abs() > 0.05;
            if new_sy.is_some() || sx_changed {
                let apply_sy = new_sy.unwrap_or(sy);
                let apply_sx = if sx_changed { want_sx } else { sx };
                crate::app::scroll_support::set_scroll_offset_with_behavior(
                    rest, apply_sx, apply_sy, true,
                );
                for child in &mut el.children {
                    if child.focus_id.as_deref() == Some(vp_fid.as_str()) {
                        child.scroll_offset = [apply_sx, apply_sy];
                        break;
                    }
                }
            }
        }
    }

    // Text content / measure-affecting edits must `mark_element_layout_dirty` so
    // `text_measurements` refreshes. `update_taffy_node_style` alone leaves stale measure
    // text → multiline leaf stays ~1 line tall while glyphon draws N lines (squeeze + clip).
    if container_layout {
        layout_tree.mark_element_layout_dirty(el);
    } else if container_style || adorn_layout {
        layout_tree.update_taffy_node_style(el);
    }
    if let Some(text_el) = find_descendant_by_template_id_mut(el, &text_tid) {
        if text_el.layout_node.is_some() {
            if text_layout {
                layout_tree.mark_element_layout_dirty(text_el);
            } else if text_style {
                layout_tree.update_taffy_node_style(text_el);
            }
        }
    }
    if adorn_style {
        for tid in [
            input_clear_template_id(&desc.base_id),
            input_reveal_template_id(&desc.base_id),
        ] {
            if let Some(btn) = find_descendant_by_template_id_mut(el, &tid) {
                if btn.layout_node.is_some() {
                    layout_tree.update_taffy_node_style(btn);
                }
            }
        }
    }

    let layout_hit = container_layout || text_layout || adorn_layout;
    let style_hit = !layout_hit && (container_style || text_style || adorn_style);
    if layout_hit {
        *impact = impact.merge(LayoutStatus::LayoutChanged);
    } else if style_hit {
        *impact = impact.merge(LayoutStatus::StyleChanged);
        // Local dirty for invalid/focus border paint (WP-2.2 PartialLoad).
        let b = el.layout_bounds;
        if b[2] > 0.0 && b[3] > 0.0 {
            let pad = el.style.shadow_blur + el.style.shadow_spread.abs() + 8.0;
            crate::app::damage::push_frame_paint_damage([
                b[0] - pad,
                b[1] - pad,
                b[2] + pad * 2.0,
                b[3] + pad * 2.0,
            ]);
        }
    }
}

fn find_descendant_by_template_id_mut<'a>(
    el: &'a mut Element,
    tid: &str,
) -> Option<&'a mut Element> {
    if el.template_id.as_deref() == Some(tid) {
        return Some(el);
    }
    for child in &mut el.children {
        if let Some(found) = find_descendant_by_template_id_mut(child, tid) {
            return Some(found);
        }
    }
    None
}

fn find_descendant_by_template_id_ref<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
    if el.template_id.as_deref() == Some(tid) {
        return Some(el);
    }
    for child in &el.children {
        if let Some(found) = find_descendant_by_template_id_ref(child, tid) {
            return Some(found);
        }
    }
    None
}
