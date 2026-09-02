//! # Alert
//!
//! ShadCN-style inline alert callout. Aligns with
//! `Reference/ShadCN-UI/.../bases/base/ui/alert.tsx` (`role="alert"`, compound slots).
//! Semantic variants (`info` / `success` / `warning` / `error`) follow
//! `Reference/gpui-component-main/crates/ui/src/alert.rs` tinted chrome.
//!
//! ```text
//! Alert                         // bordered card · variant chrome · role=alert
//! ├── Icon?                     // optional leading lucide (auto icon column)
//! ├── AlertTitle
//! ├── AlertDescription
//! └── AlertAction?              // trailing actions (dismiss / CTA)
//! ```
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags
//! <Alert variant="default">
//!     <Icon name="info" size="1rem" />
//!     <AlertTitle>"Heads up!"</AlertTitle>
//!     <AlertDescription>"You can add components using the CLI."</AlertDescription>
//! </Alert>
//! <Alert variant="destructive">
//!     <Icon name="triangle-alert" size="1rem" />
//!     <AlertTitle>"Error"</AlertTitle>
//!     <AlertDescription>"Your session has expired."</AlertDescription>
//!     <AlertAction>
//!         <Button variant="outline" size="sm">"Sign in"</Button>
//!     </AlertAction>
//! </Alert>
//!
//! // B · Builder
//! Element::from(
//!     alert()
//!         .variant("warning")
//!         .icon(build_icon_text_node(lucide_glyph("triangle-alert").unwrap()))
//!         .title("Warning")
//!         .description("This action requires confirmation.")
//! )
//! alert().variant("success").title("Saved").description("Changes were stored.")
//! ```
//!
//! ## Notes
//! - **Presentational**: empty `id` omits `template_id` (WP-2.8).
//! - `<Alert>` uses generic PascalCase �?`AlertProps`; dynamic `variant={sig.get()}` binds via
//!   [`apply_bound_alert_variant`] (same pattern as Badge / Bubble).
//! - `error` �?`destructive`. `info` / `success` / `warning` via [`SemanticTone`].
//! - 语义配色�?[`crate::style::semantic_chrome`]；默�?**不填�?*背景（主�?`card` + 语义边框/字色，同 Toast）�?
//! - `.filled(true)` / `filled={true}` 启用 Badge �?soft tint 底�?
//! - 展示型提示条：无受控开关；`role="alert"` + `accesskit_role="alert"` 供读屏播报�?

use crate::components::base::BaseComponentBuilder;
use crate::core::tree::{Element, IntoF32, LayoutStatus};
use crate::primitives::r#box::{hstack, vstack};
use crate::style::{
    apply_semantic_chrome_respecting_overrides, is_variant_style_overridden, reapply_class_color_overrides,
    semantic_chrome, set_variant_chrome_enabled, SemanticChrome, SemanticChromeDecor, SemanticChromeOpts,
    SemanticSurface, SemanticTone, variant_chrome_enabled, VariantStyleField, with_class_list,
};
use crate::tokens;
use crate::utils::take_packed;
use crate::utils::text_node::{TextNodeConfig, build_text_node};
use rui_macros::component;

const ATTR_SLOT: &str = "data-slot";
const ATTR_VARIANT: &str = "data-variant";
const ATTR_ALERT_VARIANT: &str = "rui_alert_variant";
const ATTR_SEMANTIC_FILLED: &str = "rui_semantic_filled";
const SLOT_ROOT: &str = "alert";
const SLOT_TITLE: &str = "alert-title";
const SLOT_DESCRIPTION: &str = "alert-description";
const SLOT_ACTION: &str = "alert-action";

const TITLE_SIZE_PX: f32 = 14.0;
const TITLE_WEIGHT: u16 = 600;
const DESC_SIZE_PX: f32 = 14.0;
const DESC_WEIGHT: u16 = 400;

/// Alert visual tone: ShadCN `default` / `destructive` + GPUI semantic colors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlertVariant {
    #[default]
    Default,
    Destructive,
    Info,
    Success,
    Warning,
}

impl AlertVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Destructive => "destructive",
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
        }
    }
}

impl From<&str> for AlertVariant {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::Default)
    }
}

impl From<String> for AlertVariant {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::str::FromStr for AlertVariant {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match SemanticTone::parse(s) {
            SemanticTone::Default => Self::Default,
            SemanticTone::Info => Self::Info,
            SemanticTone::Success => Self::Success,
            SemanticTone::Warning => Self::Warning,
            SemanticTone::Destructive => Self::Destructive,
        })
    }
}

impl AlertVariant {
    pub fn to_semantic(self) -> SemanticTone {
        match self {
            Self::Default => SemanticTone::Default,
            Self::Destructive => SemanticTone::Destructive,
            Self::Info => SemanticTone::Info,
            Self::Success => SemanticTone::Success,
            Self::Warning => SemanticTone::Warning,
        }
    }
}

fn alert_chrome(variant: AlertVariant, filled: bool) -> SemanticChrome {
    semantic_chrome(SemanticChromeOpts {
        tone: variant.to_semantic(),
        filled,
        surface: SemanticSurface::Card,
        decor: SemanticChromeDecor::BORDERED,
    })
}

fn slot_of(el: &Element) -> Option<&str> {
    el.attrs.get(ATTR_SLOT).map(String::as_str)
}

fn is_action(el: &Element) -> bool {
    slot_of(el) == Some(SLOT_ACTION)
}

fn is_title(el: &Element) -> bool {
    slot_of(el) == Some(SLOT_TITLE)
}

fn is_description(el: &Element) -> bool {
    slot_of(el) == Some(SLOT_DESCRIPTION)
}

fn is_icon_child(el: &Element) -> bool {
    if slot_of(el).is_some() {
        return false;
    }
    el.text_style.font_family.as_deref() == Some("lucide")
}

fn paint_icon_child(mut el: Element, fg: [u8; 4]) -> Element {
    el = el.pointer_events_none().flex_shrink(0.0);
    if el.text_style.font_family.as_deref() == Some("lucide")
        && el.text_style.size_resolved() <= 1.0
    {
        el = el.text_size(IntoF32::into_f32("1rem"));
    }
    if el.text.is_some() {
        el = el.text_color(fg);
    }
    el
}

/// Stamp variant-painted text (explicit `color` survives layout cascade).
fn stamp_alert_text_paint(el: &mut Element, color: [u8; 4], size_px: f32, weight: u16) {
    el.text_style.color = Some(color);
    el.text_style.size = Some(size_px);
    el.text_style.weight = Some(weight);
}

fn paint_title_node_on_place(el: &mut Element, fg: [u8; 4]) {
    el.text_style.color = Some(fg);
    if el.text.is_some() {
        stamp_alert_text_paint(el, fg, TITLE_SIZE_PX, TITLE_WEIGHT);
    }
    for child in &mut el.children {
        paint_title_node_on_place(child, fg);
    }
}

fn paint_description_node_on_place(el: &mut Element, fg: [u8; 4]) {
    el.text_style.color = Some(fg);
    if el.text.is_some() {
        stamp_alert_text_paint(el, fg, DESC_SIZE_PX, DESC_WEIGHT);
    }
    for child in &mut el.children {
        paint_description_node_on_place(child, fg);
    }
}

fn paint_title_node(mut el: Element, fg: [u8; 4]) -> Element {
    if let Some(text) = el.text.take() {
        return build_text_node(
            text,
            TextNodeConfig {
                size_px: TITLE_SIZE_PX,
                weight: TITLE_WEIGHT,
                color: fg,
                ..Default::default()
            },
        );
    }
    paint_title_node_on_place(&mut el, fg);
    el
}

fn paint_description_node(mut el: Element, fg: [u8; 4]) -> Element {
    if let Some(text) = el.text.take() {
        return build_text_node(
            text,
            TextNodeConfig {
                size_px: DESC_SIZE_PX,
                weight: DESC_WEIGHT,
                color: fg,
                ..Default::default()
            },
        );
    }
    paint_description_node_on_place(&mut el, fg);
    el
}

fn paint_alert_child(el: Element, chrome: &SemanticChrome) -> Element {
    if is_title(&el) {
        if is_variant_style_overridden(&el.attrs, VariantStyleField::Fg) {
            el
        } else {
            paint_title_node(el, chrome.fg)
        }
    } else if is_description(&el) {
        if is_variant_style_overridden(&el.attrs, VariantStyleField::Fg) {
            el
        } else {
            paint_description_node(el, chrome.muted_fg)
        }
    } else if is_icon_child(&el) {
        if is_variant_style_overridden(&el.attrs, VariantStyleField::Fg) {
            el
        } else {
            paint_icon_child(el, chrome.fg)
        }
    } else {
        el
    }
}

fn assemble_alert_body(chrome: &SemanticChrome, kids: Vec<Element>) -> Element {
    let (actions, rest): (Vec<_>, Vec<_>) = kids.into_iter().partition(is_action);
    let (icons, body): (Vec<_>, Vec<_>) = rest.into_iter().partition(is_icon_child);

    let icons: Vec<Element> = icons
        .into_iter()
        .map(|c| paint_alert_child(c, chrome))
        .collect();
    let body: Vec<Element> = body
        .into_iter()
        .map(|c| paint_alert_child(c, chrome))
        .collect();

    let text_col: Element = vstack()
        .gap("0.125rem")
        .flex_grow(1.0)
        .min_w(0.0)
        .w("100%")
        .children(body)
        .into();

    let main: Element = if icons.is_empty() {
        text_col
    } else {
        hstack()
            .items("start")
            .gap("0.75rem")
            .flex_grow(1.0)
            .min_w(0.0)
            .w("100%")
            .children(icons)
            .child(text_col)
            .into()
    };

    if actions.is_empty() {
        main
    } else {
        hstack()
            .w("100%")
            .items("start")
            .justify("between")
            .gap("0.75rem")
            .child(main)
            .children(actions)
            .into()
    }
}

fn build_alert_root(variant: AlertVariant, filled: bool, chrome: bool, kids: Vec<Element>) -> Element {
    let chrome_paint = alert_chrome(variant, filled);
    let body_chrome = if chrome {
        chrome_paint
    } else {
        semantic_chrome(SemanticChromeOpts {
            tone: SemanticTone::Default,
            filled: false,
            surface: SemanticSurface::Card,
            decor: SemanticChromeDecor::NONE,
        })
    };
    let body = assemble_alert_body(&body_chrome, kids);
    let mut shell = vstack()
        .w("100%")
        .min_w(0.0)
        .relative(true)
        .p(tokens::SPACE_4)
        .rounded(tokens::RADIUS_LG);
    if chrome {
        shell = shell.bg(chrome_paint.bg);
        if let Some(b) = chrome_paint.border {
            shell = shell
                .border_style("solid")
                .border_width(b.width)
                .border_color(b.color);
        }
    }
    let mut el: Element = shell.child(body).into();
    el = el.text_size(DESC_SIZE_PX);
    el.set_attr(ATTR_SLOT, SLOT_ROOT);
    el.set_attr(ATTR_VARIANT, variant.as_str());
    el.set_attr(ATTR_ALERT_VARIANT, variant.as_str());
    el.set_attr(
        ATTR_SEMANTIC_FILLED,
        if filled { "1" } else { "0" },
    );
    el = el.accesskit_role("alert");
    set_variant_chrome_enabled(&mut el, chrome);
    el
}

fn alert_filled_from_attrs(attrs: &std::collections::HashMap<String, String>) -> bool {
    attrs
        .get(ATTR_SEMANTIC_FILLED)
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn repaint_alert_descendants(el: &mut Element, chrome: &SemanticChrome) {
    if is_title(el) {
        if !is_variant_style_overridden(&el.attrs, VariantStyleField::Fg) {
            paint_title_node_on_place(el, chrome.fg);
        }
    } else if is_description(el) {
        if !is_variant_style_overridden(&el.attrs, VariantStyleField::Fg) {
            paint_description_node_on_place(el, chrome.muted_fg);
        }
    } else if is_icon_child(el) {
        if !is_variant_style_overridden(&el.attrs, VariantStyleField::Fg) {
            paint_icon_node_on_place(el, chrome.fg);
        }
    }
    for child in &mut el.children {
        repaint_alert_descendants(child, chrome);
    }
}

fn paint_icon_node_on_place(el: &mut Element, fg: [u8; 4]) {
    if el.text.is_some() {
        el.text_style.color = Some(fg);
    }
    for child in &mut el.children {
        if child.text.is_some() {
            child.text_style.color = Some(fg);
        }
    }
}

fn sync_alert_variant_chrome(el: &mut Element, variant: AlertVariant) {
    if !variant_chrome_enabled(&el.attrs) {
        return;
    }
    let filled = alert_filled_from_attrs(&el.attrs);
    let chrome = alert_chrome(variant, filled);
    apply_semantic_chrome_respecting_overrides(el, &chrome);
    repaint_alert_descendants(el, &chrome);
    let _ = reapply_class_color_overrides(el);
}

/// Apply variant chroming to an alert root and repaint titled/desc/icon descendants.
pub fn apply_alert_variant_paint(el: &mut Element, variant: impl AsRef<str>) -> LayoutStatus {
    let variant = AlertVariant::from(variant.as_ref());
    let prev = el
        .attrs
        .get(ATTR_ALERT_VARIANT)
        .map(|s| s.as_str())
        .unwrap_or("default");
    if prev == variant.as_str() {
        return LayoutStatus::Clean;
    }
    el.set_attr(ATTR_VARIANT, variant.as_str());
    el.set_attr(ATTR_ALERT_VARIANT, variant.as_str());
    sync_alert_variant_chrome(el, variant);
    LayoutStatus::StyleChanged
}

/// Solid-style in-place bind for `<Alert chrome={sig.get()}>`.
pub fn apply_bound_alert_chrome(el: &mut Element, enabled: bool) -> LayoutStatus {
    let was = variant_chrome_enabled(&el.attrs);
    set_variant_chrome_enabled(el, enabled);
    if was == enabled {
        return LayoutStatus::Clean;
    }
    if enabled {
        let variant = el
            .attrs
            .get(ATTR_ALERT_VARIANT)
            .or_else(|| el.attrs.get(ATTR_VARIANT))
            .map(|s| s.as_str())
            .unwrap_or("default");
        sync_alert_variant_chrome(el, AlertVariant::from(variant));
    }
    LayoutStatus::StyleChanged
}

/// Solid-style in-place bind for `<Alert variant={sig.get()}>`.
pub fn apply_bound_alert_variant(el: &mut Element, variant: impl AsRef<str>) -> LayoutStatus {
    apply_alert_variant_paint(el, variant)
}

// ---------------------------------------------------------------------------
// RSX parts
// ---------------------------------------------------------------------------

/// Root alert container (`role="alert"`).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Alert(
    id: String,
    variant: AlertVariant,
    filled: Option<bool>,
    chrome: Option<bool>,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let filled = filled.unwrap_or(false);
    let chrome = chrome.unwrap_or(true);
    let mut el = build_alert_root(variant, filled, chrome, kids);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    if !id.trim().is_empty() {
        el.template_id = Some(id);
    }
    el
}

/// Alert heading line (`font-semibold` · variant foreground).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AlertTitle(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = vstack()
        .w("100%")
        .min_w(0.0)
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_TITLE);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Supporting copy (`text-sm` · semantic description tone, same as Toast).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AlertDescription(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = vstack()
        .w("100%")
        .min_w(0.0)
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_DESCRIPTION);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Trailing action slot (dismiss / CTA).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AlertAction(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = hstack()
        .items_center()
        .gap(tokens::SPACE_2)
        .flex_shrink(0.0)
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_ACTION);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

// ---------------------------------------------------------------------------
// Builder (ADR-9 surface B)
// ---------------------------------------------------------------------------

pub struct AlertBuilder {
    id: String,
    variant: AlertVariant,
    filled: bool,
    class: String,
    children: Vec<Element>,
}

/// Builder entry for [`Alert`].
pub fn alert() -> AlertBuilder {
    AlertBuilder {
        id: String::new(),
        variant: AlertVariant::Default,
        filled: false,
        class: String::new(),
        children: Vec::new(),
    }
}

impl AlertBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn variant(mut self, variant: impl Into<AlertVariant>) -> Self {
        self.variant = variant.into();
        self
    }

    /// 启用 Badge �?soft tint 语义底；默认 `false`（主�?`card` + 语义边框/字色）�?
    pub fn filled(mut self, filled: bool) -> Self {
        self.filled = filled;
        self
    }

    pub fn class(mut self, classes: impl Into<String>) -> Self {
        self.class = classes.into();
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }

    pub fn icon(mut self, child: impl Into<Element>) -> Self {
        let mut kids = vec![child.into()];
        kids.extend(self.children);
        self.children = kids;
        self
    }

    pub fn title(mut self, text: impl Into<String>) -> Self {
        self.children.push(
            AlertTitle(AlertTitleProps {
                children: Some(Element::new().text(text.into())),
                base: None,
                base_builder: None,
                ..Default::default()
            }),
        );
        self
    }

    pub fn description(mut self, text: impl Into<String>) -> Self {
        self.children.push(
            AlertDescription(AlertDescriptionProps {
                children: Some(Element::new().text(text.into())),
                base: None,
                base_builder: None,
                ..Default::default()
            }),
        );
        self
    }

    pub fn action(mut self, child: impl Into<Element>) -> Self {
        self.children.push(
            AlertAction(AlertActionProps {
                children: Some(child.into()),
                base: None,
                base_builder: None,
                ..Default::default()
            }),
        );
        self
    }
}

impl crate::core::tree::IntoElementList for AlertBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<AlertBuilder> for Element {
    fn from(builder: AlertBuilder) -> Self {
        let children = if builder.children.is_empty() {
            None
        } else {
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            Some(frag.children(builder.children))
        };
        let mut el = Alert(AlertProps {
            id: builder.id,
            variant: Some(builder.variant),
            filled: Some(builder.filled),
            children,
            base: None,
            base_builder: None,
            ..Default::default()
        });
        if !builder.class.trim().is_empty() {
            el = with_class_list(el, builder.class.trim());
        }
        el
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::parse_color;

    #[test]
    fn alert_root_slot_role_and_variant() {
        let el = Alert(AlertProps {
            id: String::new(),
            variant: Some(AlertVariant::Warning),
            children: None,
            base: None,
            base_builder: None,
            ..Default::default()
        });
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
        assert_eq!(
            el.attrs.get(ATTR_VARIANT).map(String::as_str),
            Some("warning")
        );
        assert_eq!(
            el.attrs.get("accesskit_role").map(String::as_str),
            Some("alert")
        );
    }

    #[test]
    fn alert_destructive_unfilled_uses_card_surface() {
        let el = build_alert_root(AlertVariant::Destructive, false, true, vec![]);
        assert_eq!(el.style.bg_color, parse_color("card"));
        let border = el.style.border_color;
        assert_eq!(border, parse_color("red-300"));
        assert_eq!(
            el.attrs.get(ATTR_ALERT_VARIANT).map(String::as_str),
            Some("destructive")
        );
    }

    #[test]
    fn alert_info_filled_soft_tint() {
        let el = build_alert_root(AlertVariant::Info, true, true, vec![]);
        assert_ne!(el.style.bg_color, parse_color("card"));
    }

    #[test]
    fn alert_info_unfilled_uses_semantic_border() {
        let el = build_alert_root(AlertVariant::Info, false, true, vec![]);
        assert_eq!(el.style.bg_color, parse_color("card"));
        assert_eq!(el.style.border_color, parse_color("blue-300"));
        assert!(el.style.border_width > 0.5);
    }

    #[test]
    fn alert_title_and_description_slots() {
        let title = AlertTitle(AlertTitleProps {
            children: Some(Element::new().text("T")),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        let desc = AlertDescription(AlertDescriptionProps {
            children: Some(Element::new().text("D")),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        assert_eq!(
            title.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_TITLE)
        );
        assert_eq!(
            desc.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_DESCRIPTION)
        );
    }

    #[test]
    fn apply_bound_alert_variant_updates_root() {
        let mut el = build_alert_root(AlertVariant::Default, false, true, vec![]);
        assert_eq!(
            apply_bound_alert_variant(&mut el, "success"),
            LayoutStatus::StyleChanged
        );
        assert_eq!(
            el.attrs.get(ATTR_ALERT_VARIANT).map(String::as_str),
            Some("success")
        );
        assert_eq!(
            apply_bound_alert_variant(&mut el, "success"),
            LayoutStatus::Clean
        );
    }

    #[test]
    fn apply_alert_variant_preserves_user_bg_override() {
        use crate::style::{mark_variant_style_override, parse_color, VariantStyleField};

        let mut el = build_alert_root(AlertVariant::Info, false, true, vec![]);
        let custom = parse_color("red-500");
        el.style.bg_color = custom;
        mark_variant_style_override(&mut el, VariantStyleField::Bg);
        apply_alert_variant_paint(&mut el, "warning");
        assert_eq!(el.style.bg_color, custom);
        assert_eq!(
            el.attrs.get(ATTR_ALERT_VARIANT).map(String::as_str),
            Some("warning")
        );
    }

    #[test]
    fn chrome_false_skips_repaint_on_variant_change() {
        let mut el = build_alert_root(AlertVariant::Info, false, false, vec![]);
        let bg = el.style.bg_color;
        let border = el.style.border_color;
        apply_alert_variant_paint(&mut el, "destructive");
        assert_eq!(el.style.bg_color, bg);
        assert_eq!(el.style.border_color, border);
        assert_eq!(
            el.attrs.get(ATTR_ALERT_VARIANT).map(String::as_str),
            Some("destructive")
        );
    }

    #[test]
    fn alert_title_slot_text_uses_variant_fg() {
        use crate::style::parse_text_color;

        let title = AlertTitle(AlertTitleProps {
            children: Some(Element::new().text("Error")),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        let el = build_alert_root(AlertVariant::Destructive, false, true, vec![title]);
        let want = alert_chrome(AlertVariant::Destructive, false).fg;
        let mut found = false;
        fn walk(el: &Element, want: [u8; 4], found: &mut bool) {
            if el.text.as_deref() == Some("Error") {
                assert_eq!(el.text_style.color_resolved(), want);
                *found = true;
            }
            for child in &el.children {
                walk(child, want, found);
            }
        }
        walk(&el, want, &mut found);
        assert!(found, "title text node missing");
        assert_eq!(want, parse_text_color("destructive"));
    }

    #[test]
    fn alert_title_text_survives_layout_cascade() {
        use crate::layout::layout_tree::cascade_inheritable_text_style_for_test;
        use crate::style::parse_text_color;

        let title = AlertTitle(AlertTitleProps {
            children: Some(Element::new().text("Heads up!")),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        let desc = AlertDescription(AlertDescriptionProps {
            children: Some(Element::new().text("Details")),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        let mut el = build_alert_root(AlertVariant::Info, false, true, vec![title, desc]);
        let title_fg = alert_chrome(AlertVariant::Info, false).fg;
        let desc_fg = alert_chrome(AlertVariant::Info, false).muted_fg;

        cascade_inheritable_text_style_for_test(&mut el);

        let mut title_ok = false;
        let mut desc_ok = false;
        fn walk(el: &Element, title_fg: [u8; 4], desc_fg: [u8; 4], title_ok: &mut bool, desc_ok: &mut bool) {
            match el.text.as_deref() {
                Some("Heads up!") => {
                    assert_eq!(el.text_style.color_resolved(), title_fg);
                    assert_ne!(el.text_style.color_resolved(), [255, 255, 255, 255]);
                    *title_ok = true;
                }
                Some("Details") => {
                    assert_eq!(el.text_style.color_resolved(), desc_fg);
                    *desc_ok = true;
                }
                _ => {}
            }
            for child in &el.children {
                walk(child, title_fg, desc_fg, title_ok, desc_ok);
            }
        }
        walk(&el, title_fg, desc_fg, &mut title_ok, &mut desc_ok);
        assert!(title_ok, "title text missing after cascade");
        assert!(desc_ok, "description text missing after cascade");
        assert_ne!(title_fg, parse_text_color("background"));
    }

    #[test]
    fn alert_title_fg_override_preserved_on_variant_change() {
        use crate::style::{mark_variant_style_override, parse_text_color, VariantStyleField};

        let custom = parse_text_color("blue-500");
        let mut title = AlertTitle(AlertTitleProps {
            children: Some(Element::new().text("Custom")),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        mark_variant_style_override(&mut title, VariantStyleField::Fg);
        fn paint_text(el: &mut Element, c: [u8; 4]) {
            if el.text.is_some() {
                el.text_style.color = Some(c);
                return;
            }
            for child in &mut el.children {
                paint_text(child, c);
            }
        }
        paint_text(&mut title, custom);

        let mut el = build_alert_root(AlertVariant::Info, false, true, vec![title]);
        apply_alert_variant_paint(&mut el, "destructive");

        let mut found = false;
        fn walk(el: &Element, want: [u8; 4], found: &mut bool) {
            if el.text.as_deref() == Some("Custom") {
                assert_eq!(el.text_style.color_resolved(), want);
                *found = true;
            }
            for child in &el.children {
                walk(child, want, found);
            }
        }
        walk(&el, custom, &mut found);
        assert!(found);
    }

    #[test]
    fn alert_class_bg_preserved_on_variant_change() {
        use crate::style::{apply_class_list, is_variant_style_overridden, parse_color, VariantStyleField};

        let mut el = build_alert_root(AlertVariant::Info, false, true, vec![]);
        let custom = parse_color("red-500");
        apply_class_list(&mut el, "bg-red-500");
        assert_eq!(el.style.bg_color, custom);
        assert!(is_variant_style_overridden(&el.attrs, VariantStyleField::Bg));
        apply_alert_variant_paint(&mut el, "warning");
        assert_eq!(el.style.bg_color, custom);
    }

    #[test]
    fn builder_assembles_icon_title_description() {
        let mut icon = Element::new().text("!");
        icon.text_style.font_family = Some("lucide".into());
        let el: Element = alert()
            .variant("info")
            .icon(icon)
            .title("Heads up")
            .description("Details here.")
            .into();
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
        let body = &el.children[0];
        assert!(body.children.len() >= 2);
    }

    #[test]
    fn dual_surface_compiles_shape() {
        let _b: Element = alert().variant("warning").title("X").into();
        let el = Alert(AlertProps {
            id: String::new(),
            variant: Some(AlertVariant::Default),
            children: None,
            base: None,
            base_builder: None,
            ..Default::default()
        });
        let _: Element = el;
    }
}
