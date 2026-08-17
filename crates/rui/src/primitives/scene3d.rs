//! # 公开 `<scene3d>` 叶（WP-3.13）
//!
//! 实验 feature **`scene3d`**：同一 wgpu device 上的透视立方。默认 **关闭**（体积 / ADR-3）。
//! 无 feature 时本模块仍编译：叶是占位盒，不挂 GPU pass。禁止 glTF 全家桶 / three.js。

use crate::core::tree::{Element, IntoCssDim, IntoElementList};
use crate::style::parse_color;

/// 叶标记：`1` = 启用 GPU 立方（仅 `scene3d` feature）。
pub const ATTR_SCENE3D: &str = "rui_scene3d";

pub struct Scene3dBuilder {
    el: Element,
    has_w: bool,
    has_h: bool,
}

pub fn scene3d() -> Scene3dBuilder {
    Scene3dBuilder {
        el: Element::new(),
        has_w: false,
        has_h: false,
    }
}

impl Scene3dBuilder {
    pub fn w(mut self, w: impl IntoCssDim) -> Self {
        self.has_w = true;
        self.el = self.el.w(w);
        self
    }

    pub fn h(mut self, h: impl IntoCssDim) -> Self {
        self.has_h = true;
        self.el = self.el.h(h);
        self
    }

    pub fn child(self, _child: impl Into<Element>) -> Self {
        self
    }

    pub fn children(self, _children: impl IntoIterator<Item = Element>) -> Self {
        self
    }
}

impl From<Scene3dBuilder> for Element {
    fn from(builder: Scene3dBuilder) -> Self {
        let Scene3dBuilder {
            mut el,
            has_w,
            has_h,
        } = builder;
        el.children.clear();
        if !has_w {
            el = el.w(220.0);
        }
        if !has_h {
            el = el.h(160.0);
        }
        el = el.bg(parse_color("secondary"));
        #[cfg(feature = "scene3d")]
        {
            el.set_attr(ATTR_SCENE3D, "1");
        }
        el
    }
}

impl IntoElementList for Scene3dBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

/// GPU 作业：布局收集排队，渲染期画到 `rui_scene3d:` 贴图。
#[cfg(feature = "scene3d")]
#[derive(Clone, Debug)]
pub struct Scene3dJob {
    pub key: String,
    pub width: u32,
    pub height: u32,
    pub yaw: f32,
    pub pitch: f32,
}

#[cfg(feature = "scene3d")]
thread_local! {
    static JOBS: std::cell::RefCell<Vec<Scene3dJob>> = std::cell::RefCell::new(Vec::new());
}

#[cfg(feature = "scene3d")]
static SCENE3D_EPOCH: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// 布局期：有 `rui_scene3d` 则排队 GPU 立方并返回贴图键。无 feature / 无标记 → `None`。
#[cfg(not(feature = "scene3d"))]
pub fn scene3d_tex_from_element_attrs(
    attrs: &std::collections::HashMap<String, String>,
    leaf_w: f32,
    leaf_h: f32,
) -> Option<(String, f32, f32)> {
    let _ = (attrs, leaf_w, leaf_h);
    None
}

/// 布局期：有 `rui_scene3d` 则排队 GPU 立方并返回贴图键。
#[cfg(feature = "scene3d")]
pub fn scene3d_tex_from_element_attrs(
    attrs: &std::collections::HashMap<String, String>,
    leaf_w: f32,
    leaf_h: f32,
) -> Option<(String, f32, f32)> {
    if attrs.get(ATTR_SCENE3D).map(|s| s.trim()) != Some("1") {
        return None;
    }
    let w = leaf_w.round() as i32;
    let h = leaf_h.round() as i32;
    if w < 2 || h < 2 {
        return None;
    }
    let w = w as u32;
    let h = h as u32;
    if w.saturating_mul(h) > 1024 * 1024 {
        return None;
    }
    let t0 = *SCENE3D_EPOCH.get_or_init(std::time::Instant::now);
    let t = t0.elapsed().as_secs_f32();
    let key = format!("rui_scene3d:{w}x{h}");
    JOBS.with(|j| {
        j.borrow_mut().push(Scene3dJob {
            key: key.clone(),
            width: w,
            height: h,
            yaw: t * 0.85,
            pitch: 0.35 + (t * 0.4).sin() * 0.12,
        });
    });
    // Do not call `runtime::request_redraw()` here: collect runs while `AppState` is
    // already `borrow_mut()`. That panics (`RefCell already mutably borrowed`); on
    // Windows GLES the unwind then blows up in WGL `make_current` during Drop.
    // Continuous rotation is pumped after GPU flush via `window.request_redraw()`.
    Some((key, w as f32, h as f32))
}

#[cfg(feature = "scene3d")]
pub(crate) fn take_scene3d_jobs() -> Vec<Scene3dJob> {
    JOBS.with(|j| std::mem::take(&mut *j.borrow_mut()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::layout_tree::LayoutTree;

    #[test]
    fn scene3d_leaf_layout_bounds() {
        let leaf: Element = scene3d().w(120.0).h(90.0).into();
        assert!(leaf.children.is_empty());
        let mut root = Element::new().w(200.0).h(140.0).child(leaf);
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 200.0, 140.0, [-1.0, -1.0]);
        let [_, _, w, h] = root.children.first().expect("leaf").layout_bounds;
        assert!(
            w > 110.0 && w < 130.0 && h > 80.0 && h < 100.0,
            "scene3d leaf bounds [{w}, {h}]"
        );
        #[cfg(feature = "scene3d")]
        {
            assert_eq!(
                root.children[0].get_attr(ATTR_SCENE3D).map(|s| s.as_str()),
                Some("1")
            );
            let jobs = take_scene3d_jobs();
            assert!(
                !jobs.is_empty(),
                "layout collect must queue a GPU job without calling runtime::request_redraw"
            );
        }
        #[cfg(not(feature = "scene3d"))]
        {
            assert!(root.children[0].get_attr(ATTR_SCENE3D).is_none());
        }
    }

    /// Layout collect used to call `runtime::request_redraw` while `AppState` was already
    /// `borrow_mut` → panic at `runtime.rs` + GLES WGL Drop on Windows.
    #[cfg(feature = "scene3d")]
    #[test]
    fn scene3d_collect_does_not_reenter_appstate_borrow() {
        let mut attrs = std::collections::HashMap::new();
        attrs.insert(ATTR_SCENE3D.to_string(), "1".into());
        crate::app::runtime::call_while_app_state_mutably_borrowed(|| {
            assert!(scene3d_tex_from_element_attrs(&attrs, 64.0, 48.0).is_some());
        });
        assert!(!take_scene3d_jobs().is_empty());
    }
}
