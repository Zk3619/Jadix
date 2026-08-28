//! Dialog / Sheet 共享模态壳：Portal z 同步、遮罩默认色 / 不透明度 / blur。
//!
//! 面板表面与布局仍留在各组件（居中 Dialog vs 贴边 Sheet）。

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode, OverlayZ};
use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use crate::primitives::overlay::overlay;
use crate::style::parse_color;
use crate::tokens;

/// 模态遮罩默认不透明度（alpha）。
pub(crate) const DEFAULT_MODAL_OVERLAY_OPACITY: f32 = 0.25;
/// 模态遮罩默认 backdrop 模糊（逻辑 px；相对文档示例 `8` 减半，对齐 `backdrop-blur-sm`）。
pub(crate) const DEFAULT_MODAL_OVERLAY_BLUR: f32 = 2.0;

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

/// `color`：语义色 / hex；缺省用 `background` 的 RGB。  
/// `opacity`：遮罩 alpha，缺省 [`DEFAULT_MODAL_OVERLAY_OPACITY`]。
pub(crate) fn default_modal_overlay_color(
    color: Option<&str>,
    opacity: Option<f32>,
) -> [f32; 4] {
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
