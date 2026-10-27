//! Toast 卡片壳与 stack/list 渲染。

use super::motion::{apply_enter_motion, stack_pose_for_rank, ToastStackMotion};
use super::*;
use crate::components::button::button;
use crate::style::{
    apply_class_list, apply_semantic_chrome_respecting_overrides, dim_semantic_shadow,
    mark_variant_style_override, muted_from_accent, parse_text_color, semantic_chrome,
    set_variant_chrome_enabled, SemanticChrome, SemanticChromeDecor, SemanticChromeOpts,
    SemanticSurface, SemanticTone, VariantStyleField,
};
use crate::utils::text_node::build_named_icon_text_node;

// ---------------------------------------------------------------------------
// Visual: card + parts
// ---------------------------------------------------------------------------

pub(super) fn tone_chrome(tone: ToastTone) -> SemanticChrome {
    semantic_chrome(SemanticChromeOpts {
        tone: tone.to_semantic(),
        filled: false,
        surface: SemanticSurface::Popover,
        decor: SemanticChromeDecor::ELEVATED,
    })
}

/// `dirs`：关闭拖拽允许方向（四角两边，中位仅垂直反向）。
/// `collapse_h`：堆叠模式锁定与 front 同高，窥视条步长稳定（Sonner `--front-toast-height`）。
/// 后方层**不**再单独淡出文案——运动只加在整卡壳上，避免「空壳 + 文字晚到」。
fn build_toast_card(
    item: &ToastItem,
    dirs: ToastSwipeDirs,
    collapse_h: Option<f32>,
    behind: bool,
) -> Element {
    let id = item.id;
    let mut chrome = tone_chrome(item.tone);
    // 必须不透明，否则堆叠时后方文案会透出。
    chrome.bg[3] = 1.0;
    if behind {
        if let Some(s) = chrome.shadow {
            chrome.shadow = Some(dim_semantic_shadow(s, true));
        }
    }
    let fg = item.fg_override.unwrap_or(chrome.fg);
    let muted_fg = if item.fg_override.is_some() {
        muted_from_accent(fg)
    } else {
        chrome.muted_fg
    };
    let show_icon = item
        .show_icon
        .unwrap_or_else(|| item.tone.default_show_icon());
    let icon_name = SemanticTone::default_icon(item.tone.to_semantic());

    let mut body = vstack().gap("0.125rem").flex_grow(1.0).min_w(0.0);
    if !item.title.is_empty() {
        let title_el: Element = text(item.title.as_str())
            .size(14.0)
            .weight(600.0)
            .color(fg)
            .into();
        body = body.child(title_el);
    }
    if !item.description.is_empty() {
        let desc: Element = text(item.description.as_str())
            .size(14.0)
            .color(muted_fg)
            .into();
        body = body.child(desc);
    }

    let mut row = hstack()
        .w("100%")
        .items("center")
        .gap("0.75rem")
        .justify("between");

    if show_icon {
        if let Some(name) = icon_name {
            if let Some(g) = lucide_glyph(name) {
                // 独立左侧图标列，纵向居中（与文案区分离）。
                let icon = build_named_icon_text_node(name, g)
                    .text_size(18.0)
                    .text_color(fg)
                    .w(18.0)
                    .h(18.0)
                    .flex_shrink(0.0);
                row = row.child(icon);
            }
        }
    }

    row = row.child(Element::from(body));

    let close_glyph = lucide_glyph("x").unwrap_or_else(|| "×".into());
    let close_icon = build_named_icon_text_node("x", close_glyph)
        .text_size(14.0)
        .text_color({
            let mut c = fg;
            c[3] = (c[3] as f32 * 0.55) as u8;
            c
        });
    let mut close_btn: Element = button(format!("toast-close-{id}"))
        .variant("ghost")
        .size("icon-sm")
        .on_click(move || toast_dismiss(id))
        .child(close_icon)
        .into();
    close_btn = close_btn.on_pointer_down(move |e: &PointerEvent| {
        // 避免关闭钮按下启动整卡拖拽。
        e.stop_propagation();
    });
    row = row.child(close_btn.flex_shrink(0.0));

    let content: Element = Element::from(row);

    let capture_id = item_template_id(TOAST_CARD_SCOPE, id);
    let capture_for_down = capture_id.clone();

    let mut card: Element = r#box()
        .w("100%")
        .min_h(CARD_H)
        .px("1rem")
        .py("0.875rem")
        .pr("0.75rem")
        .rounded("0.5rem")
        .child(content)
        .into();
    if let Some(h) = collapse_h {
        // 与 front 同高 + 裁切，窥视条步长才稳定（Sonner `--front-toast-height`）。
        card = card.h(h).overflow("hidden");
    }
    set_variant_chrome_enabled(&mut card, item.chrome);
    if let Some(bg) = item.bg_override {
        card.style.bg_color = bg;
        mark_variant_style_override(&mut card, VariantStyleField::Bg);
    }
    if let Some(bc) = item.border_override {
        card.style.border_color = bc;
        mark_variant_style_override(&mut card, VariantStyleField::Border);
    }
    if item.fg_override.is_some() {
        mark_variant_style_override(&mut card, VariantStyleField::Fg);
    }
    apply_semantic_chrome_respecting_overrides(&mut card, &chrome);
    if let Some(ref cls) = item.utility_class {
        let _ = apply_class_list(&mut card, cls);
    }
    card = card
        .opacity(1.0)
        .focus_id(capture_id)
        .on_pointer_down(move |e: &PointerEvent| {
            if e.phase == EventPhase::Capture {
                return;
            }
            begin_toast_drag(ToastDrag {
                id,
                start_x: e.x,
                start_y: e.y,
                dirs,
            });
            crate::app::set_pointer_capture(capture_for_down.clone());
            e.stop_propagation();
        })
        .on_pointer_up(move |e: &PointerEvent| {
            if e.phase == EventPhase::Capture {
                return;
            }
            clear_toast_drag_if_any();
        });
    card
}

crate::__component_contract! {
ToastProps [];
/// 声明式条目壳（语气变体）；一般由宿主渲染队列，亦可自定义。
#[allow(non_snake_case)]
pub fn Toast(
plain [] variant: Option<String>,
plain [] title: Option<String>,
plain [] description: Option<String>,
plain [] icon: Option<bool>,
plain [] chrome: Option<bool>,
plain [] children: Option<Element>,
) -> Element {
    let _ = children;
    let tone = ToastTone::parse(variant.as_deref().unwrap_or("default"));
    let item = ToastItem {
        id: 0,
        title: title.unwrap_or_default(),
        description: description.unwrap_or_default(),
        tone,
        show_icon: icon,
        chrome: chrome.unwrap_or(true),
        utility_class: None,
        bg_override: None,
        border_override: None,
        fg_override: None,
        created: Instant::now(),
        duration: Duration::from_secs(3),
        exiting: false,
        exit_started: None,
    };
    build_toast_card(
        &item,
        swipe_dirs_for_position(ToastPosition::TopRight),
        None,
        false,
    )
}
}

crate::__component_contract! {
ToastTitleProps [];
/// 标题叶（文档/自定义组合用）。
#[allow(non_snake_case)]
pub fn ToastTitle(
plain [] children: Option<Element>,
) -> Element {
    let label = children
        .as_ref()
        .and_then(|c| c.text.clone())
        .unwrap_or_default();
    text(label.as_str())
        .size(14.0)
        .weight(600.0)
        .color(tokens::foreground_text())
        .into()
}
}

crate::__component_contract! {
ToastDescriptionProps [];
/// 描述叶。
#[allow(non_snake_case)]
pub fn ToastDescription(
plain [] children: Option<Element>,
) -> Element {
    let label = children
        .as_ref()
        .and_then(|c| c.text.clone())
        .unwrap_or_default();
    text(label.as_str())
        .size(14.0)
        .color(parse_text_color("muted-foreground"))
        .into()
}
}

crate::__component_contract! {
ToastCloseProps [];
/// 关闭按钮（需自行 `on_click` dismiss；队列卡片已内置关闭）。
#[allow(non_snake_case)]
pub fn ToastClose(
plain [] children: Option<Element>,
) -> Element {
    let _ = children;
    let g = lucide_glyph("x").unwrap_or_else(|| "×".into());
    let icon = build_named_icon_text_node("x", g)
        .text_size(14.0)
        .text_color(parse_text_color("muted-foreground"));
    button("toast-close-part")
        .variant("ghost")
        .size("icon-sm")
        .child(icon)
        .into()
}
}

// ---------------------------------------------------------------------------
// Host layout
// ---------------------------------------------------------------------------

fn place_panel(panel: Element, position: &str) -> Element {
    let pad = 16.0;
    let pos = ToastPosition::parse(position);
    match pos {
        ToastPosition::TopLeft => panel.absolute().left(pad).top(pad),
        ToastPosition::TopRight => panel.absolute().right(pad).top(pad),
        ToastPosition::BottomLeft => panel.absolute().left(pad).bottom(pad),
        ToastPosition::BottomRight => panel.absolute().right(pad).bottom(pad),
        ToastPosition::TopCenter => Element::from(
            hstack()
                .absolute(true)
                .left(0.0)
                .right(0.0)
                .top(pad)
                .w("100%")
                .justify("center")
                .pointer_events("box-none")
                .child(panel),
        ),
        ToastPosition::BottomCenter => Element::from(
            hstack()
                .absolute(true)
                .left(0.0)
                .right(0.0)
                .bottom(pad)
                .w("100%")
                .justify("center")
                .pointer_events("box-none")
                .child(panel),
        ),
    }
}

/// Sonner 式堆叠：收起时仅窥视后方条目；悬停展开为完整间距列表。
/// `items` 新→旧（index 0 = 最前）。
/// 布局：同锚点 + `translateY` 堆叠（避免中心缩放 + 不同 `top` 导致窥视条不匀）。
/// 后方到期：整卡淡出；最前到期：滑出淡出。
/// 展开态由卡/堆 `reactive_sync` 热读，宿主不因悬停重建（3+ 卡性能关键）。
fn render_stack(
    items: &[&ToastItem],
    expanded: bool,
    from_bottom: bool,
    swipe: ToastSwipeDirs,
) -> Element {
    let n = items.len();
    if n == 0 {
        return Element::new();
    }

    // 仅非退场项参与堆叠位次与堆高度（退场后方不占位，对标 Sonner 立即收高度）。
    let mut rank_of = vec![None; n];
    let mut n_active = 0usize;
    for i in 0..n {
        if !items[i].exiting {
            rank_of[i] = Some(n_active);
            n_active += 1;
        }
    }
    let has_front_exit = items
        .iter()
        .any(|t| t.exiting && toast_exit_vis(t.id).is_some_and(|v| v.as_front));
    let pile_n = n_active.max(usize::from(has_front_exit));

    let pile_h_expanded =
        pile_n as f32 * CARD_H + (pile_n.saturating_sub(1) as f32) * STACK_EXPAND_GAP;
    // 收起时命中区跟窥视堆高度走，避免展开幽灵高度拖住 hover 导致「移开却不收」。
    // 展开后用满高，保证卡间空隙仍可命中。高度由 pile sync 热更新，不重建子树。
    let pile_h_collapsed = CARD_H + (pile_n.saturating_sub(1) as f32) * STACK_PEEK;
    let pile_h = if expanded {
        pile_h_expanded
    } else {
        pile_h_collapsed
    };
    let anchor_top = if from_bottom { pile_h - CARD_H } else { 0.0 };

    let mut pile: Element = r#box()
        .relative(true)
        .w(TOAST_WIDTH)
        .h(pile_h)
        .overflow("visible")
        .into();
    // 与 CarouselStack 相同：逐卡 rects→text band，避免下层字形画在上层壳之上。
    // （框架也会在子节点有 z-index 时自动建 band；此处显式标注语义。）
    pile.set_attr("data-stacking-context", "1");
    pile.template_id = Some("__jadix_toast_stack_pile".into());
    pile.set_attr(crate::authoring::widgets::hit::ATTR_HOVER_REGION, "1");

    let last_pile_h = Cell::new(pile_h);
    pile = pile.reactive_sync(move |el: &mut Element, _lt: &mut LayoutTree| {
        let exp = stack_expanded_now();
        let h = if exp {
            pile_h_expanded
        } else {
            pile_h_collapsed
        };
        if (last_pile_h.get() - h).abs() <= 0.5 {
            return LayoutStatus::Clean;
        }
        last_pile_h.set(h);
        el.style.set_css_dim(UnitSlot::Height, CssDim::Px(h));
        if from_bottom {
            let top = h - CARD_H;
            for child in &mut el.children {
                if crate::utils::keyed_overlay::is_keyed_motion_item(child.template_id.as_deref()) {
                    child.style.set_css_dim(UnitSlot::InsetT, CssDim::Px(top));
                }
            }
        }
        LayoutStatus::LayoutChanged
    });

    // 从后往前挂：先画最旧（底），再画最新（顶），保证命中/绘制顺序。
    for draw_i in (0..n).rev() {
        let item = items[draw_i];
        let exit_vis = toast_exit_vis(item.id);
        let behind_fade = item.exiting && !expanded && exit_vis.is_some_and(|v| !v.as_front);

        let (rank, behind, pointer_none, z, live_rank) = if item.exiting {
            if let Some(v) = exit_vis {
                if v.as_front {
                    (0usize, false, true, 2000, Some(0usize))
                } else {
                    let rank = v.rank;
                    (rank, true, true, 900 - rank as i32, Some(rank))
                }
            } else {
                (0usize, false, true, 2000, Some(0usize))
            }
        } else {
            let rank = rank_of[draw_i].unwrap_or(0);
            let is_behind = !expanded && rank > 0;
            (rank, is_behind, is_behind, 1000 - rank as i32, Some(rank))
        };

        let (base_lift, base_scale, _) = stack_pose_for_rank(rank, from_bottom, expanded);

        // 后方窥视条不重播入场滑入（晋升 front 时再播）。
        if behind {
            let _ = take_toast_needs_enter(item.id);
        }
        // 整壳裁切到 front 同高；文案始终绘制，靠 overflow 裁切窥视。
        let collapse_h = Some(CARD_H);
        let mut card = build_toast_card(item, swipe, collapse_h, behind);
        card = apply_enter_motion(
            card,
            item,
            from_bottom,
            behind_fade,
            Some(ToastStackMotion {
                target_lift: base_lift,
                target_scale: base_scale,
                behind_fade,
                dim_shadow: behind,
                live_rank,
            }),
        );
        card = card
            .absolute()
            .left(0.0)
            .w("100%")
            .top(anchor_top)
            .z_index(z);
        if pointer_none {
            card = card.pointer_events_none();
        }
        pile = pile.child(card);
    }

    interactive(Element::from(pile))
        .on_hover(move |h| {
            if is_toast_dragging() {
                if h {
                    set_stack_expanded(true);
                    set_stack_timers_paused(true);
                }
                return;
            }
            set_stack_expanded(h);
            set_stack_timers_paused(h);
        })
        .into()
}

/// list：绝对定位 + translateY 占位（与 stack 同口径），index 变化才有过渡。
/// `items`：顶→底视觉序。底栏传入 oldest→newest（最上最旧）；顶栏 newest→oldest。
/// 挤出最旧：向入场推移方向再移一档（底栏向上 / 顶栏向下）并淡出。
fn render_list(items: &[&ToastItem], from_bottom: bool, swipe: ToastSwipeDirs) -> Element {
    let n = items.len();
    if n == 0 {
        return Element::new();
    }

    let step = list_step();
    let mut rank_of = vec![None; n];
    let actives: Vec<usize> = (0..n).filter(|&i| !items[i].exiting).collect();
    let n_active = actives.len();
    if from_bottom {
        // oldest→newest：末项最新 → rank 0（贴底边）。
        for (k, &i) in actives.iter().rev().enumerate() {
            rank_of[i] = Some(k);
        }
    } else {
        // newest→oldest：首项最新 → rank 0（贴顶边）。
        for (k, &i) in actives.iter().enumerate() {
            rank_of[i] = Some(k);
        }
    }

    let has_front_exit = items
        .iter()
        .any(|t| t.exiting && toast_exit_vis(t.id).is_some_and(|v| v.as_front));
    let pile_n = n_active.max(usize::from(has_front_exit));
    let pile_h = pile_n as f32 * CARD_H + (pile_n.saturating_sub(1) as f32) * LIST_GAP;
    let anchor_top = if from_bottom { pile_h - CARD_H } else { 0.0 };

    let mut pile: Element = r#box()
        .relative(true)
        .w(TOAST_WIDTH)
        .max_w("420px")
        .h(pile_h)
        .overflow("visible")
        .into();

    // 先画高 rank（更远），再画 rank0，保证命中/绘制。
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| {
        let item = items[i];
        let r = if item.exiting {
            toast_exit_vis(item.id).map(|v| v.rank).unwrap_or(0)
        } else {
            rank_of[i].unwrap_or(0)
        };
        std::cmp::Reverse(r)
    });

    for draw_i in order {
        let item = items[draw_i];
        let exit_vis = toast_exit_vis(item.id);
        let as_front = exit_vis.is_some_and(|v| v.as_front);
        // 挤出 / 非最前关闭：按档位平移淡出；最前关闭：滑向入场边。
        let list_slot_fade = item.exiting && !as_front;

        let rank = if item.exiting {
            if as_front {
                0usize
            } else {
                exit_vis.map(|v| v.rank).unwrap_or_else(visible_limit)
            }
        } else {
            rank_of[draw_i].unwrap_or(0)
        };

        let base_lift = linear_slot_offset(rank, step, !from_bottom);

        let mut card = build_toast_card(item, swipe, None, false);
        card = apply_enter_motion(
            card,
            item,
            from_bottom,
            list_slot_fade,
            Some(ToastStackMotion {
                target_lift: base_lift,
                target_scale: 1.0,
                behind_fade: list_slot_fade,
                dim_shadow: false,
                live_rank: None,
            }),
        );
        let z = 1000i32 - rank as i32;
        card = card
            .absolute()
            .left(0.0)
            .w("100%")
            .top(anchor_top)
            .z_index(z);

        let id = item.id;
        let card: Element = if item.exiting {
            card.pointer_events_none()
        } else {
            card.set_attr(crate::authoring::widgets::hit::ATTR_HOVER_REGION, "1");
            interactive(card)
                .on_hover(move |h| {
                    set_item_timer_paused(id, h);
                })
                .into()
        };
        pile = pile.child(card);
    }

    Element::from(pile)
}

pub(super) fn render_queue(
    layout: ToastLayout,
    position: &str,
    q: &[ToastItem],
    expanded: bool,
) -> Element {
    let limit = visible_limit();
    let active_ids: std::collections::HashSet<u64> = q
        .iter()
        .rev()
        .filter(|t| !t.exiting)
        .take(limit)
        .map(|t| t.id)
        .collect();
    let newest_first: Vec<&ToastItem> = q
        .iter()
        .rev()
        .filter(|t| t.exiting || active_ids.contains(&t.id))
        .collect();
    if newest_first.is_empty() {
        return Element::new();
    }

    let pos = ToastPosition::parse(position);
    let bottom = pos.is_bottom();
    let swipe = swipe_dirs_for_position(pos);
    let expanded = expanded || is_toast_dragging();
    let body = match layout {
        ToastLayout::Stack => render_stack(&newest_first, expanded, bottom, swipe),
        ToastLayout::List => {
            if bottom {
                let oldest_first: Vec<&ToastItem> = newest_first.into_iter().rev().collect();
                render_list(&oldest_first, bottom, swipe)
            } else {
                render_list(&newest_first, bottom, swipe)
            }
        }
    };

    place_panel(r#box().child(body).into(), position)
}

#[cfg(all(test, feature = "product-tests"))]
mod tests {
    use super::*;
    use crate::style::{
        is_variant_style_overridden, parse_color, variant_chrome_enabled, VariantStyleField,
    };

    fn sample_item() -> ToastItem {
        ToastItem {
            id: 1,
            title: "T".into(),
            description: String::new(),
            tone: ToastTone::Success,
            show_icon: None,
            chrome: true,
            utility_class: None,
            bg_override: None,
            border_override: None,
            fg_override: None,
            created: Instant::now(),
            duration: Duration::from_secs(3),
            exiting: false,
            exit_started: None,
        }
    }

    #[test]
    fn toast_child_hover_expands_stack_and_freezes_timers() {
        use crate::app::tree_hit::{
            apply_pointer_hover_reconcile, clear_hover_tracking, find_paint_aabb_by_focus_id,
        };
        use crate::core::reactivity::create_root;

        for layout in [ToastLayout::Stack, ToastLayout::List] {
            clear_hover_tracking();
            let (_owner, (mut root, ids)) = create_root(|| {
                toast_set_layout(if layout == ToastLayout::Stack {
                    "stack"
                } else {
                    "list"
                });
                toast_set_overflow("displace");
                toast_set_limit(3);
                toast_queue().set(vec![]);
                toast_pending().set(vec![]);
                set_stack_expanded(false);
                set_stack_timers_paused(false);
                let ids = [
                    toast("Older").duration(10_000).show(),
                    toast("Saved")
                        .description("Changes applied.")
                        .duration(10_000)
                        .show(),
                ];
                for id in ids {
                    let _ = take_toast_needs_enter(id);
                }
                let panel = render_queue(layout, "top-left", &toast_queue().peek(), false);
                (Element::new().w(800.0).h(600.0).child(panel), ids)
            });
            let mut tree = LayoutTree::new();
            let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
            let region_id = if layout == ToastLayout::Stack {
                "__jadix_toast_stack_pile".to_string()
            } else {
                item_template_id(TOAST_CARD_SCOPE, ids[1])
            };
            let transitions = std::rc::Rc::new(RefCell::new(Vec::new()));
            let seen = transitions.clone();
            let region = crate::core::tree::find_in_tree_mut(&mut root, |el| {
                el.template_id.as_deref() == Some(region_id.as_str())
            })
            .expect("notification hover region");
            let on_hover = region.on_hover.clone().unwrap();
            region.on_hover = Some(std::rc::Rc::new(RefCell::new(move |hovered| {
                seen.borrow_mut().push(hovered);
                (on_hover.borrow_mut())(hovered);
            })));
            let close_id = format!("toast-close-{}", ids[1]);
            let (bounds, _) = find_paint_aabb_by_focus_id(&root, &close_id, [0.0, 0.0])
                .expect("close button must be laid out");
            let close = [bounds[0] + bounds[2] / 2.0, bounds[1] + bounds[3] / 2.0];
            let body = [bounds[0] - 30.0, close[1]];

            // Enter the interactive child directly, then move within the notification.
            for point in [close, body, close] {
                apply_pointer_hover_reconcile(&root, point[0], point[1], true);
                assert!(
                    toast_timer_deadline(ids[1]).is_none(),
                    "{layout:?}: child hover must freeze dismissal"
                );
                if layout == ToastLayout::Stack {
                    assert!(
                        stack_expanded_now(),
                        "close button hover must expand the stack"
                    );
                    assert!(
                        toast_timer_deadline(ids[0]).is_none(),
                        "stack hover must freeze every toast"
                    );
                } else {
                    assert!(
                        toast_timer_deadline(ids[0]).is_some(),
                        "list hover must only freeze its own toast"
                    );
                }
                assert!(
                    crate::core::reactivity::use_signal(&format!("{close_id}_hover"), || false)
                        .peek()
                        == (point == close),
                    "close button must retain its own hover chrome"
                );
            }
            apply_pointer_hover_reconcile(&root, 799.0, 599.0, true);
            assert_eq!(
                *transitions.borrow(),
                [true, false],
                "moving within the toast must not collapse or briefly resume timers"
            );
            assert!(!stack_expanded_now());
            assert!(
                ids.iter().all(|id| toast_timer_deadline(*id).is_some()),
                "leaving must resume timers"
            );
            clear_hover_tracking();
            toast_queue().set(vec![]);
            for id in ids {
                timer::clear(id);
            }
        }
    }

    #[test]
    fn toast_chrome_false_skips_semantic_paint() {
        let mut item = sample_item();
        item.chrome = false;
        let card = build_toast_card(
            &item,
            swipe_dirs_for_position(ToastPosition::TopRight),
            None,
            false,
        );
        assert!(!variant_chrome_enabled(&card.attrs));
        let chromed = build_toast_card(
            &sample_item(),
            swipe_dirs_for_position(ToastPosition::TopRight),
            None,
            false,
        );
        assert_ne!(card.style.bg_color, chromed.style.bg_color);
    }

    #[test]
    fn toast_bg_override_wins_over_variant() {
        let mut item = sample_item();
        let custom = parse_color("red-500");
        item.bg_override = Some(custom);
        let card = build_toast_card(
            &item,
            swipe_dirs_for_position(ToastPosition::TopRight),
            None,
            false,
        );
        assert_eq!(card.style.bg_color, custom);
        assert!(is_variant_style_overridden(
            &card.attrs,
            VariantStyleField::Bg
        ));
    }

    #[test]
    fn toast_utility_class_applied_after_chrome() {
        let mut item = sample_item();
        item.utility_class = Some("bg-card".into());
        let card = build_toast_card(
            &item,
            swipe_dirs_for_position(ToastPosition::TopRight),
            None,
            false,
        );
        assert_eq!(card.style.bg_color, parse_color("card"));
    }
}
