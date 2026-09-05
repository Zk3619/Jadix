//! Dialog / Sheet 共享模态壳：Portal z 同步、遮罩默认色 / 不透明度 / blur。
//!
//! 面板表面与布局仍留在各组件（居中 Dialog vs 贴边 Sheet）。

use crate::app::portal_overlay::{
    portal_sync, portal_sync_presence, OverlayKind, OverlayMode, OverlayZ,
};
use crate::core::reactivity::Signal;
use crate::core::tree::{Element, LayoutStatus};
use crate::primitives::overlay::overlay;
use crate::style::{parse_color, semantic_modal_overlay_scrim, SemanticTone};
use crate::tokens;

/// 模态遮罩默认不透明度（alpha）。
pub const DEFAULT_MODAL_OVERLAY_OPACITY: f32 = 0.25;
/// 模态遮罩默认 backdrop 模糊（逻辑 px；相对文档示例 `8` 减半，对齐 `backdrop-blur-sm`）。
pub const DEFAULT_MODAL_OVERLAY_BLUR: f32 = 2.0;

/// Portal underlay / panel z-index（[`modal_sync`]）。
pub(crate) struct ModalShellZs {
    pub underlay: i32,
    pub panel: i32,
}

/// 遮罩外观与交互（产品默认：opacity 0.6、blur 4，点击关闭）。
#[derive(Clone, Debug)]
pub(crate) struct ShellOverlayOpts {
    pub color: [f32; 4],
    pub close_on_click: bool,
    pub backdrop_blur: f32,
}

impl ShellOverlayOpts {
    pub(crate) fn from_content_parts(
        color: Option<&str>,
        opacity: Option<f32>,
        close_on_click: bool,
        overlay_blur: Option<f32>,
    ) -> Self {
        Self::from_parts(
            color,
            opacity,
            close_on_click,
            overlay_blur.unwrap_or(DEFAULT_MODAL_OVERLAY_BLUR),
        )
    }

    /// Variant-aware scrim: semantic tone tints the underlay unless `color` is explicit.
    pub(crate) fn from_semantic_parts(
        tone: SemanticTone,
        color: Option<&str>,
        opacity: Option<f32>,
        close_on_click: bool,
        overlay_blur: Option<f32>,
    ) -> Self {
        Self {
            color: semantic_modal_overlay_scrim(tone, color, opacity),
            close_on_click,
            backdrop_blur: overlay_blur.unwrap_or(DEFAULT_MODAL_OVERLAY_BLUR).max(0.0),
        }
    }

    fn from_parts(
        color: Option<&str>,
        opacity: Option<f32>,
        close_on_click: bool,
        backdrop_blur: f32,
    ) -> Self {
        Self {
            color: default_modal_overlay_color(color, opacity),
            close_on_click,
            backdrop_blur: backdrop_blur.max(0.0),
        }
    }
}

/// `color`: semantic color / hex; the default is RGB of `background`.  
/// `opacity`：遮罩 alpha，缺省 [`DEFAULT_MODAL_OVERLAY_OPACITY`]。
pub(crate) fn default_modal_overlay_color(color: Option<&str>, opacity: Option<f32>) -> [f32; 4] {
    let mut c = match color {
        Some(s) if !s.is_empty() => parse_color(s),
        _ => tokens::background(),
    };
    c[3] = opacity
        .unwrap_or(DEFAULT_MODAL_OVERLAY_OPACITY)
        .clamp(0.0, 1.0);
    c
}

pub(crate) fn modal_sync(id: &str, open: Signal<bool>) -> Option<ModalShellZs> {
    let oz: OverlayZ = portal_sync(id, open, OverlayKind::Modal, OverlayMode::Stacked)?;
    Some(ModalShellZs {
        underlay: oz.underlay,
        panel: oz.panel,
    })
}

/// Esc / overlay click 关逻辑 `open`；`mounted` 为 true 时仍占栈以便播退场。
pub(crate) fn modal_sync_presence(
    id: &str,
    open: Signal<bool>,
    mounted: Signal<bool>,
) -> Option<ModalShellZs> {
    let oz: OverlayZ =
        portal_sync_presence(id, open, mounted, OverlayKind::Modal, OverlayMode::Stacked)?;
    Some(ModalShellZs {
        underlay: oz.underlay,
        panel: oz.panel,
    })
}

pub(crate) fn build_modal_overlay(
    open: Signal<bool>,
    underlay_z: i32,
    opts: &ShellOverlayOpts,
) -> Element {
    let mut b = overlay(underlay_z)
        .color(opts.color)
        .backdrop_blur(opts.backdrop_blur);
    if opts.close_on_click {
        b = b.close_on_click(open);
    }
    b.into()
}

/// In-place underlay repaint for reactive semantic variant binds (`{modal_id}__modal_overlay`).
pub(crate) fn sync_modal_overlay_scrim(
    modal_id: &str,
    tone: SemanticTone,
    explicit_color: Option<&str>,
    explicit_opacity: Option<f32>,
) -> LayoutStatus {
    let color = semantic_modal_overlay_scrim(tone, explicit_color, explicit_opacity);
    let template_id = format!("{modal_id}__modal_overlay");
    crate::app::runtime::try_update_element_by_template_id(&template_id, |el| {
        if el.style.bg_color == color {
            return LayoutStatus::Clean;
        }
        el.style.bg_color = color;
        LayoutStatus::StyleChanged
    })
    .unwrap_or(LayoutStatus::Clean)
}
