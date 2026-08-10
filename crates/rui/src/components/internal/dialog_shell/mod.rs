//! Dialog / AlertDialog 共享模态壳（[`crate::components::internal`]，对齐 solid-ui `DialogContent`）。
//!
//! 产品面：[`crate::components::dialog`] / [`crate::components::alert_dialog`]。
//!
//! 结构：
//! `Portal` → 全屏居中层 → [`crate::primitives::overlay`] + `Content`  
//! Content 内：`Header`（可选）/ **可滚动 Body** / `Footer`（可选）。

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode, OverlayZ};
use crate::components::scroll_area::scroll_area;
use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use crate::primitives::overlay::overlay;
use crate::primitives::r#box::{r#box, vstack};
use crate::layout::layout_tree::ATTR_PORTAL_KEY;
use crate::style::parse_color;
use crate::tokens;
use crate::utils::popup::{portal_panel, swallow_clicks};

/// `portal_sync` 成功时的 z 分配。
pub(crate) struct ModalShellZs {
    pub underlay: i32,
    pub panel: i32,
}

/// 遮罩外观与交互（产品默认：黑 × opacity 0.5，点击关闭；对齐 shadcn 磨砂遮罩）。
#[derive(Clone, Debug)]
pub(crate) struct DialogOverlayOpts {
    pub color: [f32; 4],
    pub close_on_click: bool,
    pub backdrop_blur: f32,
}

impl Default for DialogOverlayOpts {
    fn default() -> Self {
        Self {
            color: default_overlay_color(None, None),
            close_on_click: true,
            backdrop_blur: 0.0,
        }
    }
}

impl DialogOverlayOpts {
    pub(crate) fn from_parts(
        color: Option<&str>,
        opacity: Option<f32>,
        close_on_click: bool,
        backdrop_blur: f32,
    ) -> Self {
        Self {
            color: default_overlay_color(color, opacity),
            close_on_click,
            backdrop_blur: backdrop_blur.max(0.0),
        }
    }
}

/// `color`：语义色 / hex；缺省用 `background` 的 RGB。  
/// `opacity`：遮罩 alpha，缺省 `0.5`。
pub(crate) fn default_overlay_color(color: Option<&str>, opacity: Option<f32>) -> [f32; 4] {
    let mut c = match color {
        Some(s) if !s.is_empty() => parse_color(s),
        _ => tokens::background(),
    };
    c[3] = opacity.unwrap_or(0.5).clamp(0.0, 1.0);
    c
}

/// Header / Body / Footer 分区（滚动仅发生在 Body）。
pub(crate) struct DialogSections {
    pub header: Option<Element>,
    pub body: Element,
    pub footer: Option<Element>,
    /// Body 内 ScrollArea 实例键（须稳定唯一）。
    pub body_scroll_id: String,
}

/// 登记模态浮层。
pub(crate) fn modal_sync(id: &str, open: Signal<bool>) -> Option<ModalShellZs> {
    let oz: OverlayZ = portal_sync(id, open, OverlayKind::Modal, OverlayMode::Stacked)?;
    Some(ModalShellZs {
        underlay: oz.underlay,
        panel: oz.panel,
    })
}

/// 产品层遮罩 → [`overlay`] 原语。
pub(crate) fn dialog_overlay(
    open: Signal<bool>,
    underlay_z: i32,
    opts: &DialogOverlayOpts,
) -> Element {
    let mut b = overlay(underlay_z)
        .color(opts.color)
        .backdrop_blur(opts.backdrop_blur);
    if opts.close_on_click {
        b = b.close_on_click(open);
    }
    b.into()
}

/// Content 外壳表面（不含内部分区）。
pub(crate) fn apply_dialog_shell_surface(mut panel: Element) -> Element {
    panel = panel
        .bg(tokens::background())
        .rounded("0.5rem")
        .border_style("solid")
        .border_width(1.0)
        .border_color(tokens::border())
        .shadow([0.0, 0.0, 0.0, 0.16], 16.0)
        .p("1.5rem")
        .max_w("85dvw")
        .w("100%")
        .max_h("85dvh")
        .overflow_y("hidden")
        .overflow_x("hidden");
    panel
}

/// 组装 Header / 可滚动 Body / Footer。
pub(crate) fn dialog_sections(sections: DialogSections) -> Element {
    let DialogSections {
        header,
        body,
        footer,
        body_scroll_id,
    } = sections;

    let mut col: Element = vstack().w("100%").gap("1rem").min_h("0").into();
    col = col.flex_grow(1.0).min_h("0").overflow_y("hidden");

    if let Some(h) = header {
        let mut head = h;
        head = head.w("100%").flex_shrink(0.0);
        col = col.child(head);
    }

    let body_scroll: Element = scroll_area(body_scroll_id.as_str())
        .overflow_x("hidden")
        .overflow_y("auto")
        .scrollbar_visibility("auto")
        .w("100%")
        .h("auto")
        .min_h("0")
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .child(body)
        .into();
    col = col.child(body_scroll);

    if let Some(f) = footer {
        let mut foot = f;
        foot = foot.w("100%").flex_shrink(0.0);
        col = col.child(foot);
    }

    col
}

/// Portal + Overlay + Content 壳；`inner` 为面板内整棵子树。
/// `chrome_close`：可选右上角关闭钮（相对面板 `absolute`）。
pub(crate) fn dialog_shell_frame(
    open: Signal<bool>,
    zs: &ModalShellZs,
    inner: Element,
    overlay_opts: &DialogOverlayOpts,
    chrome_close: Option<Element>,
) -> Element {
    let overlay_el = dialog_overlay(open, zs.underlay, overlay_opts);

    // Swallow clicks only — `panel_block_clicks` would nest a second `rui_portal` under stage.
    let mut panel: Element = vstack()
        .w("100%")
        .min_h("0")
        .child(inner)
        .into();
    panel = apply_dialog_shell_surface(panel);
    panel = swallow_clicks(panel.relative());
    if let Some(mut close) = chrome_close {
        close = close.absolute().top("1rem").right("1rem").z_index(1);
        panel = panel.child(close);
    }

    let mut stage: Element = r#box()
        .absolute(true)
        .inset(0.0)
        .w("100%")
        .h("100%")
        .items_center()
        .justify_center()
        .child(panel)
        .into();
    stage.set_attr(ATTR_PORTAL_KEY, format!("panel:{}", zs.panel));
    stage = portal_panel(stage, zs.panel);
    stage = stage.pointer_events_box_none();

    Element::new()
        .child(overlay_el)
        .child(stage)
        .pointer_events_box_none()
}

/// Portal + Overlay + Content（AlertDialog 等分区 API）；遮罩用默认。
pub(crate) fn dialog_content_frame(
    open: Signal<bool>,
    zs: &ModalShellZs,
    sections: DialogSections,
) -> Element {
    dialog_shell_frame(
        open,
        zs,
        dialog_sections(sections),
        &DialogOverlayOpts::default(),
        None,
    )
}
