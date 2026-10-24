//! # Badge
//!
//! ShadCN / solid-ui style logos: status labels, count capsules, lightweight labels.
//! 对齐 `Reference/ShadCN-UI/.../ui/badge.tsx`：`inline-flex w-fit shrink-0` +
//! `variant`（default / secondary / success / warning / destructive(=error) / outline / ghost / link）。
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags
//! <Badge>"New"</Badge>
//! <Badge variant="success">"Done"</Badge>
//! <Badge variant="warning">"Retry"</Badge>
//! <Badge variant="error">"Failed"</Badge> // ≡ destructive
//! <Badge variant="outline">
//!     <Icon name="check" size="0.75rem" />
//!     "Verified"
//! </Badge>
//! // 加载中：Icon 用 utility `animate-spin`（非 `src`）
//! <Badge variant="secondary">
//!     <Icon name="loader-circle" size="0.75rem" class="animate-spin" />
//!     "Loading"
//! </Badge>
//!
//! // B · Builder
//! Element::from(badge().variant("destructive").text("Error"))
//! Element::from(badge().variant("warn").text("Caution")) // warn ≡ warning
//! ```
//!
//! ## Notes
//! - **Display type**: User `id` is not required by default; do not write `template_id` in an empty string (WP-2.8).
//! - The structure is chrome container + copywriting/icon leaf (BorderBox), avoiding the ContentBox trap of single node pad+center.
//! - `ui!` 多子节点会包成 `VStack`；Badge 会拆开并横排，同时按变体重着色（否则白字 + 裁切空壳）。
//! - Dynamic `variant={sig.get()}` is reduced from macro to [`apply_bound_badge_variant`] (same origin as Button).
//! - The semantic color (success/warning/destructive) is the rest state of Button soft tint, non-solid filling.

use crate::components::base::SlotChrome;
use crate::core::tree::{Element, IntoF32, IntoNodeSize, LayoutStatus};
use crate::primitives::r#box::{hstack, BoxBuilder};
use crate::style::{
    apply_semantic_border, is_variant_style_overridden, parse_color, parse_text_color,
    semantic_chrome, set_variant_chrome_enabled, variant_chrome_enabled, SemanticBorder,
    SemanticChromeDecor, SemanticChromeOpts, SemanticSurface, SemanticTone, VariantStyleField,
};
use crate::utils::take_packed;
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use jadix_macros::component;

const ATTR_SLOT: &str = "data-slot";
const SLOT_BADGE: &str = "badge";
const ATTR_VARIANT: &str = "data-variant";
const ATTR_VARIANT_LEGACY: &str = "jadix_badge_variant";
/// Badge `variant`: ShadCN baseline + semantic color (success/warning) + `error`≡destructive.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum BadgeVariant {
    #[default]
    Default,
    Secondary,
    Success,
    Warning,
    Destructive,
    Outline,
    Ghost,
    Link,
}

impl From<&str> for BadgeVariant {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::Default)
    }
}

impl From<String> for BadgeVariant {
    fn from(s: String) -> Self {
        s.parse().unwrap_or(Self::Default)
    }
}

impl From<&String> for BadgeVariant {
    fn from(s: &String) -> Self {
        s.parse().unwrap_or(Self::Default)
    }
}

impl std::str::FromStr for BadgeVariant {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "secondary" => Self::Secondary,
            "outline" => Self::Outline,
            "ghost" => Self::Ghost,
            "link" => Self::Link,
            _ => match SemanticTone::parse(s) {
                SemanticTone::Success => Self::Success,
                SemanticTone::Warning => Self::Warning,
                SemanticTone::Destructive => Self::Destructive,
                _ => Self::Default,
            },
        })
    }
}

impl BadgeVariant {
    pub fn as_paint_key(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Secondary => "secondary",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Destructive => "destructive",
            Self::Outline => "outline",
            Self::Ghost => "ghost",
            Self::Link => "link",
        }
    }
}

fn badge_chrome(variant: &str) -> ([f32; 4], [u8; 4], Option<[f32; 4]>) {
    match variant {
        "secondary" => (
            parse_color("secondary"),
            parse_text_color("secondary-foreground"),
            None,
        ),
        "success" | "warning" | "destructive" => {
            let tone = SemanticTone::parse(variant);
            let c = semantic_chrome(SemanticChromeOpts {
                tone,
                filled: true,
                surface: SemanticSurface::Card,
                decor: SemanticChromeDecor::NONE,
            });
            (c.bg, c.fg, c.border.map(|b| b.color))
        }
        "outline" => (
            [0.0, 0.0, 0.0, 0.0],
            parse_text_color("foreground"),
            Some(parse_color("border")),
        ),
        "ghost" => ([0.0, 0.0, 0.0, 0.0], parse_text_color("foreground"), None),
        "link" => ([0.0, 0.0, 0.0, 0.0], parse_text_color("primary"), None),
        _ => (
            parse_color("primary"),
            parse_text_color("primary-foreground"),
            None,
        ),
    }
}

fn apply_chrome(el: &mut Element, variant: &str) {
    let (bg, text_color, border) = badge_chrome(variant);
    if variant_chrome_enabled(&el.attrs) {
        if !is_variant_style_overridden(&el.attrs, VariantStyleField::Bg) {
            el.style.bg_color = bg;
        }
        if !is_variant_style_overridden(&el.attrs, VariantStyleField::Border) {
            apply_semantic_border(el, border.map(|color| SemanticBorder { color, width: 1.0 }));
        }
        if !is_variant_style_overridden(&el.attrs, VariantStyleField::Fg) {
            paint_descendant_text(el, text_color, variant == "link");
        }
    }
    el.set_attr(ATTR_VARIANT, variant);
    el.set_attr(ATTR_VARIANT_LEGACY, variant);
}

/// 将 Badge 变体 chroming 应用到任意子树（[`AvatarBadge`] 圆点复用）。
pub fn apply_badge_variant_paint(el: &mut Element, variant: impl AsRef<str>) -> LayoutStatus {
    let key = BadgeVariant::from(variant.as_ref()).as_paint_key();
    let prev = el
        .attrs
        .get(ATTR_VARIANT)
        .map(|s| s.as_str())
        .unwrap_or("default");
    if prev == key {
        return LayoutStatus::Clean;
    }
    apply_chrome(el, key);
    LayoutStatus::StyleChanged
}

fn paint_descendant_text(el: &mut Element, color: [u8; 4], underline: bool) {
    crate::core::tree::walk_elements_mut(el, |el| {
        if el.text.is_some() {
            el.text_style.color = Some(color);
            el.text_style.underline = Some(underline);
        }
    });
}

pub struct BadgeBuilder {
    id: String,
    variant: BadgeVariant,
    chrome_enabled: bool,
    container: BoxBuilder,
    content: Vec<Element>,
    text_style: TextNodeConfig,
    slot_chrome: SlotChrome,
}

/// Builder 入口：展示型徽标，默认无用户 id。
pub fn badge() -> BadgeBuilder {
    BadgeBuilder {
        id: String::new(),
        variant: BadgeVariant::Default,
        chrome_enabled: true,
        // ShadCN base: inline-flex w-fit shrink-0 items-center justify-center gap-1
        // rounded-full border px-2 py-0.75 text-xs font-medium whitespace-nowrap
        container: hstack()
            .items_center()
            .justify_center()
            .gap("0.25rem")
            .px("0.5rem")
            .py("0.1875rem")
            .min_h("1.25rem")
            .rounded_full()
            .flex_shrink(0.0)
            .w("auto")
            .h("auto")
            .overflow("hidden"),
        content: Vec::new(),
        text_style: TextNodeConfig {
            size_px: IntoF32::into_f32("0.75rem"),
            weight: 500,
            color: [255, 255, 255, 255],
            underline: false,
            underline_offset_px: None,
            text_align: crate::style::TextAlign::Center,
            line_height_px: None,
            leading_scale: None,
            font_family: None,
        },
        slot_chrome: SlotChrome::default(),
    }
}

/// RSX `<Badge variant="…" />`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Badge(
    id: String,
    variant: BadgeVariant,
    chrome: Option<bool>,
    children: Option<Element>,
) -> Element {
    let mut builder = badge().variant(variant);
    if let Some(enabled) = chrome {
        builder = builder.chrome(enabled);
    }
    if !id.trim().is_empty() {
        builder = builder.id(id);
    }
    if let Some(c) = children {
        for kid in take_packed(c) {
            builder = builder.child(kid);
        }
    }
    builder.into()
}

impl BadgeBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn variant(mut self, v: impl Into<BadgeVariant>) -> Self {
        self.variant = v.into();
        self
    }

    pub fn chrome(mut self, enabled: bool) -> Self {
        self.chrome_enabled = enabled;
        self
    }

    pub fn class(mut self, classes: impl Into<String>) -> Self {
        self.slot_chrome.class = classes.into();
        self
    }

    pub fn style(
        mut self,
        f: impl Fn(
                crate::components::base::BaseComponentBuilder,
            ) -> crate::components::base::BaseComponentBuilder
            + 'static,
    ) -> Self {
        self.slot_chrome = self.slot_chrome.style(f);
        self
    }

    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.content.push(Element::new().text(content.into()));
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.content.extend(take_packed(child.into()));
        self
    }

    pub fn children(mut self, kids: impl IntoIterator<Item = Element>) -> Self {
        for c in kids {
            self = self.child(c);
        }
        self
    }

    #[track_caller]
    pub fn w(mut self, w: impl IntoNodeSize) -> Self {
        self.container = self.container.w(w);
        self
    }
}

impl crate::core::tree::IntoElementList for BadgeBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<BadgeBuilder> for Element {
    fn from(builder: BadgeBuilder) -> Self {
        let variant = builder.variant.as_paint_key();
        let (bg, text_color, border) = badge_chrome(variant);

        let mut container = builder.container;
        if builder.chrome_enabled {
            container = container.bg(bg);
            if let Some(b) = border {
                container = container.border(b, 1.0);
            }
        }

        let mut el: Element = container.into();
        set_variant_chrome_enabled(&mut el, builder.chrome_enabled);
        el.attrs
            .insert(ATTR_SLOT.to_string(), SLOT_BADGE.to_string());
        el.set_attr(ATTR_VARIANT, variant);
        el.set_attr(ATTR_VARIANT_LEGACY, variant);
        if !builder.id.trim().is_empty() {
            el.template_id = Some(builder.id.clone());
        }

        for mut child in builder.content {
            if let Some(text_content) = child.text.take() {
                let preserve_binding = child.template_id.is_some() || child.on_unmount.is_some();
                let is_lucide = child.text_style.font_family.as_deref() == Some("lucide");
                let size_px = if is_lucide {
                    // ShadCN [&>svg]:size-3 — keep author size when set, else 0.75rem
                    if child.text_style.size_resolved() > 1.0 {
                        child.text_style.size_resolved()
                    } else {
                        IntoF32::into_f32("0.75rem")
                    }
                } else {
                    builder.text_style.size_px
                };
                let caption_color = if builder.chrome_enabled {
                    text_color
                } else {
                    child.text_style.color_resolved()
                };
                let mut text_node = build_text_node(
                    text_content,
                    TextNodeConfig {
                        color: caption_color,
                        underline: builder.chrome_enabled && variant == "link",
                        underline_offset_px: if builder.chrome_enabled && variant == "link" {
                            Some(2.0)
                        } else {
                            None
                        },
                        font_family: child.text_style.font_family.clone(),
                        size_px,
                        weight: if is_lucide {
                            400
                        } else {
                            builder.text_style.weight
                        },
                        ..builder.text_style.clone()
                    },
                );
                if !is_lucide {
                    text_node = text_node.whitespace_nowrap();
                }
                if preserve_binding {
                    if let Some(tid) = child.template_id.take() {
                        text_node.template_id = Some(tid);
                    }
                    if let Some(hook) = child.on_unmount.take() {
                        text_node.on_unmount = Some(hook);
                    }
                }
                el = el.child(text_node);
            } else {
                // Non-text child (e.g. packed icon Element already built).
                if child.text_style.font_family.as_deref() == Some("lucide")
                    && child.text_style.size_resolved() <= 1.0
                {
                    child = child.text_size(IntoF32::into_f32("0.75rem"));
                }
                if child.text.is_some() {
                    if builder.chrome_enabled {
                        child.text_style.color = Some(text_color);
                    }
                }
                el = el.child(child);
            }
        }

        builder.slot_chrome.apply(el)
    }
}

/// Solid-style in-place bind for `<Badge variant={sig.get()}>`.
pub fn apply_bound_badge_variant(el: &mut Element, variant: impl AsRef<str>) -> LayoutStatus {
    apply_badge_variant_paint(el, variant)
}

/// Solid-style in-place bind for `<Badge chrome={sig.get()}>`.
pub fn apply_bound_badge_chrome(el: &mut Element, enabled: bool) -> LayoutStatus {
    let was = variant_chrome_enabled(&el.attrs);
    set_variant_chrome_enabled(el, enabled);
    if was == enabled {
        return LayoutStatus::Clean;
    }
    if enabled {
        let key = el
            .attrs
            .get(ATTR_VARIANT)
            .map(|s| s.clone())
            .unwrap_or_else(|| "default".to_string());
        apply_chrome(el, key.as_str());
    }
    LayoutStatus::StyleChanged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::layout_tree::LayoutTree;

    #[test]
    fn badge_variants_set_slot_and_paint_key() {
        for (name, _) in [
            ("default", BadgeVariant::Default),
            ("secondary", BadgeVariant::Secondary),
            ("success", BadgeVariant::Success),
            ("warning", BadgeVariant::Warning),
            ("destructive", BadgeVariant::Destructive),
            ("outline", BadgeVariant::Outline),
            ("ghost", BadgeVariant::Ghost),
            ("link", BadgeVariant::Link),
        ] {
            let el: Element = badge().variant(name).text(name).into();
            assert_eq!(
                el.attrs.get(ATTR_SLOT).map(|s| s.as_str()),
                Some(SLOT_BADGE)
            );
            assert_eq!(el.attrs.get(ATTR_VARIANT).map(|s| s.as_str()), Some(name));
            assert!(
                el.children.iter().any(|c| c.text.as_deref() == Some(name)),
                "caption leaf missing for {name}"
            );
        }
    }

    #[test]
    fn badge_warn_and_error_aliases_normalize() {
        let warn: Element = badge().variant("warn").text("w").into();
        assert_eq!(
            warn.attrs.get(ATTR_VARIANT).map(|s| s.as_str()),
            Some("warning")
        );
        let err: Element = badge().variant("error").text("e").into();
        assert_eq!(
            err.attrs.get(ATTR_VARIANT).map(|s| s.as_str()),
            Some("destructive")
        );
        assert_eq!(
            "warn".parse::<BadgeVariant>().unwrap(),
            BadgeVariant::Warning
        );
        assert_eq!(
            "error".parse::<BadgeVariant>().unwrap(),
            BadgeVariant::Destructive
        );
    }

    #[test]
    fn badge_empty_id_does_not_write_empty_template() {
        let el: Element = badge().text("x").into();
        assert!(el.template_id.is_none() || el.template_id.as_deref() != Some(""));
    }

    #[test]
    fn badge_default_height_has_vertical_breathing_room() {
        let mut el: Element = badge().text("Hi").into();
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut el, 400.0, 80.0, [-1.0, -1.0]);
        let h = el.layout_bounds[3];
        let text_h = el.children[0].layout_bounds[3];
        // text-xs default leading (~16px) + py (0.1875rem × 2) ≈ 22px; must exceed tight 1.0 leading.
        assert!(
            h >= 20.0,
            "badge layout height should be at least 20px, got {h:.2} (text leaf {text_h:.2})"
        );
        assert!(
            text_h >= 14.0,
            "badge text leaf should use Tailwind text-xs leading, got {text_h:.2}"
        );
    }

    #[test]
    fn fluent_badge_uses_compact_caption_metrics() {
        use crate::style::{set_theme_preset, ThemePreset};

        let previous = crate::style::get_theme();
        set_theme_preset(ThemePreset::Fluent);
        let el: Element = badge().text("Default").into();
        let caption = el
            .children
            .iter()
            .find(|child| child.text.is_some())
            .unwrap();
        assert_eq!(caption.text_style.size, Some(12.0));
        assert_eq!(caption.text_style.line_height_px, Some(16.0));
        crate::style::set_theme(previous);
    }

    #[test]
    fn badge_content_sized_shrinks_with_shorter_label() {
        use crate::primitives::r#box::hstack;
        let mut long: Element = hstack()
            .items_center()
            .child(badge().text("Blocked"))
            .into();
        let mut short: Element = hstack().items_center().child(badge().text("Ready")).into();
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut long, 400.0, 80.0, [-1.0, -1.0]);
        let _ = tree.build_and_layout(&mut short, 400.0, 80.0, [-1.0, -1.0]);
        let lw = long.children[0].layout_bounds[2];
        let sw = short.children[0].layout_bounds[2];
        assert!(
            lw > sw + 4.0,
            "Blocked ({lw:.1}) should be wider than Ready ({sw:.1})"
        );
    }

    #[test]
    fn badge_unpacks_macro_vstack_children_and_paints_variant_color() {
        use crate::primitives::r#box::vstack;
        use crate::utils::text_node::build_icon_text_node;

        // Simulate ui! multi-child pack: Icon + caption wrapped in a VStack.
        let packed = Element::from(vstack().children(vec![
            build_icon_text_node("✓").text_size(12.0),
            Element::new().text("Verified"),
        ]));
        let el: Element = Badge(BadgeProps {
            variant: Some(BadgeVariant::Secondary),
            children: Some(packed),
            ..Default::default()
        });
        assert_eq!(
            el.attrs.get(ATTR_VARIANT).map(|s| s.as_str()),
            Some("secondary")
        );
        assert_eq!(
            el.children.len(),
            2,
            "icon+caption must be hoisted into the badge row, not left as a nested VStack"
        );
        let want = parse_text_color("secondary-foreground");
        for (i, c) in el.children.iter().enumerate() {
            assert!(
                c.text.as_ref().is_some_and(|t| !t.is_empty()),
                "child {i} must be a text/icon leaf"
            );
            assert_eq!(
                c.text_style.color_resolved(),
                want,
                "child {i} must use secondary-foreground, not default white"
            );
        }
        assert!(
            el.children
                .iter()
                .any(|c| c.text.as_deref() == Some("Verified")),
            "caption must survive unpack"
        );
    }

    #[test]
    fn apply_bound_badge_variant_updates_chrome() {
        let mut el: Element = badge().variant("default").text("Hi").into();
        let status = apply_bound_badge_variant(&mut el, "outline");
        assert_ne!(status, LayoutStatus::Clean);
        assert_eq!(
            el.attrs.get(ATTR_VARIANT).map(|s| s.as_str()),
            Some("outline")
        );
        assert!(el.style.border_width > 0.5);
        let again = apply_bound_badge_variant(&mut el, "outline");
        assert_eq!(again, LayoutStatus::Clean);
    }

    #[test]
    fn semantic_badge_soft_tint_matches_button_rest() {
        use crate::style::{set_theme_mode, ThemeMode};
        set_theme_mode(ThemeMode::Light);
        for (name, accent_token, text_token) in [
            ("success", "success", "success"),
            ("warning", "warning", "warning"),
            ("destructive", "destructive", "destructive"),
        ] {
            let el: Element = badge().variant(name).text(name).into();
            let accent = parse_color(accent_token);
            let bg = parse_color("background");
            let want_bg = [
                accent[0] * 0.1 + bg[0] * 0.9,
                accent[1] * 0.1 + bg[1] * 0.9,
                accent[2] * 0.1 + bg[2] * 0.9,
                1.0,
            ];
            assert!(
                (0..4).all(|i| (el.style.bg_color[i] - want_bg[i]).abs() < 1e-4),
                "{name} bg should match button soft rest"
            );
            let want_fg = parse_text_color(text_token);
            let leaf = el
                .children
                .iter()
                .find(|c| c.text.as_deref() == Some(name))
                .expect("caption");
            assert_eq!(leaf.text_style.color_resolved(), want_fg);
        }
    }

    #[test]
    fn builder_style() {
        let el: Element = badge().style(|b| b.text_size(20.0)).text("New").into();
        fn find_size(el: &Element, want: f32) -> bool {
            el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
                || (el.text_style.size_resolved() - want).abs() < 0.01
                || el.children.iter().any(|c| find_size(c, want))
        }
        assert!(
            find_size(&el, 20.0),
            "badge .style(|b| b.text_size(20)) must land on the badge"
        );
    }
}
