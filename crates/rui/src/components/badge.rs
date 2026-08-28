//! # Badge
//!
//! ShadCN / solid-ui 风格徽标：状态标签、计数胶囊、轻量标签。
//! 对齐 `Reference/ShadCN-UI/.../ui/badge.tsx`：`inline-flex w-fit shrink-0` +
//! `variant`（default / secondary / success / warning / destructive(=error) / outline / ghost / link）。
//!
//! ## 用法（双写法 · ADR-9）
//! ```ignore
//! // A · RSX / 标签
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
//! ## 注意
//! - **展示型**：默认不要求用户 `id`；空串不写 `template_id`（WP-2.8）。
//! - 结构为 chrome 容器 + 文案/图标叶（BorderBox），避免单节点 pad+center 的 ContentBox 陷阱。
//! - `ui!` 多子节点会包成 `VStack`；Badge 会拆开并横排，同时按变体重着色（否则白字 + 裁切空壳）。
//! - 动态 `variant={sig.get()}` 由宏降为 [`apply_bound_badge_variant`]（与 Button 同源）。
//! - 语义色（success / warning / destructive）为 Button soft tint 的 rest 态，非实心填充。

use crate::core::tree::{
    parse_color, parse_text_color, Element, IntoCssDim, IntoF32, LayoutStatus,
};
use crate::primitives::r#box::{hstack, BoxBuilder};
use crate::style::is_dark_theme;
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use rui_macros::component;

const ATTR_SLOT: &str = "data-slot";
const SLOT_BADGE: &str = "badge";
const ATTR_VARIANT: &str = "rui_badge_variant";

/// `ui!` 把组件的多个子节点包成单根 `VStack`（或 `rui_fragment`）。徽标要横排图标+文案，
/// 必须拆开；否则会竖排进 `overflow:hidden` 被裁成空壳，且叶节点仍是默认白字。
fn take_packed(mut el: Element) -> Vec<Element> {
    if el.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1") {
        return std::mem::take(&mut el.children);
    }
    if el.text.as_ref().is_none_or(|t| t.is_empty()) && el.children.len() > 1 {
        return std::mem::take(&mut el.children);
    }
    vec![el]
}

/// Badge `variant`：ShadCN 基线 + 语义色（success / warning）+ `error`≡destructive。
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

impl std::str::FromStr for BadgeVariant {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "secondary" => Self::Secondary,
            "success" => Self::Success,
            "warning" | "warn" => Self::Warning,
            "destructive" | "error" => Self::Destructive,
            "outline" => Self::Outline,
            "ghost" => Self::Ghost,
            "link" => Self::Link,
            _ => Self::Default,
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
    let dark = is_dark_theme();
    // Soft tint 与 Button rest chrome 同源（浅 10% / 暗 20% 叠 background）。
    let soft = |accent: [f32; 4]| -> [f32; 4] {
        let a = if dark { 0.2 } else { 0.1 };
        let bg = parse_color("background");
        [
            accent[0] * a + bg[0] * (1.0 - a),
            accent[1] * a + bg[1] * (1.0 - a),
            accent[2] * a + bg[2] * (1.0 - a),
            1.0,
        ]
    };
    match variant {
        "secondary" => (
            parse_color("secondary"),
            parse_text_color("secondary-foreground"),
            None,
        ),
        "success" => (
            soft(parse_color("green-600")),
            parse_text_color("green-600"),
            None,
        ),
        "warning" => (
            soft(parse_color("orange-600")),
            parse_text_color("orange-600"),
            None,
        ),
        "destructive" => (
            soft(parse_color("destructive")),
            parse_text_color("destructive"),
            None,
        ),
        "outline" => (
            [0.0, 0.0, 0.0, 0.0],
            parse_text_color("foreground"),
            Some(parse_color("border")),
        ),
        "ghost" => (
            [0.0, 0.0, 0.0, 0.0],
            parse_text_color("foreground"),
            None,
        ),
        "link" => (
            [0.0, 0.0, 0.0, 0.0],
            parse_text_color("primary"),
            None,
        ),
        _ => (
            parse_color("primary"),
            parse_text_color("primary-foreground"),
            None,
        ),
    }
}

fn apply_chrome(el: &mut Element, variant: &str) {
    let (bg, text_color, border) = badge_chrome(variant);
    el.style.bg_color = bg;
    match border {
        Some(b) => {
            el.style.border_color = b;
            el.style.border_width = 1.0;
        }
        None => {
            el.style.border_width = 0.0;
            el.style.border_color = [0.0, 0.0, 0.0, 0.0];
        }
    }
    el.set_attr(ATTR_VARIANT, variant);
    paint_descendant_text(el, text_color, variant == "link");
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
    if el.text.is_some() {
        el.text_style.color = color;
        el.text_style.underline = underline;
    }
    for child in &mut el.children {
        paint_descendant_text(child, color, underline);
    }
}

pub struct BadgeBuilder {
    id: String,
    variant: BadgeVariant,
    container: BoxBuilder,
    content: Vec<Element>,
    text_style: TextNodeConfig,
}

/// Builder 入口：展示型徽标，默认无用户 id。
pub fn badge() -> BadgeBuilder {
    BadgeBuilder {
        id: String::new(),
        variant: BadgeVariant::Default,
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
    }
}

/// RSX `<Badge variant="…" />`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Badge(id: String, variant: BadgeVariant, children: Option<Element>) -> Element {
    let mut builder = badge().variant(variant.as_paint_key());
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

    pub fn variant(mut self, v: impl AsRef<str>) -> Self {
        self.variant = v.as_ref().parse().unwrap_or(BadgeVariant::Default);
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

    pub fn w(mut self, w: impl IntoCssDim) -> Self {
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

        let mut container = builder.container.bg(bg);
        if let Some(b) = border {
            container = container.border(b, 1.0);
        }

        let mut el: Element = container.into();
        el.attrs
            .insert(ATTR_SLOT.to_string(), SLOT_BADGE.to_string());
        el.set_attr(ATTR_VARIANT, variant);
        if !builder.id.trim().is_empty() {
            el.template_id = Some(builder.id.clone());
        }

        for mut child in builder.content {
            if let Some(text_content) = child.text.take() {
                let preserve_binding = child.template_id.is_some() || child.on_unmount.is_some();
                let is_lucide = child.text_style.font_family.as_deref() == Some("lucide");
                let size_px = if is_lucide {
                    // ShadCN [&>svg]:size-3 — keep author size when set, else 0.75rem
                    if child.text_style.size > 1.0 {
                        child.text_style.size
                    } else {
                        IntoF32::into_f32("0.75rem")
                    }
                } else {
                    builder.text_style.size_px
                };
                let mut text_node = build_text_node(
                    text_content,
                    TextNodeConfig {
                        color: text_color,
                        underline: variant == "link",
                        underline_offset_px: if variant == "link" {
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
                    && child.text_style.size <= 1.0
                {
                    child = child.text_size(IntoF32::into_f32("0.75rem"));
                }
                if child.text.is_some() {
                    child.text_style.color = text_color;
                }
                el = el.child(child);
            }
        }

        el
    }
}

/// Solid-style in-place bind for `<Badge variant={sig.get()}>`.
pub fn apply_bound_badge_variant(el: &mut Element, variant: impl AsRef<str>) -> LayoutStatus {
    apply_badge_variant_paint(el, variant)
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
    fn badge_content_sized_shrinks_with_shorter_label() {
        use crate::primitives::r#box::hstack;
        let mut long: Element = hstack()
            .items_center()
            .child(badge().text("Blocked"))
            .into();
        let mut short: Element = hstack()
            .items_center()
            .child(badge().text("Ready"))
            .into();
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
        let packed = Element::from(
            vstack().children(vec![
                build_icon_text_node("✓").text_size(12.0),
                Element::new().text("Verified"),
            ]),
        );
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
                c.text_style.color, want,
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
            ("success", "green-600", "green-600"),
            ("warning", "orange-600", "orange-600"),
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
            assert_eq!(leaf.text_style.color, want_fg);
        }
    }
}
