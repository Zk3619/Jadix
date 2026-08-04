//! # NavigationHost
//!
//! 渲染 [`crate::app::navigator::Navigator`] 页面栈：
//! - Host：内容区叠放；埋层 `pointer_events_none`
//! - Sheet：portal + underlay（点击 pop）
//! - 根节点挂 [`Navigator::unmount_hook`]，从树移除时自动 dispose
//!
//! 须包在 `<Reactive>` 内以订阅栈 / 转场 progress。

use std::hash::Hash;

use crate::app::navigator::{
    host_z, sheet_z, Frame, FrameState, Navigator, Presentation, Transition,
};
use crate::core::tree::Element;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{r#box, vstack};
use crate::utils::popup::{panel_block_clicks, portal_panel};

pub struct NavigationHostBuilder<R, F>
where
    R: Clone + Eq + Hash + 'static,
    F: Fn(&R) -> Element + 'static,
{
    nav: Navigator<R>,
    render: F,
}

pub fn navigation_host<R, F>(nav: Navigator<R>, render: F) -> NavigationHostBuilder<R, F>
where
    R: Clone + Eq + Hash + 'static,
    F: Fn(&R) -> Element + 'static,
{
    NavigationHostBuilder { nav, render }
}

fn slide_offset_px(frame: &Frame<impl Clone>) -> f32 {
    match frame.transition {
        Transition::None => 0.0,
        Transition::Slide { .. } => match frame.state {
            FrameState::Entering => (1.0 - frame.progress) * 28.0,
            FrameState::Exiting => (1.0 - frame.progress) * 28.0,
            FrameState::Active => 0.0,
        },
    }
}

impl<R, F> From<NavigationHostBuilder<R, F>> for Element
where
    R: Clone + Eq + Hash + 'static,
    F: Fn(&R) -> Element + 'static,
{
    fn from(builder: NavigationHostBuilder<R, F>) -> Self {
        let nav = builder.nav;
        nav.register_active();
        let frames = nav.stack();
        // 顶层：最后一个非 Exiting（Exiting 仍绘制在上层直到结束）
        let top_live = frames
            .iter()
            .rposition(|f| f.state != FrameState::Exiting)
            .unwrap_or(0);

        let mut host_layer: Element = vstack().w("100%").h("100%").into();
        host_layer = host_layer.relative();

        let mut sheet_nodes: Vec<Element> = Vec::new();

        for (i, frame) in frames.iter().enumerate() {
            let is_top = i == top_live && frame.state != FrameState::Exiting
                || frame.state == FrameState::Exiting && i == frames.len() - 1;
            // keep_alive：非顶且非动画中的底层可跳过构建
            let visible = frame.keep_alive
                || is_top
                || matches!(frame.state, FrameState::Entering | FrameState::Exiting);
            if !visible {
                continue;
            }
            let mut page = (builder.render)(&frame.route);
            page.template_id = Some(format!("nav:{}:{}:{}", nav.host_id(), frame.key, i));

            let ox = slide_offset_px(frame);

            match frame.presentation {
                Presentation::Host => {
                    let z = host_z(i);
                    let mut wrap: Element = r#box().w("100%").h("100%").into();
                    wrap = wrap.absolute().inset(0.0);
                    if ox.abs() > 0.05 {
                        wrap = wrap.left(ox);
                    }
                    wrap.style.z_index = z;
                    wrap = wrap.child(page);
                    let buried = i < top_live && frame.state == FrameState::Active;
                    if buried {
                        wrap = wrap.pointer_events_none();
                        wrap.set_attr("rui_nav_buried", "1");
                    }
                    host_layer = host_layer.child(wrap);
                }
                Presentation::Sheet => {
                    let z = sheet_z(i);
                    let nav_u = nav.clone();
                    let mut underlay: Element = interactive(r#box().into())
                        .on_click(move || {
                            let _ = nav_u.pop();
                        })
                        .into();
                    underlay = underlay.absolute().inset(0.0).w("100%").h("100%");
                    underlay = underlay.bg([0.0, 0.0, 0.0, 0.35 * frame.progress.max(0.15)]);
                    underlay = portal_panel(underlay, z);

                    let mut panel = page;
                    if ox.abs() > 0.05 {
                        panel = panel.left(ox);
                    }
                    let panel = panel_block_clicks(panel, z + 1);
                    sheet_nodes.push(underlay);
                    sheet_nodes.push(panel);
                }
            }
        }

        let mut root: Element = vstack().w("100%").h("100%").into();
        root = root.relative();
        root = root.child(host_layer);
        for s in sheet_nodes {
            root = root.child(s);
        }
        root.template_id = Some(format!("navhost:{}", nav.host_id()));
        root = root.on_unmount_hook(nav.unmount_hook());
        root
    }
}

pub type NavigationHostFrame<R> = Frame<R>;
