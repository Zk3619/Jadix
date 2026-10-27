//! Carousel product-layer submodule — shares parent symbols via `use super::*`.

use super::drag::*;
use super::geometry::*;
use super::*;

/// Viewport length is a measured signal, not a Taffy geometry change.
/// `LayoutChanged` here FullClears the window (parent ScrollArea thumb flash).
fn viewport_measure_status(applied: bool) -> LayoutStatus {
    if applied {
        LayoutStatus::StyleChanged
    } else {
        LayoutStatus::Clean
    }
}

fn track_motion_sync_status(el: &Element, changed: bool, dragging: bool) -> LayoutStatus {
    let animating =
        !dragging && crate::utils::motion::motion_is_animating(&el.style, &el.transition_snapshot);
    if changed || dragging || animating {
        // The track's children overflow its translated border box. Repaint the
        // viewport as a region so entering slides and their glyphs share a clear.
        let mut bounds = el.layout_bounds;
        bounds[0] -= el.paint_scroll_offset()[0];
        bounds[1] -= el.paint_scroll_offset()[1];
        crate::app::damage::push_frame_region_paint(
            crate::app::damage::paint_element_key(el),
            bounds,
        );
        LayoutStatus::StyleChanged
    } else {
        LayoutStatus::Clean
    }
}

fn build_stack_content(ctx: &CarouselCtx, items: Vec<Element>) -> Element {
    // 与 Single/Multi 一致：舞台宽度 = 视口 100%；高度由内容撑开（不用写死 rem）。
    // 绝对卡不占文档流，故放隐形测量槽（首卡克隆）提供高度；卡面 h(100%) 填满舞台。
    const CARD_WIDTH_FRAC: f32 = 0.46;

    let mut stage: Element = r#box()
        .relative(true)
        .w("100%")
        .min_w(0.0)
        .overflow("hidden")
        .into();
    // Stack cards move independently inside this fixed viewport. Mark the
    // viewport as a region so retained pixels from the previous card are
    // cleared before the transformed card/text is replayed.
    stage.paint_region_replay = true;
    stage.paint_children_unculled = true;
    stage
        .attrs
        .insert("data-stacking-context".to_string(), "1".to_string());

    if let Some(src) = items.first() {
        let mut measure = clone_carousel_slide_visual(src);
        measure
            .style
            .set_css_dim(UnitSlot::Width, CssDim::Percent(CARD_WIDTH_FRAC));
        measure = measure
            .mx("auto")
            .opacity(0.0)
            .pointer_events_none()
            .flex_shrink(0.0)
            .flex_grow(0.0);
        measure.set_attr("data-carousel-stack-measure", "1");
        stage = stage.child(measure);
    }

    for (i, mut card) in items.into_iter().enumerate() {
        card.style
            .set_css_dim(UnitSlot::Width, CssDim::Percent(CARD_WIDTH_FRAC));
        // 壳透明：透字由舞台 data-stacking-context 的页面 band 绘制解决。
        card = card
            .absolute()
            .h("100%")
            .left("50%")
            .top(0.0)
            .translate_pct(-0.5, 0.0)
            .px(0.0)
            .flex_basis("auto")
            .rounded(6.0)
            .bg([0.0, 0.0, 0.0, 0.0]);
        ensure_stack_card_css_transition(&mut card);
        reset_stack_descendant_scales(&mut card);
        let sync_ctx = ctx.clone();
        let idx = i;
        card = card.reactive_sync(move |el: &mut Element, _lt: &mut LayoutTree| {
            let _ = sync_ctx.index.peek();
            let _ = sync_ctx.drag_px.peek();
            let vp = sync_ctx.viewport_len.peek();
            // 视口尚未测到时勿用假 step（旧 fallback=96）写 translate/scale，
            // Otherwise, before the first frame/first screen is enlarged, the side card will be thrown out of the cropping area, and then the interior will look like "nailed to the old position with blank space around it."
            if vp <= 1.0 {
                return LayoutStatus::Clean;
            }
            let off = offset_target(&sync_ctx.id).peek();
            let count = sync_ctx.count.peek().max(1);
            let step = stack_step_px(vp).max(1.0);
            // progress = -offset/step；与 Multi 轨道同构，拖拽跟手 / 松手吸附共用。
            let progress = -off / step;
            let dist_f = if wraps_nav(&sync_ctx) {
                signed_ring_dist_f(idx as f32, progress, count)
            } else {
                idx as f32 - progress
            };
            let ad = dist_f.abs();
            let sides = sync_ctx.stack_sides.max(1) as f32;
            let tx = dist_f * step;
            let (sc, op, z) = stack_card_appearance(ad, sides, idx, count);

            let dragging = is_carousel_dragging_id(&sync_ctx.id);
            if dragging {
                el.style.transition.clear();
                el.transition_snapshot = Default::default();
            } else if el.style.transition.is_empty() {
                // Just let go: pin the CSS baseline to the current visual position, and then turn on the transition to reach the target (avoiding from=to instantaneous jump).
                crate::style::snap_element_transitions_to_current(
                    &el.inner.style,
                    &mut el.inner.transition_snapshot,
                );
                ensure_stack_card_css_transition(el);
            } else {
                ensure_stack_card_css_transition(el);
            }

            el.style.shadow_blur = 0.0;
            el.style.shadow_color = [0.0, 0.0, 0.0, 0.0];
            el.style.shadow_offset = [0.0, 0.0];

            let mut changed = false;
            // Do not lay an opaque bottom board (the transition/side card will expose the abnormal whiteboard); rely on page_stack_bands for occlusion.
            if el.style.bg_color[3] > 0.01 {
                el.style.bg_color = [0.0, 0.0, 0.0, 0.0];
                changed = true;
            }
            let tx_eps = if dragging { 0.05 } else { 0.5 };
            if dragging || (el.style.translate[0] - tx).abs() > tx_eps {
                el.style.translate[0] = tx;
                changed = true;
            }
            // The scaling is written on the shell (same node/same transition as translate), subtree scale=1;
            // collect via ancestor_scale / paint_scale to make the text scale with the shell (including rotate)
            // ancestor_linear → glyphon TextArea.transform）。
            // 拖拽时无阈值：每帧写入，避免与矩形连续插值不同步。
            let scale_eps = if dragging { 0.0 } else { 0.001 };
            if (el.style.scale[0] - sc).abs() > scale_eps
                || (el.style.scale[1] - sc).abs() > scale_eps
            {
                el.style.scale = [sc, sc];
                changed = true;
            }
            if (el.style.opacity - op).abs() > 0.005 {
                el.style.opacity = op;
                changed = true;
            }
            if el.style.z_index != z {
                el.style.z_index = z;
                changed = true;
            }
            reset_stack_descendant_scales(el);
            if dragging {
                fn clear_stack_transitions(node: &mut Element) {
                    crate::core::tree::walk_elements_mut(node, |n| {
                        n.style.transition.clear();
                        n.transition_snapshot = Default::default();
                    });
                }
                clear_stack_transitions(el);
            }

            let animating = !dragging
                && (el.transition_snapshot.has_active_tweens()
                    || crate::style::css_transition_needs_frame(
                        &el.style,
                        &el.transition_snapshot,
                    )
                    || el.children.iter().any(|c| {
                        c.transition_snapshot.has_active_tweens()
                            || crate::style::css_transition_needs_frame(
                                &c.style,
                                &c.transition_snapshot,
                            )
                    }));
            if changed || animating || dragging {
                crate::app::damage::push_frame_paint_damage(el.layout_bounds);
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        });
        stage = stage.child(card);
    }

    stage
        .attrs
        .insert(ATTR_SLOT.to_string(), SLOT_CONTENT.to_string());
    stage.template_id = Some(format!("{}:content", ctx.id));
    stage = stage.cursor_kind(crate::core::tree::CursorKind::Grab);

    // 与 Single/Multi 相同：测量视口宽度；resize 时经 apply_viewport_len 重钉 offset，
    // 避免 step 变了但 offset 仍按旧步长 → progress 漂移（首屏放大后卡位错乱/空白）。
    let vp_ctx = ctx.clone();
    stage = stage.reactive_sync(move |el, _lt: &mut LayoutTree| {
        #[cfg(all(test, feature = "product-tests"))]
        stamp_carousel_painted(&vp_ctx.id);
        let len = el.layout_bounds[2];
        let off = offset_target(&vp_ctx.id).peek();
        let previous_off = last_visual_offset(&vp_ctx.id);
        set_last_visual_offset(&vp_ctx.id, off);
        auto_sync_offset_if_snapping(&vp_ctx);
        let measured = apply_viewport_len(&vp_ctx, len);
        let dragging = is_carousel_dragging_id(&vp_ctx.id);
        let animating = !dragging
            && el.children.iter().any(|child| {
                child.transition_snapshot.has_active_tweens()
                    || crate::style::css_transition_needs_frame(
                        &child.style,
                        &child.transition_snapshot,
                    )
            });
        let moving = dragging || (off - previous_off).abs() > 0.05 || animating;
        if moving {
            let mut bounds = el.layout_bounds;
            bounds[0] -= el.paint_scroll_offset()[0];
            bounds[1] -= el.paint_scroll_offset()[1];
            crate::app::damage::push_frame_region_paint(
                crate::app::damage::paint_element_key(el),
                bounds,
            );
        }
        if measured || moving {
            LayoutStatus::StyleChanged
        } else {
            LayoutStatus::Clean
        }
    });

    let capture_id = format!("{}:content", ctx.id);
    let down_ctx = ctx.clone();
    let capture_for_down = capture_id.clone();
    stage = stage.on_pointer_down(move |e: &PointerEvent| {
        if e.phase == EventPhase::Capture {
            return;
        }
        let ptr = match down_ctx.orientation {
            CarouselOrientation::Horizontal => e.x,
            CarouselOrientation::Vertical => e.y,
        };
        begin_carousel_drag(CarouselDrag {
            id: down_ctx.id.clone(),
            start_ptr: ptr,
            base_offset: last_visual_offset(&down_ctx.id),
            index: down_ctx.index,
            count: down_ctx.count,
            per_view: down_ctx.per_view,
            wraps: wraps_nav(&down_ctx),
            loop_clones: loop_clone_count(&down_ctx),
            end_align: down_ctx.end_align,
            variant: down_ctx.variant,
            item_sizing: down_ctx.item_sizing,
            item_width_px: down_ctx.item_width_px,
            scroll_by: down_ctx.scroll_by,
            page_snap: down_ctx.page_snap,
            snap: effective_snap(&down_ctx),
            viewport_len: down_ctx.viewport_len,
            drag_px: down_ctx.drag_px,
            orientation: down_ctx.orientation,
            autoplay_ms: down_ctx.autoplay_ms,
        });
        crate::app::set_pointer_capture(capture_for_down.clone());
        e.stop_propagation();
    });
    stage = stage.on_pointer_up(move |e: &PointerEvent| {
        if e.phase == EventPhase::Capture {
            return;
        }
        clear_carousel_drag_if_any();
    });

    stage
}

crate::__component_contract! {
CarouselContentProps [];
/// solid-ui/ShadCN `CarouselContent`: `overflow-hidden` + spring track (dragable).
#[allow(non_snake_case)]
pub fn CarouselContent(
plain [] children: Option<Element>,
) -> Element {
    let ctx = require_carousel("CarouselContent");
    let mut kids = children.map(take_packed).unwrap_or_default();
    let mut items = Vec::new();
    for kid in kids.drain(..) {
        if is_carousel_item(&kid) {
            items.push(kid);
        } else {
            for n in take_packed(kid) {
                if is_carousel_item(&n) {
                    items.push(n);
                }
            }
        }
    }
    let n = items.len();
    ctx.count.set(n);
    if n > 0 {
        let hi = max_index(&ctx);
        if ctx.index.peek() > hi {
            ctx.index.set(hi);
        }
    }
    // 视口未就绪时 step≈1，set 会把弹簧拉向假目标；克隆轨道应等真实宽度再对齐。
    // snap=false：重建 content 时也不得把自由停靠 offset 拉回 index 格点。
    if ctx.viewport_len.peek() > 1.0 {
        auto_sync_offset_if_snapping(&ctx);
    }

    if ctx.variant == CarouselVariant::Stack {
        return build_stack_content(&ctx, items);
    }

    let cc = loop_clone_count(&ctx);
    let mut track_items: Vec<Element> = Vec::with_capacity(items.len() + cc * 2);
    if cc > 0 {
        let n = items.len();
        for src in &items[n - cc..] {
            track_items.push(clone_carousel_slide_visual(src));
        }
    }
    let suffix: Vec<Element> = if cc > 0 {
        items[..cc]
            .iter()
            .map(clone_carousel_slide_visual)
            .collect()
    } else {
        Vec::new()
    };
    track_items.extend(items);
    track_items.extend(suffix);

    // Pad：尾部追加空白槽（无缝循环时已禁用）。
    let spacers = pad_spacer_count(&ctx);
    for _ in 0..spacers {
        let mut spacer: Element = r#box().min_w(0.0).flex_shrink(0.0).flex_grow(0.0).into();
        spacer.set_attr("data-carousel-spacer", "1");
        track_items.push(spacer);
    }

    // Alignment ShadCN: fraction → basis=100%/perView; fixed → fixed itemWidth.
    let basis_frac = 1.0 / ctx.per_view.max(0.25);
    let horizontal = ctx.orientation == CarouselOrientation::Horizontal;
    let fixed_px = if ctx.item_sizing == CarouselItemSizing::Fixed && ctx.item_width_px > 1.0 {
        Some(ctx.item_width_px)
    } else {
        None
    };
    let items: Vec<Element> = track_items
        .into_iter()
        .map(|mut el| {
            if let Some(px) = fixed_px {
                if horizontal {
                    el.style.set_css_dim(UnitSlot::Width, CssDim::Px(px));
                    el.style.set_css_dim(UnitSlot::FlexBasis, CssDim::Px(px));
                } else {
                    el.style.set_css_dim(UnitSlot::Height, CssDim::Px(px));
                    el.style.set_css_dim(UnitSlot::FlexBasis, CssDim::Px(px));
                }
            } else if horizontal {
                el.style
                    .set_css_dim(UnitSlot::Width, CssDim::Percent(basis_frac));
                el.style
                    .set_css_dim(UnitSlot::FlexBasis, CssDim::Percent(basis_frac));
            } else {
                el.style
                    .set_css_dim(UnitSlot::Height, CssDim::Percent(basis_frac));
                el.style
                    .set_css_dim(UnitSlot::FlexBasis, CssDim::Percent(basis_frac));
            }
            el.min_w(0.0).flex_shrink(0.0).flex_grow(0.0)
        })
        .collect();

    let mut track: Element = match ctx.orientation {
        // w(100%)：给百分比 basis 提供定宽包含块（= 视口），子项 shrink-0 后横向溢出。
        CarouselOrientation::Horizontal => hstack()
            .w("100%")
            .min_w(0.0)
            .items("stretch")
            .children(items)
            .into(),
        CarouselOrientation::Vertical => vstack()
            .w("100%")
            .h("100%")
            .min_w(0.0)
            .children(items)
            .into(),
    };
    // 供 layout collect 识别：禁止 scroll_only sibling band（translate=0 时也要访齐后卡）。
    track.paint_region_replay = true;
    track.paint_children_unculled = true;
    // CSS `transition: transform` (including translate); target driven by offset_target signal.
    ensure_track_css_transition(&mut track);

    let track_ctx = ctx.clone();
    track = track.reactive_sync(move |el, _layout_tree: &mut LayoutTree| {
        let _ = track_ctx.index.peek();
        let _ = track_ctx.drag_px.peek();
        let _ = track_ctx.viewport_len.peek();
        let _ = offset_target(&track_ctx.id).peek();

        // Read the CSS interpolated visual value of the previous frame before writing to the target.
        let visual = match track_ctx.orientation {
            CarouselOrientation::Horizontal => el.style.translate[0],
            CarouselOrientation::Vertical => el.style.translate[1],
        };
        set_last_visual_offset(&track_ctx.id, visual);
        maybe_finish_loop_snap(&track_ctx, visual);
        // snap=false: After letting go, the index is only dots nearest, and the free offset must not be pulled back to the grid point.
        auto_sync_offset_if_snapping(&track_ctx);

        let x_or_y = if is_carousel_dragging_id(&track_ctx.id) {
            offset_target(&track_ctx.id).peek()
        } else if let Some(anim) = loop_anim(&track_ctx.id) {
            anim.target_offset
        } else {
            offset_target(&track_ctx.id).peek()
        };

        // 拖拽：关闭 transition，跟手写入。
        if is_carousel_dragging_id(&track_ctx.id) {
            let _ = take_instant_snap_to(&track_ctx.id);
            clear_track_css_transition(el);
            apply_axis_translate(el, track_ctx.orientation, x_or_y);
            return track_motion_sync_status(el, true, true);
        }

        ensure_track_css_transition(el);

        if let Some(park) = take_instant_snap_to(&track_ctx.id) {
            apply_axis_translate(el, track_ctx.orientation, park);
            snap_motion_baseline(el);
        }

        apply_axis_translate(el, track_ctx.orientation, x_or_y);
        let style_changed = (visual - x_or_y).abs() > 0.05;
        track_motion_sync_status(el, style_changed, false)
    });

    let mut viewport: Element = r#box()
        .w("100%")
        .min_w(0.0)
        .overflow("hidden")
        .child(track)
        .into();
    viewport
        .attrs
        .insert(ATTR_SLOT.to_string(), SLOT_CONTENT.to_string());
    viewport.template_id = Some(format!("{}:content", ctx.id));
    viewport = viewport.cursor_kind(crate::core::tree::CursorKind::Grab);

    let vp_ctx = ctx.clone();
    viewport = viewport.reactive_sync(move |el, _lt: &mut LayoutTree| {
        #[cfg(all(test, feature = "product-tests"))]
        stamp_carousel_painted(&vp_ctx.id);
        let len = match vp_ctx.orientation {
            CarouselOrientation::Horizontal => el.layout_bounds[2],
            CarouselOrientation::Vertical => el.layout_bounds[3],
        };
        let prev = vp_ctx.viewport_len.peek();
        let thresh = (prev.abs() * 0.01).max(2.0);
        if len > 1.0 && (prev - len).abs() > thresh {
            // Size catch-up / window resize: retarget offset via signals + StyleChanged.
            // Do not LayoutChanged — that FullClears the page and yanks outer scrollbars.
            return viewport_measure_status(apply_viewport_len(&vp_ctx, len));
        }
        LayoutStatus::Clean
    });

    let capture_id = format!("{}:content", ctx.id);
    let down_ctx = ctx.clone();
    let capture_for_down = capture_id.clone();
    viewport = viewport.on_pointer_down(move |e: &PointerEvent| {
        if e.phase == EventPhase::Capture {
            return;
        }
        let ptr = match down_ctx.orientation {
            CarouselOrientation::Horizontal => e.x,
            CarouselOrientation::Vertical => e.y,
        };
        begin_carousel_drag(CarouselDrag {
            id: down_ctx.id.clone(),
            start_ptr: ptr,
            base_offset: last_visual_offset(&down_ctx.id),
            index: down_ctx.index,
            count: down_ctx.count,
            per_view: down_ctx.per_view,
            wraps: wraps_nav(&down_ctx),
            loop_clones: loop_clone_count(&down_ctx),
            end_align: down_ctx.end_align,
            variant: down_ctx.variant,
            item_sizing: down_ctx.item_sizing,
            item_width_px: down_ctx.item_width_px,
            scroll_by: down_ctx.scroll_by,
            page_snap: down_ctx.page_snap,
            snap: effective_snap(&down_ctx),
            viewport_len: down_ctx.viewport_len,
            drag_px: down_ctx.drag_px,
            orientation: down_ctx.orientation,
            autoplay_ms: down_ctx.autoplay_ms,
        });
        crate::app::set_pointer_capture(capture_for_down.clone());
        e.stop_propagation();
    });
    viewport = viewport.on_pointer_up(move |e: &PointerEvent| {
        if e.phase == EventPhase::Capture {
            return;
        }
        clear_carousel_drag_if_any();
    });

    viewport
}
}

crate::__component_contract! {
CarouselItemProps [];
/// solid-ui / ShadCN `CarouselItem`。
#[allow(non_snake_case)]
pub fn CarouselItem(
plain [] children: Option<Element>,
) -> Element {
    let ctx = require_carousel("CarouselItem");
    let kids = children.map(take_packed).unwrap_or_default();
    let gap = if ctx.variant == CarouselVariant::Single || ctx.variant == CarouselVariant::Stack {
        0.0
    } else {
        // Multi slot spacing is 0.375rem on the left and right.
        0.375
    };
    // Content 会按 per_view 写入 basis%；此处只锁 shrink，避免被内容撑破。
    let mut el: Element = r#box()
        .min_w(0.0)
        .flex_shrink(0.0)
        .flex_grow(0.0)
        .children(kids)
        .into();
    if ctx.orientation == CarouselOrientation::Vertical {
        el = el.py(format!("{gap}rem")).px(0.0);
    } else {
        el = el.px(format!("{gap}rem"));
    }
    el.attrs
        .insert(ATTR_SLOT.to_string(), SLOT_ITEM.to_string());
    el = el.accesskit_role("group");
    el.set_attr("aria-roledescription", "slide");
    el
}
}

fn nav_button(
    ctx: &CarouselCtx,
    which: &'static str,
    lucide: &'static str,
    on_click: impl Fn() + 'static,
) -> Element {
    use crate::components::button::button;

    let icon = lucide_chevron(lucide);
    // 产品 Button icon-sm = 2×2rem，items/justify center；lucide 字形不强制框，圆心对齐。
    let mut btn: Element = button(format!("{}:{which}", ctx.id))
        .variant("outline")
        .size("icon-sm")
        .on_click(on_click)
        .child(icon)
        .into();
    btn = btn.rounded_full().flex_shrink(0.0);
    btn = btn.aria_label(if which == "prev" {
        crate::tr_default_zh("jadix.carousel.prev", "Previous slide", "上一张")
    } else {
        crate::tr_default_zh("jadix.carousel.next", "Next slide", "下一张")
    });
    btn.attrs.insert(
        ATTR_SLOT.to_string(),
        if which == "prev" {
            SLOT_PREV.to_string()
        } else {
            SLOT_NEXT.to_string()
        },
    );
    let sync_ctx = ctx.clone();
    let sync_which = which;
    btn = btn.reactive_sync(move |el: &mut Element, _lt: &mut LayoutTree| {
        let enabled = if sync_which == "prev" {
            can_scroll_prev(&sync_ctx)
        } else {
            can_scroll_next(&sync_ctx)
        };
        let target = if enabled { 1.0 } else { 0.45 };
        if (el.style.opacity - target).abs() > 0.01 {
            el.style.opacity = target;
            LayoutStatus::StyleChanged
        } else {
            LayoutStatus::Clean
        }
    });
    btn
}

crate::__component_contract! {
CarouselPreviousProps [];
/// solid-ui / ShadCN `CarouselPrevious`。
#[allow(non_snake_case)]
pub fn CarouselPrevious(
plain [] children: Option<Element>,
) -> Element {
    let _ = children;
    let ctx = require_carousel("CarouselPrevious");
    let c = ctx.clone();
    nav_button(&ctx, "prev", "chevron-left", move || {
        if can_scroll_prev(&c) {
            scroll_prev(&c);
        }
    })
}
}

crate::__component_contract! {
CarouselNextProps [];
/// solid-ui / ShadCN `CarouselNext`。
#[allow(non_snake_case)]
pub fn CarouselNext(
plain [] children: Option<Element>,
) -> Element {
    let _ = children;
    let ctx = require_carousel("CarouselNext");
    let c = ctx.clone();
    nav_button(&ctx, "next", "chevron-right", move || {
        if can_scroll_next(&c) {
            scroll_next(&c);
        }
    })
}
}

crate::__component_contract! {
CarouselDotsProps [];
/// 页点指示器（当前页高亮：同尺寸实心填充，不扩宽）。
#[allow(non_snake_case)]
pub fn CarouselDots(
plain [] children: Option<Element>,
) -> Element {
    let _ = children;
    let ctx = require_carousel("CarouselDots");
    let view_id = format!("{}:dots_view", ctx.id);
    let mut el = reactive_view(view_id, move || {
        let n = ctx.count.get();
        let cur = ctx.index.get();
        let mut row: Element = hstack()
            .gap("0.375rem")
            .items_center()
            .justify_center()
            .w("100%")
            .into();
        for i in 0..n {
            let on = i == cur;
            let mut dot: Element = r#box()
                .w("0.5rem")
                .h("0.5rem")
                .bg(if on {
                    tokens::primary()
                } else {
                    tokens::muted()
                })
                .into();
            dot = dot.rounded_full();
            let jump = ctx.clone();
            let idx = i;
            dot = interactive(dot)
                .focus_id(format!("{}:dot:{i}", ctx.id))
                .focusable(true)
                .on_click(move || {
                    if loop_anim(&jump.id).is_some() {
                        resolve_loop_anim_keep_offset(&jump.id, jump.index);
                    }
                    jump.index.set(idx);
                    bump_autoplay(&jump);
                    sync_offset_target(&jump);
                })
                .into();
            row = row.child(dot);
        }
        row
    });
    // The gap between the content area and the content area is taken care of by the column gap of assemble_carousel_chrome (do not write placeholder.mt:
    // reactive reconciliation will override the root style with an inner fragment).
    el = el.w("100%").h("auto").min_h("0.5rem");
    el.attrs
        .insert(ATTR_SLOT.to_string(), SLOT_DOTS.to_string());
    el
}
}
