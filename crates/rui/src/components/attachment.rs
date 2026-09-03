//! # Attachment
//!
//! ShadCN-style file / media attachment chip. Aligns with
//! `Reference/ShadCN-UI/.../bases/base/ui/attachment.tsx` — upload states, media slot,
//! trailing actions, invisible trigger overlay, and horizontal scroll group.
//!
//! ```text
//! AttachmentGroup                 // horizontal snap scroll · flex row
//! └── Attachment                  // state · size · orientation
//!     ├── AttachmentMedia?        // variant: icon | image
//!     ├── AttachmentContent
//!     │   ├── AttachmentTitle
//!     │   └── AttachmentDescription?
//!     ├── AttachmentActions?      // trailing icon buttons
//!     ├── AttachmentAction?       // ghost icon-xs Button shortcut
//!     └── AttachmentTrigger?      // absolute inset overlay (open / retry)
//! ```
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags
//! <AttachmentGroup>
//!     <Attachment state="done" size="default" orientation="horizontal">
//!         <AttachmentMedia variant="icon">
//!             <Icon name="file-text" size="1rem" />
//!         </AttachmentMedia>
//!         <AttachmentContent>
//!             <AttachmentTitle>"report.pdf"</AttachmentTitle>
//!             <AttachmentDescription>"2.4 MB"</AttachmentDescription>
//!         </AttachmentContent>
//!         <AttachmentActions>
//!             <AttachmentAction id="att-rm">
//!                 <Icon name="x" size="0.75rem" />
//!             </AttachmentAction>
//!         </AttachmentActions>
//!     </Attachment>
//! </AttachmentGroup>
//!
//! // Image preview (Message / composer): vertical card + variant=image
//! <Attachment orientation="vertical">
//!     <AttachmentMedia variant="image">
//!         <Image texture="photo" />
//!     </AttachmentMedia>
//! </Attachment>
//!
//! // B · Builder
//! Element::from(
//!     attachment_group()
//!         .child(
//!             attachment()
//!                 .state("error")
//!                 .size("sm")
//!                 .media_icon(icon_child)
//!                 .title("failed.zip")
//!                 .description("Upload failed")
//!                 .action(button("att-retry").variant("ghost").size("icon-xs").text("↻"))
//!         )
//! )
//! ```
//!
//! ## Notes
//! - **Presentational**: no scope / open state; `state` / `size` / `orientation` support macro `bind_attr`.
//! - `orientation=vertical` is `w-24` (`w-30` when `AttachmentContent` is present); media is `w-full` + `aspect-square` (ShadCN).
//! - Horizontal file chips hug content (`max-w-full` + `w-fit`); the middle title / description
//!   ellipsize. Icon and trailing actions do not shrink. Stretch columns still `align-self: start`
//!   (Message content pack) so chips do not fill the parent.
//! - `AttachmentMedia variant=image` fills a child [`crate::components::Image`] (`w-full` / `h-full`).
//! - `<Attachment>` compound tags use generic PascalCase → `*Props` (same as Item / Empty).
//! - `idle` uses dashed border; semantic states (`info` / `success` / `warning` / `destructive`) use
//!   [`semantic_chrome`] (same tone keys as Alert / Button); `uploading` / `processing` pulse title.
//! - `error` parses as `destructive`. Reactive `state={sig.get()}` repaints root + slot descendants.
//! - 展示型附件条：无受控开关；`AttachmentTrigger` 为铺满根节点的透明按钮层。

use std::sync::OnceLock;

use crate::components::base::BaseComponentBuilder;
use crate::components::button::{button, ButtonSize, ButtonVariant};
use crate::core::reactivity::{create_context, provide_context, use_context, Context};
use crate::core::tree::{parse_color, parse_text_color, Element, IntoF32, LayoutStatus};
use crate::primitives::r#box::{hstack, vstack};
use crate::style::{
    apply_semantic_chrome_respecting_overrides, is_variant_style_overridden, semantic_chrome,
    semantic_media_wash, with_class_list, SemanticChrome, SemanticChromeDecor, SemanticChromeOpts,
    SemanticSurface, SemanticTone, VariantStyleField,
};
use crate::tokens;
use crate::utils::take_packed;
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use rui_macros::component;

const ATTR_SLOT: &str = "data-slot";
const ATTR_STATE: &str = "data-state";
const ATTR_SIZE: &str = "data-size";
const ATTR_ORIENTATION: &str = "data-orientation";
const ATTR_VARIANT: &str = "data-variant";
const ATTR_ATTACHMENT_STATE: &str = "rui_attachment_state";
const ATTR_ATTACHMENT_SIZE: &str = "rui_attachment_size";
const ATTR_ATTACHMENT_ORIENTATION: &str = "rui_attachment_orientation";

const SLOT_GROUP: &str = "attachment-group";
const SLOT_ROOT: &str = "attachment";
const SLOT_MEDIA: &str = "attachment-media";
const SLOT_CONTENT: &str = "attachment-content";
const SLOT_TITLE: &str = "attachment-title";
const SLOT_DESCRIPTION: &str = "attachment-description";
const SLOT_ACTIONS: &str = "attachment-actions";
const SLOT_ACTION: &str = "attachment-action";
const SLOT_TRIGGER: &str = "attachment-trigger";

const TITLE_SIZE_PX: f32 = 13.0;
const TITLE_WEIGHT: u16 = 500;
const DESC_SIZE_PX: f32 = 11.0;
const DESC_WEIGHT: u16 = 400;
const ROOT_RADIUS: &str = "0.75rem";
const PULSE_CLASS: &str = "animate-pulse";
/// ShadCN `orientation=vertical` → `w-24`; `has-content` → `w-30`.
const VERTICAL_W: &str = "6rem";
const VERTICAL_W_CONTENT: &str = "7.5rem";

// ---------------------------------------------------------------------------
// Variants
// ---------------------------------------------------------------------------

/// Attachment tone / lifecycle: lifecycle (`idle` · `uploading` · `processing` · `done`) plus
/// semantic tones (`info` · `success` · `warning` · `destructive`; `error` ≡ `destructive`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AttachmentState {
    Idle,
    Uploading,
    Processing,
    Info,
    Success,
    Warning,
    Destructive,
    #[default]
    Done,
}

impl AttachmentState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Uploading => "uploading",
            Self::Processing => "processing",
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Destructive => "destructive",
            Self::Done => "done",
        }
    }

    fn title_shimmer(self) -> bool {
        matches!(self, Self::Uploading | Self::Processing)
    }

    fn uses_dashed_border(self) -> bool {
        self == Self::Idle
    }

    fn semantic_tone(self) -> Option<SemanticTone> {
        match self {
            Self::Info => Some(SemanticTone::Info),
            Self::Success => Some(SemanticTone::Success),
            Self::Warning => Some(SemanticTone::Warning),
            Self::Destructive => Some(SemanticTone::Destructive),
            _ => None,
        }
    }
}

impl From<&str> for AttachmentState {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::Done)
    }
}

impl From<String> for AttachmentState {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::str::FromStr for AttachmentState {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "idle" | "empty" | "pending" => Self::Idle,
            "uploading" | "upload" => Self::Uploading,
            "processing" | "process" => Self::Processing,
            "info" => Self::Info,
            "success" | "ok" => Self::Success,
            "warning" | "warn" => Self::Warning,
            "destructive" | "error" | "failed" | "fail" => Self::Destructive,
            _ => Self::Done,
        })
    }
}

/// Density: `default` | `sm` | `xs`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AttachmentSize {
    #[default]
    Default,
    Sm,
    Xs,
}

impl AttachmentSize {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Sm => "sm",
            Self::Xs => "xs",
        }
    }

    fn gap(self) -> &'static str {
        match self {
            Self::Default => "0.5rem",
            Self::Sm => "0.375rem",
            Self::Xs => "0.25rem",
        }
    }

    fn px(self) -> &'static str {
        match self {
            Self::Default => "0.625rem",
            Self::Sm => "0.5rem",
            Self::Xs => "0.375rem",
        }
    }

    fn py(self) -> &'static str {
        match self {
            Self::Default => "0.5rem",
            Self::Sm => "0.375rem",
            Self::Xs => "0.25rem",
        }
    }

    /// `has-media:p-2` / `p-1.5` / `p-1` when the card is media-only.
    fn media_pad(self) -> &'static str {
        match self {
            Self::Default => "0.5rem",
            Self::Sm => "0.375rem",
            Self::Xs => "0.25rem",
        }
    }

    /// Horizontal icon well: `w-10` / `w-8` / `w-7`.
    fn media_box(self) -> &'static str {
        match self {
            Self::Default => "2.5rem",
            Self::Sm => "2rem",
            Self::Xs => "1.75rem",
        }
    }

    fn root_radius(self) -> &'static str {
        match self {
            Self::Xs => "0.5rem",
            _ => ROOT_RADIUS,
        }
    }

    fn media_radius(self) -> &'static str {
        match self {
            Self::Xs => "0.375rem",
            _ => "0.5rem",
        }
    }

    fn title_size_px(self) -> f32 {
        match self {
            Self::Default => TITLE_SIZE_PX,
            Self::Sm => 12.0,
            Self::Xs => 11.0,
        }
    }

    fn desc_size_px(self) -> f32 {
        match self {
            Self::Default => DESC_SIZE_PX,
            Self::Sm => 10.0,
            Self::Xs => 10.0,
        }
    }
}

impl From<&str> for AttachmentSize {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::Default)
    }
}

impl From<String> for AttachmentSize {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::str::FromStr for AttachmentSize {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "sm" | "small" => Self::Sm,
            "xs" | "xsmall" | "extra-small" => Self::Xs,
            _ => Self::Default,
        })
    }
}

/// Layout axis: `horizontal` (default) | `vertical`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AttachmentOrientation {
    #[default]
    Horizontal,
    Vertical,
}

impl AttachmentOrientation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }
}

impl From<&str> for AttachmentOrientation {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::Horizontal)
    }
}

impl From<String> for AttachmentOrientation {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::str::FromStr for AttachmentOrientation {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "vertical" | "v" | "column" | "col" => Self::Vertical,
            _ => Self::Horizontal,
        })
    }
}

/// `AttachmentMedia` chrome: `icon` (default) | `image`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AttachmentMediaVariant {
    #[default]
    Icon,
    Image,
}

impl AttachmentMediaVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Icon => "icon",
            Self::Image => "image",
        }
    }
}

impl From<&str> for AttachmentMediaVariant {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::Icon)
    }
}

impl From<String> for AttachmentMediaVariant {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::str::FromStr for AttachmentMediaVariant {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "image" | "img" | "photo" | "thumbnail" => Self::Image,
            _ => Self::Icon,
        })
    }
}

// ---------------------------------------------------------------------------
// Context (child slots read parent state / size)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct AttachmentCtx {
    state: AttachmentState,
    size: AttachmentSize,
    orientation: AttachmentOrientation,
}

fn attachment_context() -> Context<AttachmentCtx> {
    static CTX: OnceLock<Context<AttachmentCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<AttachmentCtx>)
}

fn use_attachment() -> Option<AttachmentCtx> {
    use_context(attachment_context())
}

fn attachment_state_from_attrs(
    attrs: &std::collections::HashMap<String, String>,
) -> AttachmentState {
    attrs
        .get(ATTR_ATTACHMENT_STATE)
        .map(|s| AttachmentState::from(s.as_str()))
        .unwrap_or_default()
}

fn attachment_size_from_attrs(attrs: &std::collections::HashMap<String, String>) -> AttachmentSize {
    attrs
        .get(ATTR_ATTACHMENT_SIZE)
        .map(|s| AttachmentSize::from(s.as_str()))
        .unwrap_or_default()
}

fn attachment_orientation_from_attrs(
    attrs: &std::collections::HashMap<String, String>,
) -> AttachmentOrientation {
    attrs
        .get(ATTR_ATTACHMENT_ORIENTATION)
        .map(|s| AttachmentOrientation::from(s.as_str()))
        .unwrap_or_default()
}

fn attachment_chrome(state: AttachmentState) -> SemanticChrome {
    let tone = state.semantic_tone().unwrap_or(SemanticTone::Default);
    semantic_chrome(SemanticChromeOpts {
        tone,
        filled: false,
        surface: SemanticSurface::Card,
        decor: SemanticChromeDecor::BORDERED,
    })
}

fn slot_of(el: &Element) -> Option<&str> {
    el.attrs.get(ATTR_SLOT).map(String::as_str)
}

fn is_lucide_icon(el: &Element) -> bool {
    el.text_style.font_family.as_deref() == Some("lucide")
}

fn stamp_text_paint(el: &mut Element, color: [u8; 4], size_px: f32, weight: u16) {
    el.text_style.color = Some(color);
    el.text_style.size = Some(size_px);
    el.text_style.weight = Some(weight);
}

fn paint_text_nodes(el: &mut Element, color: [u8; 4], size_px: f32, weight: u16) {
    if el.text.is_some() {
        stamp_text_paint(el, color, size_px, weight);
    }
    for child in &mut el.children {
        paint_text_nodes(child, color, size_px, weight);
    }
}

fn set_pulse_class(el: &mut Element, enabled: bool) {
    let class = el.attrs.get("class").map(String::as_str).unwrap_or("");
    let has = class.split_whitespace().any(|c| c == PULSE_CLASS);
    if enabled && !has {
        *el = with_class_list(std::mem::take(el), PULSE_CLASS);
    } else if !enabled && has {
        let trimmed: String = class
            .split_whitespace()
            .filter(|c| *c != PULSE_CLASS)
            .collect::<Vec<_>>()
            .join(" ");
        if trimmed.is_empty() {
            el.attrs.remove("class");
        } else {
            el.set_attr("class", trimmed);
        }
    }
}

fn media_fg_for_state(state: AttachmentState, chrome: &SemanticChrome) -> [u8; 4] {
    if state.semantic_tone().is_some() {
        chrome.fg
    } else {
        parse_text_color("foreground")
    }
}

fn media_bg_for_state(state: AttachmentState) -> [f32; 4] {
    state
        .semantic_tone()
        .and_then(semantic_media_wash)
        .unwrap_or_else(|| parse_color("muted"))
}

fn fill_image_media_child(child: &mut Element) {
    if child.image_texture.is_some() {
        *child = std::mem::take(child).w("100%").h("100%");
    }
}

fn size_media_shell(
    mut el: Element,
    variant: AttachmentMediaVariant,
    size: AttachmentSize,
    orientation: AttachmentOrientation,
    state: AttachmentState,
) -> Element {
    el = el
        .flex_shrink(0.0)
        .rounded(size.media_radius())
        .overflow("hidden")
        .aspect_ratio(1.0);
    el = match orientation {
        AttachmentOrientation::Vertical => el.w("100%").h("auto").min_w(0.0).min_h(0.0),
        AttachmentOrientation::Horizontal => {
            let box_sz = size.media_box();
            el.w(box_sz).h(box_sz).min_w(box_sz).min_h(box_sz)
        }
    };
    if !is_variant_style_overridden(&el.attrs, VariantStyleField::Bg) {
        el.style.bg_color = media_bg_for_state(state);
    }
    if variant == AttachmentMediaVariant::Image {
        let opaque = matches!(state, AttachmentState::Done | AttachmentState::Idle);
        el = el.opacity(if opaque { 1.0 } else { 0.6 });
        for child in &mut el.children {
            fill_image_media_child(child);
        }
    } else {
        el = el.opacity(1.0);
    }
    el
}

fn media_variant_from_attrs(
    attrs: &std::collections::HashMap<String, String>,
) -> AttachmentMediaVariant {
    attrs
        .get(ATTR_VARIANT)
        .map(|s| AttachmentMediaVariant::from(s.as_str()))
        .unwrap_or_default()
}

fn repaint_attachment_media_on_place(
    el: &mut Element,
    state: AttachmentState,
    size: AttachmentSize,
    orientation: AttachmentOrientation,
    chrome: &SemanticChrome,
) {
    let variant = media_variant_from_attrs(&el.attrs);
    let taken = std::mem::take(el);
    *el = size_media_shell(taken, variant, size, orientation, state);
    el.set_attr(ATTR_ATTACHMENT_STATE, state.as_str());
    el.set_attr(ATTR_ATTACHMENT_SIZE, size.as_str());
    el.set_attr(ATTR_ATTACHMENT_ORIENTATION, orientation.as_str());
    let fg = media_fg_for_state(state, chrome);
    for child in &mut el.children {
        if is_lucide_icon(child) && child.text_style.size_resolved() <= 1.0 {
            child.text_style.size = Some(IntoF32::into_f32("0.875rem"));
        }
        if child.text.is_some() && !is_variant_style_overridden(&child.attrs, VariantStyleField::Fg)
        {
            child.text_style.color = Some(fg);
        }
    }
}

fn repaint_attachment_title_on_place(
    el: &mut Element,
    state: AttachmentState,
    size: AttachmentSize,
    chrome: &SemanticChrome,
) {
    if !is_variant_style_overridden(&el.attrs, VariantStyleField::Fg) {
        paint_text_nodes(el, chrome.fg, size.title_size_px(), TITLE_WEIGHT);
    }
    set_pulse_class(el, state.title_shimmer());
}

fn repaint_attachment_description_on_place(
    el: &mut Element,
    state: AttachmentState,
    size: AttachmentSize,
    chrome: &SemanticChrome,
) {
    if is_variant_style_overridden(&el.attrs, VariantStyleField::Fg) {
        return;
    }
    let color = if state == AttachmentState::Destructive {
        let mut c = parse_text_color("destructive");
        c[3] = ((c[3] as f32) * 0.8) as u8;
        c
    } else {
        chrome.muted_fg
    };
    paint_text_nodes(el, color, size.desc_size_px(), DESC_WEIGHT);
}

fn repaint_attachment_descendants(
    el: &mut Element,
    state: AttachmentState,
    size: AttachmentSize,
    orientation: AttachmentOrientation,
) {
    let chrome = attachment_chrome(state);
    match slot_of(el) {
        Some(SLOT_MEDIA) => {
            repaint_attachment_media_on_place(el, state, size, orientation, &chrome)
        }
        Some(SLOT_TITLE) => repaint_attachment_title_on_place(el, state, size, &chrome),
        Some(SLOT_DESCRIPTION) => repaint_attachment_description_on_place(el, state, size, &chrome),
        _ => {}
    }
    for child in &mut el.children {
        repaint_attachment_descendants(child, state, size, orientation);
    }
}

// ---------------------------------------------------------------------------
// Paint helpers
// ---------------------------------------------------------------------------

fn title_text_config(size: AttachmentSize, state: AttachmentState) -> TextNodeConfig {
    let chrome = attachment_chrome(state);
    TextNodeConfig {
        size_px: size.title_size_px(),
        weight: TITLE_WEIGHT,
        color: chrome.fg,
        ..Default::default()
    }
}

fn description_text_config(state: AttachmentState, size: AttachmentSize) -> TextNodeConfig {
    let chrome = attachment_chrome(state);
    let color = if state == AttachmentState::Destructive {
        let mut c = parse_text_color("destructive");
        c[3] = ((c[3] as f32) * 0.8) as u8;
        c
    } else {
        chrome.muted_fg
    };
    TextNodeConfig {
        size_px: size.desc_size_px(),
        weight: DESC_WEIGHT,
        color,
        ..Default::default()
    }
}

fn caption_from_children(children: Option<Element>, cfg: &TextNodeConfig) -> Vec<Element> {
    let Some(c) = children else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for mut child in take_packed(c) {
        if let Some(text) = child.text.take() {
            out.push(build_text_node(text, cfg.clone()));
        } else {
            child.text_style.color = Some(cfg.color);
            child.text_style.size = Some(cfg.size_px);
            child.text_style.weight = Some(cfg.weight);
            out.push(child);
        }
    }
    out
}

/// Keep [`bind_attr`] / [`bind_text`] identity when rebuilding a painted shell.
fn preserve_binding_identity(from: &Element, to: &mut Element) {
    to.template_id = from.template_id.clone();
    to.focus_id = from.focus_id.clone();
    to.on_click = from.on_click.clone();
}

pub fn apply_attachment_paint(
    el: &mut Element,
    state: AttachmentState,
    size: AttachmentSize,
    orientation: AttachmentOrientation,
) -> LayoutStatus {
    let prev_state = attachment_state_from_attrs(&el.attrs);
    let prev_size = attachment_size_from_attrs(&el.attrs);
    let prev_orientation = attachment_orientation_from_attrs(&el.attrs);
    let already_painted = el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_ROOT);
    if already_painted
        && prev_state == state
        && prev_size == size
        && prev_orientation == orientation
    {
        return LayoutStatus::Clean;
    }

    let size_changed = prev_size != size;
    let orientation_changed = prev_orientation != orientation;

    let mut taken = std::mem::take(el);
    let kids = std::mem::take(&mut taken.children);
    let has_media = kids.iter().any(|k| slot_of(k) == Some(SLOT_MEDIA));
    let has_content = kids.iter().any(|k| slot_of(k) == Some(SLOT_CONTENT));

    let mut painted: Element = match orientation {
        AttachmentOrientation::Horizontal => hstack()
            .items_center()
            .gap(size.gap())
            .wrap("nowrap")
            .children(kids)
            .into(),
        AttachmentOrientation::Vertical => vstack()
            .items("stretch")
            .gap(size.gap())
            .children(kids)
            .into(),
    };
    painted = painted
        .max_w("100%")
        .flex_shrink(0.0)
        .rounded(size.root_radius())
        .overflow("hidden")
        .position("relative");
    painted = match orientation {
        // Hug content; `max-w-full` already applied. `w("fit-content")` stays Auto in Taffy;
        // stretch columns still need `align-self: start` (Message content pack).
        AttachmentOrientation::Horizontal => painted.w("fit-content").min_w(0.0),
        AttachmentOrientation::Vertical => painted
            .w(if has_content {
                VERTICAL_W_CONTENT
            } else {
                VERTICAL_W
            })
            .min_w(0.0),
    };
    painted = if has_content {
        painted.px(size.px()).py(size.py())
    } else if has_media {
        painted.p(size.media_pad())
    } else {
        painted.px(size.px()).py(size.py())
    };

    painted.set_attr(ATTR_ATTACHMENT_STATE, state.as_str());
    painted.set_attr(ATTR_ATTACHMENT_SIZE, size.as_str());
    painted.set_attr(ATTR_ATTACHMENT_ORIENTATION, orientation.as_str());
    painted.set_attr(ATTR_SLOT, SLOT_ROOT);
    painted.set_attr(ATTR_STATE, state.as_str());
    painted.set_attr(ATTR_SIZE, size.as_str());
    painted.set_attr(ATTR_ORIENTATION, orientation.as_str());
    *el = painted;

    if let Some(_tone) = state.semantic_tone() {
        apply_semantic_chrome_respecting_overrides(el, &attachment_chrome(state));
    } else {
        let mut taken = std::mem::take(el);
        taken = taken
            .bg(parse_color("card"))
            .border_style(if state.uses_dashed_border() {
                "dashed"
            } else {
                "solid"
            })
            .border_width(1.0)
            .border_color(parse_color("border"));
        *el = taken;
    }
    repaint_attachment_descendants(el, state, size, orientation);

    if size_changed || orientation_changed {
        LayoutStatus::LayoutChanged
    } else {
        LayoutStatus::StyleChanged
    }
}

/// Macro bind: reactive `state={sig.get()}` on `<Attachment>`.
pub fn apply_bound_attachment_state(el: &mut Element, state: impl AsRef<str>) -> LayoutStatus {
    let size = attachment_size_from_attrs(&el.attrs);
    let orientation = attachment_orientation_from_attrs(&el.attrs);
    apply_attachment_paint(el, AttachmentState::from(state.as_ref()), size, orientation)
}

/// Macro bind: reactive `size={sig.get()}` on `<Attachment>`.
pub fn apply_bound_attachment_size(el: &mut Element, size: impl AsRef<str>) -> LayoutStatus {
    let state = attachment_state_from_attrs(&el.attrs);
    let orientation = attachment_orientation_from_attrs(&el.attrs);
    apply_attachment_paint(el, state, AttachmentSize::from(size.as_ref()), orientation)
}

/// Macro bind: reactive `orientation={sig.get()}` on `<Attachment>`.
pub fn apply_bound_attachment_orientation(
    el: &mut Element,
    orientation: impl AsRef<str>,
) -> LayoutStatus {
    let state = attachment_state_from_attrs(&el.attrs);
    let size = attachment_size_from_attrs(&el.attrs);
    apply_attachment_paint(
        el,
        state,
        size,
        AttachmentOrientation::from(orientation.as_ref()),
    )
}

fn paint_attachment_media(
    variant: AttachmentMediaVariant,
    children: Vec<Element>,
    state: AttachmentState,
    size: AttachmentSize,
    orientation: AttachmentOrientation,
) -> Element {
    let chrome = attachment_chrome(state);

    let kids: Vec<Element> = children
        .into_iter()
        .map(|mut child| {
            if is_lucide_icon(&child) {
                child = child.pointer_events_none();
                if variant == AttachmentMediaVariant::Icon
                    && child.text_style.size_resolved() <= 1.0
                {
                    child = child.text_size(IntoF32::into_f32("0.875rem"));
                }
            }
            if child.text.is_some() {
                child.text_style.color = Some(media_fg_for_state(state, &chrome));
            }
            if variant == AttachmentMediaVariant::Image {
                fill_image_media_child(&mut child);
            }
            child
        })
        .collect();

    let shell = hstack().items_center().justify_center();
    let mut el: Element = shell.children(kids).into();
    el = size_media_shell(el, variant, size, orientation, state);
    el.set_attr(ATTR_SLOT, SLOT_MEDIA);
    el.set_attr(ATTR_VARIANT, variant.as_str());
    el.set_attr(ATTR_ATTACHMENT_STATE, state.as_str());
    el.set_attr(ATTR_ATTACHMENT_SIZE, size.as_str());
    el.set_attr(ATTR_ATTACHMENT_ORIENTATION, orientation.as_str());
    el
}

fn build_attachment_media(variant: AttachmentMediaVariant, children: Vec<Element>) -> Element {
    let ctx = use_attachment();
    paint_attachment_media(
        variant,
        children,
        ctx.map(|c| c.state).unwrap_or(AttachmentState::Done),
        ctx.map(|c| c.size).unwrap_or(AttachmentSize::Default),
        ctx.map(|c| c.orientation)
            .unwrap_or(AttachmentOrientation::Horizontal),
    )
}

pub fn apply_bound_attachment_media_variant(
    el: &mut Element,
    variant: impl AsRef<str>,
) -> LayoutStatus {
    let v = AttachmentMediaVariant::from(variant.as_ref());
    let prev = media_variant_from_attrs(&el.attrs);
    if prev == v {
        return LayoutStatus::Clean;
    }
    let kids = std::mem::take(&mut el.children);
    let mut rebuilt = paint_attachment_media(
        v,
        kids,
        attachment_state_from_attrs(&el.attrs),
        attachment_size_from_attrs(&el.attrs),
        attachment_orientation_from_attrs(&el.attrs),
    );
    preserve_binding_identity(el, &mut rebuilt);
    *el = rebuilt;
    LayoutStatus::StyleChanged
}

fn build_attachment_title(children: Vec<Element>) -> Element {
    let ctx = use_attachment();
    let state = ctx.map(|c| c.state).unwrap_or(AttachmentState::Done);
    let size = ctx.map(|c| c.size).unwrap_or(AttachmentSize::Default);
    let cfg = title_text_config(size, state);
    let kids = caption_from_children(Some(Element::new().children(children)), &cfg);
    let mut el: Element = hstack()
        .w("100%")
        .max_w("100%")
        .min_w(0.0)
        .items_center()
        .children(kids)
        .into();
    el = el.text_overflow_ellipsis().overflow_x("hidden");
    if state.title_shimmer() {
        el = with_class_list(el, PULSE_CLASS);
    }
    el.set_attr(ATTR_SLOT, SLOT_TITLE);
    el
}

fn build_attachment_description(children: Vec<Element>) -> Element {
    let ctx = use_attachment();
    let state = ctx.map(|c| c.state).unwrap_or(AttachmentState::Done);
    let size = ctx.map(|c| c.size).unwrap_or(AttachmentSize::Default);
    let cfg = description_text_config(state, size);
    let kids = caption_from_children(Some(Element::new().children(children)), &cfg);
    let mut el: Element = hstack()
        .w("100%")
        .max_w("100%")
        .min_w(0.0)
        .items_center()
        .children(kids)
        .into();
    el = el.text_overflow_ellipsis().overflow_x("hidden");
    el.set_attr(ATTR_SLOT, SLOT_DESCRIPTION);
    el
}

// ---------------------------------------------------------------------------
// RSX parts
// ---------------------------------------------------------------------------

/// Horizontal snap scroll row for multiple attachments.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AttachmentGroup(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = hstack()
        .min_w(0.0)
        .items_center()
        .gap(tokens::SPACE_2)
        .overflow_x("auto")
        .overflow_y("hidden")
        .scroll_behavior("smooth")
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_GROUP);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Root attachment chip (`state` · `size` · `orientation`).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Attachment(
    state: Option<AttachmentState>,
    size: Option<AttachmentSize>,
    orientation: Option<AttachmentOrientation>,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let state = state.unwrap_or_default();
    let size = size.unwrap_or_default();
    let orientation = orientation.unwrap_or_default();
    provide_context(
        attachment_context(),
        AttachmentCtx {
            state,
            size,
            orientation,
        },
    );
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = hstack().children(kids).into();
    apply_attachment_paint(&mut el, state, size, orientation);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Media thumbnail or icon slot (`variant`: `icon` | `image`).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AttachmentMedia(
    variant: Option<AttachmentMediaVariant>,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let variant = variant.unwrap_or_default();
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = build_attachment_media(variant, kids);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Main text column (`flex-1` · `min-w-0`).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AttachmentContent(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = vstack()
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .min_w(0.0)
        .max_w("100%")
        .gap("0.125rem")
        .items("start")
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_CONTENT);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Primary filename label (ellipsis · pulse while uploading / processing).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AttachmentTitle(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = build_attachment_title(kids);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Secondary meta line (size · status · error copy).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AttachmentDescription(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = build_attachment_description(kids);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Trailing action cluster (icon buttons).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AttachmentActions(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = hstack()
        .items_center()
        .flex_shrink(0.0)
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_ACTIONS);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Single trailing action — thin wrapper over [`Button`] (`ghost` · `icon-xs` defaults).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AttachmentAction(
    id: String,
    variant: Option<ButtonVariant>,
    size: Option<ButtonSize>,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let variant = variant.unwrap_or(ButtonVariant::Ghost);
    let size = size.unwrap_or(ButtonSize::IconXs);
    let size_key = match size {
        ButtonSize::Inline => "inline",
        ButtonSize::Xs => "xs",
        ButtonSize::Sm => "sm",
        ButtonSize::Lg => "lg",
        ButtonSize::Icon => "icon",
        ButtonSize::IconXs => "icon-xs",
        ButtonSize::IconSm => "icon-sm",
        ButtonSize::IconLg => "icon-lg",
        ButtonSize::Default => "default",
    };
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = button(&id)
        .variant(variant.as_paint_key())
        .size(size_key)
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_ACTION);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Invisible full-bleed overlay button (open file / retry upload).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AttachmentTrigger(
    id: String,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = button(&id)
        .variant("ghost")
        .size("icon-xs")
        .children(kids)
        .into();
    el = el.absolute().inset(0.0).z_index(10).opacity(0.0);
    el.set_attr(ATTR_SLOT, SLOT_TRIGGER);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

// ---------------------------------------------------------------------------
// Builders (ADR-9 surface B)
// ---------------------------------------------------------------------------

pub struct AttachmentGroupBuilder {
    children: Vec<Element>,
}

pub fn attachment_group() -> AttachmentGroupBuilder {
    AttachmentGroupBuilder {
        children: Vec::new(),
    }
}

impl AttachmentGroupBuilder {
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }
}

impl From<AttachmentGroupBuilder> for Element {
    fn from(builder: AttachmentGroupBuilder) -> Self {
        AttachmentGroup(AttachmentGroupProps {
            children: if builder.children.is_empty() {
                None
            } else {
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                Some(frag.children(builder.children))
            },
            base: None,
            base_builder: None,
            ..Default::default()
        })
    }
}

pub struct AttachmentBuilder {
    state: AttachmentState,
    size: AttachmentSize,
    orientation: AttachmentOrientation,
    children: Vec<Element>,
}

pub fn attachment() -> AttachmentBuilder {
    AttachmentBuilder {
        state: AttachmentState::Done,
        size: AttachmentSize::Default,
        orientation: AttachmentOrientation::Horizontal,
        children: Vec::new(),
    }
}

impl AttachmentBuilder {
    pub fn state(mut self, s: impl Into<AttachmentState>) -> Self {
        self.state = s.into();
        self
    }

    pub fn size(mut self, s: impl Into<AttachmentSize>) -> Self {
        self.size = s.into();
        self
    }

    pub fn orientation(mut self, o: impl Into<AttachmentOrientation>) -> Self {
        self.orientation = o.into();
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn media(mut self, child: impl Into<Element>) -> Self {
        self.children.push(AttachmentMedia(AttachmentMediaProps {
            variant: Some(AttachmentMediaVariant::Icon),
            children: Some(child.into()),
            base: None,
            base_builder: None,
            ..Default::default()
        }));
        self
    }

    pub fn media_icon(mut self, child: impl Into<Element>) -> Self {
        self.children.push(AttachmentMedia(AttachmentMediaProps {
            variant: Some(AttachmentMediaVariant::Icon),
            children: Some(child.into()),
            base: None,
            base_builder: None,
            ..Default::default()
        }));
        self
    }

    pub fn media_image(mut self, child: impl Into<Element>) -> Self {
        self.children.push(AttachmentMedia(AttachmentMediaProps {
            variant: Some(AttachmentMediaVariant::Image),
            children: Some(child.into()),
            base: None,
            base_builder: None,
            ..Default::default()
        }));
        self
    }

    pub fn content(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn title_description(
        mut self,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        self.children
            .push(AttachmentContent(AttachmentContentProps {
                children: {
                    let mut frag = Element::new();
                    frag.set_attr("rui_fragment", "1");
                    Some(frag.children([
                        AttachmentTitle(AttachmentTitleProps {
                            children: Some(Element::new().text(title.into())),
                            base: None,
                            base_builder: None,
                            ..Default::default()
                        }),
                        AttachmentDescription(AttachmentDescriptionProps {
                            children: Some(Element::new().text(description.into())),
                            base: None,
                            base_builder: None,
                            ..Default::default()
                        }),
                    ]))
                },
                base: None,
                base_builder: None,
                ..Default::default()
            }));
        self
    }

    pub fn title(mut self, text: impl Into<String>) -> Self {
        self.children
            .push(AttachmentContent(AttachmentContentProps {
                children: {
                    let mut frag = Element::new();
                    frag.set_attr("rui_fragment", "1");
                    Some(frag.children([AttachmentTitle(AttachmentTitleProps {
                        children: Some(Element::new().text(text.into())),
                        base: None,
                        base_builder: None,
                        ..Default::default()
                    })]))
                },
                base: None,
                base_builder: None,
                ..Default::default()
            }));
        self
    }

    pub fn action(mut self, child: impl Into<Element>) -> Self {
        self.children
            .push(AttachmentActions(AttachmentActionsProps {
                children: {
                    let mut frag = Element::new();
                    frag.set_attr("rui_fragment", "1");
                    Some(frag.children([child.into()]))
                },
                base: None,
                base_builder: None,
                ..Default::default()
            }));
        self
    }
}

impl From<AttachmentBuilder> for Element {
    fn from(builder: AttachmentBuilder) -> Self {
        Attachment(AttachmentProps {
            state: Some(builder.state),
            size: Some(builder.size),
            orientation: Some(builder.orientation),
            children: if builder.children.is_empty() {
                None
            } else {
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                Some(frag.children(builder.children))
            },
            base: None,
            base_builder: None,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_enums_parse() {
        assert_eq!(
            AttachmentState::from("uploading"),
            AttachmentState::Uploading
        );
        assert_eq!(AttachmentState::from("error"), AttachmentState::Destructive);
        assert_eq!(AttachmentState::from("warn"), AttachmentState::Warning);
        assert_eq!(AttachmentState::from("info"), AttachmentState::Info);
        assert_eq!(AttachmentSize::from("xs"), AttachmentSize::Xs);
        assert_eq!(
            AttachmentOrientation::from("vertical"),
            AttachmentOrientation::Vertical
        );
        assert_eq!(
            AttachmentMediaVariant::from("image"),
            AttachmentMediaVariant::Image
        );
    }

    #[test]
    fn attachment_paint_sets_slots() {
        let mut el = Element::new();
        assert_eq!(
            apply_attachment_paint(
                &mut el,
                AttachmentState::Destructive,
                AttachmentSize::Sm,
                AttachmentOrientation::Horizontal,
            ),
            LayoutStatus::LayoutChanged
        );
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
        assert_eq!(
            el.attrs.get(ATTR_STATE).map(String::as_str),
            Some("destructive")
        );
        assert_eq!(
            apply_attachment_paint(
                &mut el,
                AttachmentState::Destructive,
                AttachmentSize::Sm,
                AttachmentOrientation::Horizontal,
            ),
            LayoutStatus::Clean
        );
        assert_eq!(
            apply_attachment_paint(
                &mut el,
                AttachmentState::Idle,
                AttachmentSize::Sm,
                AttachmentOrientation::Horizontal,
            ),
            LayoutStatus::StyleChanged
        );
    }

    #[test]
    fn attachment_group_slot() {
        let group = AttachmentGroup(AttachmentGroupProps {
            children: None,
            base: None,
            base_builder: None,
            ..Default::default()
        });
        assert_eq!(
            group.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_GROUP)
        );
    }

    #[test]
    fn apply_bound_attachment_state_repaints_title_pulse() {
        fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
            if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
                return Some(el);
            }
            for child in &el.children {
                if let Some(found) = find_slot(child, slot) {
                    return Some(found);
                }
            }
            None
        }

        let root = Attachment(AttachmentProps {
            state: Some(AttachmentState::Done),
            size: Some(AttachmentSize::Default),
            orientation: Some(AttachmentOrientation::Horizontal),
            children: {
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                Some(frag.children([AttachmentTitle(AttachmentTitleProps {
                    children: Some(Element::new().text("file.txt")),
                    base: None,
                    base_builder: None,
                    ..Default::default()
                })]))
            },
            base: None,
            base_builder: None,
            ..Default::default()
        });
        let mut el = root;
        assert_eq!(
            apply_bound_attachment_state(&mut el, "uploading"),
            LayoutStatus::StyleChanged
        );
        let title = find_slot(&el, SLOT_TITLE).expect("title slot");
        let has_pulse = title
            .attrs
            .get("rui_utility_class")
            .map(|c| c.contains(PULSE_CLASS))
            .unwrap_or(false)
            || title
                .class_set
                .as_ref()
                .is_some_and(|cs| cs.base.iter().any(|t| t.contains(PULSE_CLASS)));
        assert!(has_pulse, "uploading should enable animate-pulse on title");
    }

    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
            return Some(el);
        }
        for child in &el.children {
            if let Some(found) = find_slot(child, slot) {
                return Some(found);
            }
        }
        None
    }

    #[test]
    fn default_done_horizontal_paints_card_chrome() {
        let mut el = Element::new();
        assert_ne!(
            apply_attachment_paint(
                &mut el,
                AttachmentState::Done,
                AttachmentSize::Default,
                AttachmentOrientation::Horizontal,
            ),
            LayoutStatus::Clean
        );
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
        assert_eq!(
            crate::layout::dim_length_px(el.style.min_size.width),
            Some(0.0)
        );
        assert_eq!(el.style.flex_wrap, taffy::style::FlexWrap::NoWrap);
        assert_eq!(
            apply_attachment_paint(
                &mut el,
                AttachmentState::Done,
                AttachmentSize::Default,
                AttachmentOrientation::Horizontal,
            ),
            LayoutStatus::Clean
        );
    }

    #[test]
    fn attachment_title_is_full_width_for_ellipsis() {
        let el = AttachmentTitle(AttachmentTitleProps {
            children: Some(Element::new().text("very-long-filename.pdf")),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        assert!(
            matches!(
                crate::layout::dim_kind(el.style.size.width),
                crate::layout::DimKind::Percent(p) if (p - 1.0).abs() < 0.001
            ),
            "title must be w-full so a clamped chip can ellipsize"
        );
    }

    #[test]
    fn vertical_image_preview_fills_media_and_uses_w24() {
        let mut img = Element::new();
        img.image_texture = Some("demo-checker".into());
        let root = Attachment(AttachmentProps {
            orientation: Some(AttachmentOrientation::Vertical),
            children: {
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                Some(frag.children([AttachmentMedia(AttachmentMediaProps {
                    variant: Some(AttachmentMediaVariant::Image),
                    children: Some(img),
                    base: None,
                    base_builder: None,
                    ..Default::default()
                })]))
            },
            base: None,
            base_builder: None,
            ..Default::default()
        });
        assert_eq!(
            crate::layout::dim_length_px(root.style.size.width),
            Some(96.0)
        );
        let media = find_slot(&root, SLOT_MEDIA).expect("media slot");
        assert!(
            matches!(
                crate::layout::dim_kind(media.style.size.width),
                crate::layout::DimKind::Percent(p) if (p - 1.0).abs() < 0.001
            ),
            "vertical image media should be w-full"
        );
        let photo = media.children.first().expect("image child");
        assert!(
            matches!(
                crate::layout::dim_kind(photo.style.size.width),
                crate::layout::DimKind::Percent(p) if (p - 1.0).abs() < 0.001
            ),
            "image child should fill the media well"
        );
    }
}
