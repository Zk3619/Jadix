//! Toaster / portal 宿主。

use super::view::render_queue;
use super::*;
use crate::app::portal_overlay::{OverlayKind, OverlayMode, portal_sync};
use crate::utils::keyed_overlay::{mount_reactive_anchor, viewport_portal_stage};

/// Builder：配置宿主布局/锚点/上限策略。
pub struct ToasterBuilder {
    variant: ToastLayout,
    position: String,
    overflow: ToastOverflow,
    limit: usize,
}

pub fn toaster() -> ToasterBuilder {
    ToasterBuilder {
        variant: ToastLayout::Stack,
        position: "top-center".into(),
        overflow: ToastOverflow::Displace,
        limit: DEFAULT_LIMIT,
    }
}

/// 兼容旧名：默认 `stack` 宿主。
pub fn toast_host() -> Element {
    toaster().into()
}

impl ToasterBuilder {
    /// `stack`（默认）| `list`。切换时套用布局默认锚点（stack→顶中，list→右下）。
    pub fn variant(mut self, v: impl Into<String>) -> Self {
        self.variant = ToastLayout::parse(&v.into());
        self.position = match self.variant {
            ToastLayout::List => ToastPosition::BottomRight.as_str().to_string(),
            ToastLayout::Stack => ToastPosition::TopCenter.as_str().to_string(),
        };
        self
    }
    pub fn layout(mut self, layout: ToastLayout) -> Self {
        self.variant = layout;
        self.position = match layout {
            ToastLayout::List => ToastPosition::BottomRight.as_str().to_string(),
            ToastLayout::Stack => ToastPosition::TopCenter.as_str().to_string(),
        };
        self
    }
    /// `top-center`（stack 默认）| `top-right` | `top-left` | `bottom-left` | `bottom-center` | `bottom-right`（list 默认）。
    pub fn position(mut self, p: impl Into<String>) -> Self {
        self.position = ToastPosition::parse(&p.into()).as_str().to_string();
        self
    }
    /// 上限策略：`displace`（挤出最旧）| `wait`（等待空位）。
    pub fn overflow(mut self, v: impl Into<String>) -> Self {
        self.overflow = ToastOverflow::parse(&v.into());
        self
    }
    pub fn overflow_policy(mut self, policy: ToastOverflow) -> Self {
        self.overflow = policy;
        self
    }
    /// 同时可见上限（默认 3，至少 1）。
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = n.max(1);
        self
    }
}

impl crate::core::tree::IntoElementList for ToasterBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<ToasterBuilder> for Element {
    fn from(builder: ToasterBuilder) -> Self {
        layout_signal().set(builder.variant.as_str().to_string());
        position_signal().set(ToastPosition::parse(&builder.position).as_str().to_string());
        overflow_signal().set(builder.overflow.as_str().to_string());
        limit_signal().set(builder.limit.max(1));
        match builder.overflow {
            ToastOverflow::Displace => flush_pending_by_displace(),
            ToastOverflow::Wait => promote_pending(),
        }
        mount_toast_host()
    }
}

pub(super) const TOAST_HOST_VIEW: &str = "__rui_toast_host_view";

/// 反应根必须是**非 portal** 外壳。若把 `rui_portal` 当 `__rui_toast_host_view` 根，
/// hoist 后身份落到 portal 层；Command Modal 关闭再调和会找不到壳，后续通知全部不上屏。
fn toast_host_idle() -> Element {
    Element::new().w(0.0).h(0.0).pointer_events_none()
}

/// 铺满视口的舞台（提升到窗口根 portal 层）；卡片在其内按 `position` 锚点摆放。
fn toast_viewport_stage(panel: Element) -> Element {
    viewport_portal_stage("toast-host", panel, TOAST_PORTAL_Z)
}

pub(crate) fn paint_toast_host() -> Element {
    let open = use_signal(TOAST_HOST_OPEN, || true);
    let _ = tick_toast_lifecycle_commit(false);
    let q = toast_queue().get();
    let _ = open.get();
    let _ = toast_pending().get(); // 等待队列变化时重绘（角标/状态）
    let layout = ToastLayout::parse(&layout_signal().get());
    let position = position_signal().get();
    // 仅 peek：悬停展开由卡/堆 sync 热更新，勿订阅以免整树重建。
    let expanded = stack_expanded_now();
    let _ = overflow_signal().get();
    let _ = limit_signal().get();
    // 与 Tooltip 相同：Pinned 登记，不进 Modal 栈。关 Command 不得把队列藏起来。
    let _ = portal_sync(
        "toast-host",
        open,
        OverlayKind::Tooltip,
        OverlayMode::Pinned,
    );
    if q.is_empty() && toast_pending().peek().is_empty() {
        if expanded_signal().peek() {
            expanded_signal().set(false);
        }
        set_stack_timers_paused(false);
        return toast_host_idle();
    }
    if q.is_empty() {
        sync_pending_when_visible_empty(&q);
        return toast_host_idle();
    }
    let panel = render_queue(layout, position.as_str(), &q, expanded);
    toast_host_idle().child(toast_viewport_stage(panel))
}

fn mount_toast_host() -> Element {
    mount_reactive_anchor(TOAST_HOST_VIEW, paint_toast_host)
}

/// RSX `<Toaster variant="stack" overflow="displace" limit={3} position="top-right" />`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Toaster(
    variant: Option<String>,
    position: Option<String>,
    overflow: Option<String>,
    limit: Option<i32>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = toaster();
    if let Some(v) = variant {
        b = b.variant(v);
    }
    if let Some(p) = position {
        b = b.position(p);
    }
    if let Some(o) = overflow {
        b = b.overflow(o);
    }
    if let Some(n) = limit {
        b = b.limit(n.max(1) as usize);
    }
    b.into()
}
