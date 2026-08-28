//! # Progress
//!
//! 对照 ShadCN **base** [`progress.tsx`](../../../../reference/ShadCN-UI/apps/v4/registry/bases/base/ui/progress.tsx)：
//!
//! ```text
//! Progress                 // Root：value ∈ 0..=1；自动挂 Track + Indicator
//! ├── ProgressLabel        // 可选标题
//! ├── ProgressValue        // 可选百分比（bind_text）
//! └── ProgressTrack        // cn-progress-track（也可单独使用）
//!     └── ProgressIndicator
//! ```
//!
//! 轨道样式对位 `cn-progress-track` / `cn-progress-indicator`：`h-2` · `rounded-full` ·
//! `bg-primary/20` 轨 + `bg-primary` 指示条 · `w-full`。
//!
//! `ui!` 中 `<Progress>` 由宏展开为 [`progress_scope`]（子树先拿到上下文）。
//! Builder [`progress`] 简单条与 `.child(|| …)` 复合面共用同一套实现。
//!
//! ## 用法（双写）
//! ```ignore
//! // A · RSX（camelCase 宏别名：valueSignal）
//! ui! {
//!     <Progress id="upload" valueSignal={pct}>
//!         <ProgressLabel>"Uploading"</ProgressLabel>
//!         <ProgressValue />
//!     </Progress>
//!     <Progress value={0.4} />
//! }
//!
//! // B · Builder（只 snake_case）
//! progress()
//!     .id("upload")
//!     .value_signal(pct)
//!     .child(|| {
//!         let mut frag = Element::new();
//!         frag.set_attr("rui_fragment", "1");
//!         frag.children(vec![
//!             ProgressLabel(ProgressLabelProps {
//!                 children: Some(Element::new().text("Uploading")),
//!                 base: None,
//!             }),
//!             ProgressValue(ProgressValueProps {
//!                 children: None,
//!                 base: None,
//!             }),
//!         ])
//!     })
//! // 简单条：
//! progress().default_value(0.4)
//! ```
//!
//! **身份（WP-2.8）**：`id` 可选；空串 → [`resolve_instance_id`]。

use std::sync::OnceLock;

use crate::app::reactive::{bind_text, reactive_view};
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, with_child_owner, Context, Signal,
};
use crate::core::tree::Element;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::primitives::text::text;
use crate::style::{parse_text_color, with_class_list};
use crate::tokens;
use crate::utils::resolve_instance_id;
use rui_macros::component;

// ---------------------------------------------------------------------------
// Context / scope
// ---------------------------------------------------------------------------

/// Root 上下文：进度 `value` ∈ `0..=1` + 稳定实例 `id`。
#[derive(Clone)]
pub struct ProgressCtx {
    pub value: Signal<f32>,
    pub id: String,
}

fn progress_context() -> Context<ProgressCtx> {
    static CTX: OnceLock<Context<ProgressCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<ProgressCtx>)
}

/// 读取最近的 [`Progress`] 上下文。
pub fn use_progress() -> Option<ProgressCtx> {
    use_context(progress_context())
}

fn require_progress(part: &str) -> ProgressCtx {
    use_progress().unwrap_or_else(|| {
        panic!("{part} must be used within <Progress> / progress_scope");
    })
}

fn is_empty_el(el: &Element) -> bool {
    el.children.is_empty()
        && el.text.as_ref().is_none_or(|t| t.is_empty())
        && el.template_id.is_none()
}

fn flatten_header(mut header: Element) -> Option<Element> {
    if is_empty_el(&header) {
        return None;
    }
    let is_frag = header.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1");
    if is_frag {
        let kids = std::mem::take(&mut header.children);
        if kids.is_empty() {
            return None;
        }
        if kids.len() == 1 {
            return Some(kids.into_iter().next().unwrap());
        }
        // Label + Value 等：一行两端对齐（对位 Root flex-wrap 首行）。
        return Some(
            hstack()
                .w("100%")
                .items_center()
                .justify("between")
                .gap("0.5rem")
                .children(kids)
                .into(),
        );
    }
    Some(header)
}

/// 提供 Progress 上下文；子树之后自动挂 [`progress_track`]（对位 Root 注入 Track）。
pub fn progress_scope(
    id: impl Into<String>,
    value: Option<Signal<f32>>,
    default_value: f32,
    class: Option<String>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = resolve_instance_id(id, "progress");
    let default_value = default_value.clamp(0.0, 1.0);
    let value = value.unwrap_or_else(|| {
        use_signal(format!("{id}__progress").as_str(), move || default_value)
    });
    let mut root = with_child_owner(|| {
        provide_context(
            progress_context(),
            ProgressCtx {
                value,
                id: id.clone(),
            },
        );
        let header = flatten_header(body());
        let track = progress_track(ProgressTrackOpts {
            class: None,
            value: Some(value),
            id: Some(id.clone()),
        });
        let mut col = vstack().w("100%").gap("0.75rem");
        if let Some(h) = header {
            col = col.child(h);
        }
        col = col.child(track);
        let mut el: Element = col.into();
        if let Some(ref c) = class {
            if !c.trim().is_empty() {
                el = with_class_list(el, c);
            }
        }
        el
    });
    if root.template_id.is_none() {
        root.template_id = Some(format!("{id}__progress"));
    }
    root
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

pub struct ProgressBuilder {
    id: String,
    value: Option<Signal<f32>>,
    default_value: f32,
    class: String,
}

/// Builder 入口：默认无用户 id（WP-2.8）。
pub fn progress() -> ProgressBuilder {
    ProgressBuilder {
        id: String::new(),
        value: None,
        default_value: 0.0,
        class: String::new(),
    }
}

impl ProgressBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn value_signal(mut self, s: Signal<f32>) -> Self {
        self.value = Some(s);
        self
    }

    pub fn default_value(mut self, v: f32) -> Self {
        self.default_value = v.clamp(0.0, 1.0);
        self
    }

    /// Tailwind / utility class（根）。
    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.class = c.into();
        self
    }

    /// 复合面：在 Track 之前渲染 Label / Value 等（内部 → [`progress_scope`]）。
    pub fn child(self, body: impl FnOnce() -> Element + 'static) -> Element {
        let class = if self.class.trim().is_empty() {
            None
        } else {
            Some(self.class)
        };
        progress_scope(self.id, self.value, self.default_value, class, body)
    }
}

impl From<ProgressBuilder> for Element {
    fn from(builder: ProgressBuilder) -> Self {
        let ProgressBuilder {
            id,
            value,
            default_value,
            class,
        } = builder;
        let class = if class.trim().is_empty() {
            None
        } else {
            Some(class)
        };
        progress_scope(id, value, default_value, class, Element::new)
    }
}

/// RSX 备用入口（无宏特殊展开时）；复合面请走宏 → [`progress_scope`]。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Progress(
    id: String,
    value: Option<f32>,
    value_signal: Option<Signal<f32>>,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    let mut b = progress().id(id);
    if let Some(s) = value_signal {
        b = b.value_signal(s);
    } else if let Some(v) = value {
        b = b.default_value(v);
    }
    if let Some(c) = class {
        b = b.class(c);
    }
    if let Some(ch) = children {
        // 无宏时子节点已脱离上下文；仅作占位，复合请用 ui! 宏展开。
        let _ = ch;
    }
    b.into()
}

// ---------------------------------------------------------------------------
// Track / Indicator
// ---------------------------------------------------------------------------

/// `ProgressTrack` 选项。
#[derive(Clone, Default)]
pub struct ProgressTrackOpts {
    pub class: Option<String>,
    /// 显式值信号；`None` 时读 [`ProgressCtx`]。
    pub value: Option<Signal<f32>>,
    pub id: Option<String>,
}

/// ShadCN `cn-progress-track` + 内嵌 Indicator（响应 `value`）。
pub fn progress_track(opts: ProgressTrackOpts) -> Element {
    let value = opts.value.or_else(|| use_progress().map(|c| c.value));
    let Some(val) = value else {
        panic!("ProgressTrack must be used within <Progress> / progress_scope (or pass value)");
    };
    let instance = opts
        .id
        .or_else(|| use_progress().map(|c| c.id))
        .unwrap_or_else(|| "progress".into());
    let class = opts.class.filter(|s| !s.trim().is_empty());
    let view_id = format!("{instance}__progress_track_view");

    reactive_view(view_id, move || {
        let t = val.get().clamp(0.0, 1.0);
        let pct = (t * 100.0).clamp(0.0, 100.0);
        let mut track_bg = tokens::primary();
        track_bg[3] *= 0.2;
        // cn-progress-track: relative flex w-full items-center overflow-x-hidden; h-2 rounded-full
        let mut track: Element = hstack()
            .w("100%")
            .h("0.5rem")
            .rounded_full()
            .overflow("hidden")
            .items_center()
            .into();
        track = track.bg(track_bg);
        // cn-progress-indicator: h-full transition-all；宽按百分比
        let mut fill: Element = r#box()
            .h("100%")
            .w(format!("{pct}%"))
            .rounded_full()
            .flex_shrink(0.0)
            .into();
        fill = fill.bg(tokens::primary());
        track = track.child(fill);
        track.set_attr("rui_progress", format!("{pct:.1}%"));
        track = track.accesskit_role("progressbar");
        if let Some(ref c) = class {
            track = with_class_list(track, c);
        }
        track.template_id = Some(format!("{instance}__track"));
        track
    })
}

/// RSX `<ProgressTrack class=… />`（需在 Progress 内；通常由 Root 自动注入）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ProgressTrack(class: Option<String>, children: Option<Element>) -> Element {
    let _ = children;
    progress_track(ProgressTrackOpts {
        class,
        value: None,
        id: None,
    })
}

/// RSX `<ProgressIndicator />` — Track 已内嵌指示条；本标签为 no-op 占位（对位导出面）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ProgressIndicator(class: Option<String>, children: Option<Element>) -> Element {
    let _ = (class, children);
    Element::new().w(0.0).h(0.0)
}

// ---------------------------------------------------------------------------
// Label / Value
// ---------------------------------------------------------------------------

fn take_packed_children(mut el: Element) -> Vec<Element> {
    let bare = el.text.as_ref().is_none_or(|t| t.is_empty());
    if bare && el.children.len() > 1 {
        std::mem::take(&mut el.children)
    } else {
        vec![el]
    }
}

/// ShadCN `ProgressLabel`（`text-sm` / `font-medium`）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ProgressLabel(children: Option<Element>) -> Element {
    let _ = require_progress("ProgressLabel");
    let fg = parse_text_color("foreground");
    let kids = children.map(take_packed_children).unwrap_or_default();
    if kids.len() == 1 {
        let c = kids.into_iter().next().unwrap();
        if c.children.is_empty() && c.text.as_ref().is_some_and(|t| !t.is_empty()) {
            return text(c.text.clone().unwrap_or_default())
                .size(14.0)
                .weight(500.0)
                .color(fg)
                .into();
        }
        return c.text_size(14.0).font_weight(500.0).text_color(fg);
    }
    Element::from(
        hstack()
            .w("100%")
            .items_center()
            .justify("between")
            .gap("0.5rem")
            .children(
                kids.into_iter()
                    .map(|c| c.text_size(14.0).font_weight(500.0).text_color(fg))
                    .collect::<Vec<_>>(),
            ),
    )
}

/// ShadCN `ProgressValue`：当前百分比（`bind_text` 原地更新）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ProgressValue(children: Option<Element>) -> Element {
    let _ = children;
    let ctx = require_progress("ProgressValue");
    let value = ctx.value;
    let bind_id = format!("{}:progress_value", ctx.id);
    let muted = parse_text_color("muted-foreground");
    bind_text(bind_id, move || {
        let t = value.get().clamp(0.0, 1.0);
        format!("{:.0}%", t * 100.0)
    })
    .text_size(14.0)
    .font_weight(400.0)
    .text_color(muted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::create_root;

    #[test]
    fn progress_empty_id_never_writes_empty_template() {
        let el: Element = progress().default_value(0.25).into();
        let tid = el.template_id.as_deref().unwrap_or("");
        assert_ne!(tid, "");
        assert!(!tid.is_empty() || el.template_id.is_none());
    }

    #[test]
    fn progress_empty_id_resolves_nonempty_instance_attr() {
        let el: Element = progress().into();
        assert_ne!(el.template_id.as_deref(), Some(""));
    }

    #[test]
    fn scope_exposes_template() {
        let (_owner, el): (_, Element) = create_root(|| {
            progress()
                .id("prog-scope")
                .value_signal(use_signal("prog-scope-val", || 0.5f32))
                .into()
        });
        assert_eq!(el.template_id.as_deref(), Some("prog-scope__progress"));
    }

    #[test]
    fn labeled_scope_keeps_context() {
        let (_owner, el): (_, Element) = create_root(|| {
            progress().id("prog-lbl").default_value(0.3).child(|| {
                ProgressLabel(ProgressLabelProps {
                    children: Some(Element::new().text("Load")),
                    base: None,
                })
            })
        });
        assert_eq!(el.template_id.as_deref(), Some("prog-lbl__progress"));
    }
}
