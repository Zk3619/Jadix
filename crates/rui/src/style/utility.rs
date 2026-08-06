//! Runtime utility-class application — single apply path for static `ui!` classes and
//! dynamic `bind_class` / `bind_style` updates.
//!
//! Token recognition lives in [`rui_style_tokens`] so the macro can warn on unknown
//! classes against the same catalog.

use rui_style_tokens::{
    classify_utility, parse_class_list, KnownUtility, ParsedClassList, VariantSelector,
};

use crate::core::tree::{Element, LayoutStatus};
use crate::layout::cols_equal_fr;
use crate::primitives::interactive::mark_hover_paint_dirty;
use crate::style::{parse_color, parse_text_color, Style, TextStyle};

/// Snapshot + variant rules attached to an [`Element`] after [`apply_class_list`].
#[derive(Clone, Debug, Default)]
pub struct ClassSet {
    pub base: Vec<String>,
    /// Each entry: all selectors must match (AND), then apply the utility.
    pub variants: Vec<(Vec<VariantSelector>, String)>,
    pub base_style: Option<Style>,
    pub base_text_style: Option<TextStyle>,
}

/// Interaction / theme context used to resolve variant utilities each frame.
#[derive(Clone, Debug, Default)]
pub struct VariantContext<'a> {
    pub hovered: bool,
    pub focused: bool,
    pub disabled: bool,
    pub active: bool,
    pub dark: bool,
    /// Raw element attrs (for `data-[state=…]`).
    pub attrs: Option<&'a std::collections::HashMap<String, String>>,
}

impl VariantContext<'_> {
    fn matches_selector(&self, sel: &VariantSelector) -> bool {
        match sel {
            VariantSelector::Hover => self.hovered,
            VariantSelector::Focus | VariantSelector::FocusVisible => self.focused,
            VariantSelector::Active => self.active,
            VariantSelector::Disabled => self.disabled,
            VariantSelector::Dark => self.dark,
            VariantSelector::Data { key, value } => {
                let Some(attrs) = self.attrs else {
                    return false;
                };
                let data_key = format!("data-{key}");
                attrs
                    .get(&data_key)
                    .or_else(|| attrs.get(key))
                    .is_some_and(|v| v == value)
            }
        }
    }
}

/// Apply a whitespace-separated utility / variant class list onto `el`.
///
/// Base tokens are applied immediately and snapshotted. Variant tokens are stored on
/// [`Element::class_set`] and re-applied by [`resolve_class_variants`].
pub fn apply_class_list(el: &mut Element, classes: &str) -> LayoutStatus {
    let parsed = parse_class_list(classes);
    apply_parsed_class_list(el, parsed, classes)
}

/// Backward-compatible alias used by [`crate::app::reactive::bind_class`].
#[inline]
pub fn apply_utility_classes(el: &mut Element, classes: &str) -> LayoutStatus {
    apply_class_list(el, classes)
}

/// Builder-friendly: apply classes and return the element.
pub fn with_class_list(mut el: Element, classes: &str) -> Element {
    let _ = apply_class_list(&mut el, classes);
    el
}

fn apply_parsed_class_list(
    el: &mut Element,
    parsed: ParsedClassList,
    raw_classes: &str,
) -> LayoutStatus {
    let mut impact = LayoutStatus::Clean;
    for token in &parsed.base {
        impact = impact.merge(apply_known_utility(el, token));
    }

    let class_set = ClassSet {
        base: parsed.base,
        variants: parsed.variants,
        base_style: Some(el.style.clone()),
        base_text_style: Some(el.text_style.clone()),
    };
    el.class_set = Some(class_set);
    el.attrs
        .insert("rui_utility_class".to_string(), raw_classes.to_string());
    impact
}

/// Restore base snapshot and apply currently-matching variant utilities.
pub fn resolve_class_variants(el: &mut Element, ctx: &VariantContext<'_>) -> LayoutStatus {
    let Some(cs) = el.class_set.as_ref() else {
        return LayoutStatus::Clean;
    };
    if cs.variants.is_empty() {
        return LayoutStatus::Clean;
    }

    if let Some(s) = cs.base_style.as_ref() {
        el.style = s.clone();
    }
    if let Some(t) = cs.base_text_style.as_ref() {
        el.text_style = t.clone();
    }

    let mut impact = LayoutStatus::Clean;
    // Collect matching utilities first to avoid borrow issues.
    let mut apply: Vec<String> = Vec::new();
    if let Some(cs) = el.class_set.as_ref() {
        for (sels, util) in &cs.variants {
            if sels.iter().all(|s| ctx.matches_selector(s)) {
                apply.push(util.clone());
            }
        }
    }
    for util in apply {
        impact = impact.merge(apply_known_utility(el, &util));
    }
    impact
}

/// Walk the tree and resolve class variants (chrome sync).
pub fn sync_class_variants_subtree(
    el: &mut Element,
    mouse_pos: [f32; 2],
    focused_id: &str,
    dark: bool,
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
    impact: &mut LayoutStatus,
) {
    let hovered = point_in_bounds(mouse_pos, el.layout_bounds);
    let focused = el
        .focus_id
        .as_deref()
        .is_some_and(|id| !id.is_empty() && id == focused_id);
    let disabled = el.attrs.get("disabled").is_some_and(|v| v != "false")
        || el.attrs.get("aria-disabled").is_some_and(|v| v == "true");
    let attrs_snapshot = el.attrs.clone();
    let ctx = VariantContext {
        hovered,
        focused,
        disabled,
        active: false,
        dark,
        attrs: Some(&attrs_snapshot),
    };
    let before = el.style.clone();
    let before_text = el.text_style.clone();
    let local = resolve_class_variants(el, &ctx);
    if local != LayoutStatus::Clean || el.style != before || el.text_style != before_text {
        *impact = impact.merge(local);
        mark_hover_paint_dirty();
        if el.layout_node.is_some() {
            layout_tree.update_taffy_node_style(el);
        }
    }
    for child in &mut el.children {
        sync_class_variants_subtree(child, mouse_pos, focused_id, dark, layout_tree, impact);
    }
}

fn point_in_bounds(pos: [f32; 2], b: [f32; 4]) -> bool {
    pos[0] >= b[0] && pos[1] >= b[1] && pos[0] < b[0] + b[2] && pos[1] < b[1] + b[3]
}

fn apply_known_utility(el: &mut Element, token: &str) -> LayoutStatus {
    let Some(known) = classify_utility(token) else {
        return LayoutStatus::Clean;
    };
    match known {
        KnownUtility::FlexRow => {
            *el = std::mem::take(el).flex_row();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::FlexCol => {
            *el = std::mem::take(el).flex_col();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::FlexWrap => {
            *el = std::mem::take(el).flex_wrap_kw("wrap");
            LayoutStatus::LayoutChanged
        }
        KnownUtility::FlexNoWrap => {
            *el = std::mem::take(el).flex_wrap_kw("nowrap");
            LayoutStatus::LayoutChanged
        }
        KnownUtility::DisplayGrid => {
            *el = std::mem::take(el).display_grid();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::ItemsCenter => {
            *el = std::mem::take(el).items_center();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::JustifyCenter => {
            *el = std::mem::take(el).justify_center();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Grow(v) => {
            *el = std::mem::take(el).flex_grow(v);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Shrink(v) => {
            *el = std::mem::take(el).flex_shrink(v);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Overflow(v) => {
            *el = std::mem::take(el).overflow(v);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::OverflowX(v) => {
            *el = std::mem::take(el).overflow_x(v);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::OverflowY(v) => {
            *el = std::mem::take(el).overflow_y(v);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::RoundedFull => {
            *el = std::mem::take(el).rounded_full();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Rounded(px) => {
            *el = std::mem::take(el).rounded(px);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::GridCols(n) => {
            *el = std::mem::take(el)
                .display_grid()
                .grid_template_columns(cols_equal_fr(n));
            LayoutStatus::LayoutChanged
        }
        KnownUtility::GridRows(n) => {
            *el = std::mem::take(el)
                .display_grid()
                .grid_template_rows(cols_equal_fr(n));
            LayoutStatus::LayoutChanged
        }
        KnownUtility::GapX(v) => {
            *el = std::mem::take(el).gap_x(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::GapY(v) => {
            *el = std::mem::take(el).gap_y(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Gap(px) => {
            *el = std::mem::take(el).gap(px);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Width(v) => {
            *el = std::mem::take(el).w(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Height(v) => {
            *el = std::mem::take(el).h(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MinWidth(v) => {
            *el = std::mem::take(el).min_w(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MaxWidth(v) => {
            *el = std::mem::take(el).max_w(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MinHeight(v) => {
            *el = std::mem::take(el).min_h(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MaxHeight(v) => {
            *el = std::mem::take(el).max_h(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Padding(v) => {
            *el = std::mem::take(el).p(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PaddingX(v) => {
            *el = std::mem::take(el).px(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PaddingY(v) => {
            *el = std::mem::take(el).py(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PaddingT(v) => {
            *el = std::mem::take(el).pt(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PaddingR(v) => {
            *el = std::mem::take(el).pr(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PaddingB(v) => {
            *el = std::mem::take(el).pb(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PaddingL(v) => {
            *el = std::mem::take(el).pl(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Margin(v) => {
            *el = std::mem::take(el).m(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MarginX(v) => {
            *el = std::mem::take(el).mx(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MarginY(v) => {
            *el = std::mem::take(el).my(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MarginT(v) => {
            *el = std::mem::take(el).mt(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MarginR(v) => {
            *el = std::mem::take(el).mr(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MarginB(v) => {
            *el = std::mem::take(el).mb(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MarginL(v) => {
            *el = std::mem::take(el).ml(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::BgColor(name) => {
            let color = parse_color(name);
            if el.style.bg_color != color {
                el.style.bg_color = color;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TextColor(name) => {
            let color = parse_text_color(name);
            if el.text_style.color != color {
                el.text_style.color = color;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TextSize(px) => {
            if (el.text_style.size - px).abs() > f32::EPSILON {
                el.text_style.size = px;
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::FontWeight(w) => {
            let w = w.weight_u16();
            if el.text_style.weight != w {
                el.text_style.weight = w;
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::AlignSelf(kw) => {
            *el = std::mem::take(el).align_self_kw(kw);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Opacity(v) => {
            if (el.style.opacity - v).abs() > f32::EPSILON {
                el.style.opacity = v;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TranslateX(px) => {
            if (el.style.translate[0] - px).abs() > f32::EPSILON {
                el.style.translate[0] = px;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TranslateY(px) => {
            if (el.style.translate[1] - px).abs() > f32::EPSILON {
                el.style.translate[1] = px;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::Truncate => {
            use crate::style::TextOverflow;
            let mut changed = LayoutStatus::Clean;
            if el.text_style.overflow != TextOverflow::Ellipsis {
                el.text_style.overflow = TextOverflow::Ellipsis;
                changed = LayoutStatus::StyleChanged;
            }
            // Match Tailwind `truncate`: overflow hidden + nowrap + ellipsis.
            let overflow_changed = el.style.overflow.x != taffy::style::Overflow::Hidden
                || el.style.overflow.y != taffy::style::Overflow::Hidden;
            if overflow_changed {
                el.style.overflow.x = taffy::style::Overflow::Hidden;
                el.style.overflow.y = taffy::style::Overflow::Hidden;
                changed = LayoutStatus::LayoutChanged;
            }
            changed
        }

        KnownUtility::PositionFixed => {
            *el = std::mem::take(el).fixed();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PositionAbsolute => {
            *el = std::mem::take(el).absolute();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PositionRelative => {
            el.style.position_fixed = false;
            *el = std::mem::take(el).relative();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::AspectRatio(r) => {
            if el.style.aspect_ratio != Some(r) {
                el.style.aspect_ratio = Some(r);
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::Rotate(deg) => {
            if (el.style.rotate_deg - deg).abs() > f32::EPSILON {
                el.style.rotate_deg = deg;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::Scale(s) => {
            if (el.style.scale - s).abs() > f32::EPSILON {
                el.style.scale = s;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::BgGradientTo(angle) => {
            let mut g = el.style.bg_gradient.unwrap_or_default();
            g.angle_deg = angle;
            el.style.bg_gradient = Some(g);
            LayoutStatus::StyleChanged
        }
        KnownUtility::GradientFrom(name) => {
            let color = parse_color(name);
            let mut g = el.style.bg_gradient.unwrap_or_default();
            g.from = color;
            el.style.bg_gradient = Some(g);
            LayoutStatus::StyleChanged
        }
        KnownUtility::GradientTo(name) => {
            let color = parse_color(name);
            let mut g = el.style.bg_gradient.unwrap_or_default();
            g.to = color;
            el.style.bg_gradient = Some(g);
            LayoutStatus::StyleChanged
        }
        KnownUtility::LetterSpacing(px) => {
            if (el.text_style.letter_spacing - px).abs() > f32::EPSILON {
                el.text_style.letter_spacing = px;
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TextDirectionLtr => {
            use crate::style::TextDirection;
            if el.text_style.direction != TextDirection::Ltr {
                el.text_style.direction = TextDirection::Ltr;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TextDirectionRtl => {
            use crate::style::TextDirection;
            if el.text_style.direction != TextDirection::Rtl {
                el.text_style.direction = TextDirection::Rtl;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rui_style_tokens::parse_class_token;

    #[test]
    fn apply_base_and_store_variants() {
        let mut el = Element::new();
        let impact = apply_class_list(&mut el, "flex p-4 hover:bg-accent dark:bg-primary");
        assert_ne!(impact, LayoutStatus::Clean);
        let cs = el.class_set.as_ref().unwrap();
        assert!(cs.base.iter().any(|t| t == "flex"));
        assert_eq!(cs.variants.len(), 2);
    }

    #[test]
    fn resolve_hover() {
        let mut el = Element::new();
        apply_class_list(&mut el, "bg-transparent hover:bg-accent");
        let base_bg = el.style.bg_color;
        let _ = resolve_class_variants(
            &mut el,
            &VariantContext {
                hovered: true,
                ..Default::default()
            },
        );
        assert_ne!(el.style.bg_color, base_bg);
    }

    #[test]
    fn apply_opacity_tokens() {
        let mut el = Element::new();
        let impact = apply_class_list(&mut el, "opacity-50");
        assert_eq!(impact, LayoutStatus::StyleChanged);
        assert_eq!(el.style.opacity, 0.5);

        let mut el = Element::new();
        apply_class_list(&mut el, "opacity-[0.4]");
        assert_eq!(el.style.opacity, 0.4);
    }

    #[test]
    fn apply_translate_and_truncate() {
        let mut el = Element::new();
        apply_class_list(&mut el, "translate-x-4 translate-y-2");
        assert_eq!(el.style.translate, [16.0, 8.0]);

        let mut el = Element::new().text("long");
        apply_class_list(&mut el, "truncate");
        assert_eq!(el.text_style.overflow, crate::style::TextOverflow::Ellipsis);
        assert_eq!(el.style.overflow.x, taffy::style::Overflow::Hidden);
    }

    #[test]
    fn parse_stacked() {
        let t = parse_class_token("dark:hover:bg-muted");
        assert_eq!(t.utility, "bg-muted");
        assert_eq!(t.variants.len(), 2);
    }
}

#[cfg(test)]
mod p1_utility_tests {
    use super::*;

    #[test]
    fn apply_aspect_ratio_utility() {
        let mut el = Element::new();
        let impact = apply_class_list(&mut el, "aspect-square");
        assert_eq!(impact, LayoutStatus::LayoutChanged);
        assert_eq!(el.style.aspect_ratio, Some(1.0));

        let mut el = Element::new();
        apply_class_list(&mut el, "aspect-[16/9]");
        assert!((el.style.aspect_ratio.unwrap() - 16.0 / 9.0).abs() < 0.001);
    }

    #[test]
    fn apply_fixed_and_rtl() {
        let mut el = Element::new();
        apply_class_list(&mut el, "fixed");
        assert!(el.style.position_fixed);
        assert_eq!(el.style.position, taffy::style::Position::Absolute);

        let mut el = Element::new().text("x");
        apply_class_list(&mut el, "rtl");
        assert_eq!(el.text_style.direction, crate::style::TextDirection::Rtl);
    }
}
