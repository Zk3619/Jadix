//! # 公开 `<scene3d>` 叶（WP-3.13）
//!
//! 实验 feature **`scene3d`**：同一 wgpu device 上的透视立方。默认 **关闭**（体积 / ADR-3）。
//! 无 feature 时本模块仍编译：叶是占位盒，不挂 GPU pass。禁止 glTF 全家桶 / three.js。

use crate::core::tree::{Element, IntoCssDim, IntoElementList};
use crate::style::parse_color;

/// 叶标记：`1` = 启用 GPU 立方（仅 `scene3d` feature）。
pub const ATTR_SCENE3D: &str = "jadix_scene3d";
pub const ATTR_SCENE3D_FOV_DEG: &str = "jadix_scene3d_fov_deg";
pub const ATTR_SCENE3D_YAW_SPEED: &str = "jadix_scene3d_yaw_speed";
pub const ATTR_SCENE3D_PITCH: &str = "jadix_scene3d_pitch";
pub const ATTR_SCENE3D_BG: &str = "jadix_scene3d_bg";
pub const ATTR_SCENE3D_CAMERA_Z: &str = "jadix_scene3d_camera_z";
pub const ATTR_SCENE3D_LIGHT: &str = "jadix_scene3d_light";
pub const ATTR_SCENE3D_MATERIAL: &str = "jadix_scene3d_material";
/// Optional SDF/Bake text label drawn as a perspective billboard in the scene3d RT.
pub const ATTR_SCENE3D_LABEL: &str = "jadix_scene3d_label";

/// Formal scene camera (perspective MVP inputs).
#[derive(Clone, Copy, Debug)]
pub struct Scene3dCamera {
    pub fov_deg: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl Default for Scene3dCamera {
    fn default() -> Self {
        Self {
            fov_deg: 45.0,
            pitch: 0.35,
            distance: 2.35,
        }
    }
}

/// Simple directional + ambient light (shader uniforms later).
#[derive(Clone, Copy, Debug)]
pub struct Scene3dLight {
    pub ambient: f32,
    pub directional: f32,
    pub dir: [f32; 3],
}

impl Default for Scene3dLight {
    fn default() -> Self {
        Self {
            ambient: 0.22,
            directional: 0.78,
            dir: [0.3, 0.8, 0.5],
        }
    }
}

/// Per-mesh material coefficients (metallic / roughness MVP).
#[derive(Clone, Copy, Debug)]
pub struct Scene3dMaterial {
    pub metallic: f32,
    pub roughness: f32,
}

impl Default for Scene3dMaterial {
    fn default() -> Self {
        Self {
            metallic: 0.15,
            roughness: 0.55,
        }
    }
}

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

    pub fn fov_deg(mut self, deg: f32) -> Self {
        self.el
            .set_attr(ATTR_SCENE3D_FOV_DEG, format!("{:.3}", deg.max(1.0)));
        self
    }

    pub fn yaw_speed(mut self, speed: f32) -> Self {
        self.el
            .set_attr(ATTR_SCENE3D_YAW_SPEED, format!("{:.4}", speed));
        self
    }

    pub fn pitch_deg(mut self, deg: f32) -> Self {
        self.el
            .set_attr(ATTR_SCENE3D_PITCH, format!("{:.4}", deg.to_radians()));
        self
    }

    pub fn background(mut self, color: impl AsRef<str>) -> Self {
        let raw = color.as_ref().to_string();
        self.el.set_attr(ATTR_SCENE3D_BG, raw.clone());
        self.el = self.el.bg(parse_color(&raw));
        self
    }

    pub fn camera(mut self, cam: Scene3dCamera) -> Self {
        self.el
            .set_attr(ATTR_SCENE3D_FOV_DEG, format!("{:.3}", cam.fov_deg.max(1.0)));
        self.el
            .set_attr(ATTR_SCENE3D_PITCH, format!("{:.4}", cam.pitch));
        self.el.set_attr(
            ATTR_SCENE3D_CAMERA_Z,
            format!("{:.4}", cam.distance.max(0.1)),
        );
        self
    }

    pub fn light(mut self, light: Scene3dLight) -> Self {
        self.el.set_attr(
            ATTR_SCENE3D_LIGHT,
            format!(
                "{:.3},{:.3},{:.3},{:.3},{:.3}",
                light.ambient, light.directional, light.dir[0], light.dir[1], light.dir[2]
            ),
        );
        self
    }

    pub fn material(mut self, mat: Scene3dMaterial) -> Self {
        self.el.set_attr(
            ATTR_SCENE3D_MATERIAL,
            format!("{:.3},{:.3}", mat.metallic, mat.roughness),
        );
        self
    }

    /// Perspective text billboard (SDF bake via [`crate::renderer::bake_text_texture_key_for_3d`]).
    pub fn label(mut self, text: impl Into<String>) -> Self {
        self.el.set_attr(ATTR_SCENE3D_LABEL, text.into());
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

/// GPU 作业：布局收集排队，渲染期画到 `jadix_scene3d:` 贴图。
#[cfg(feature = "scene3d")]
#[derive(Clone, Debug)]
pub struct Scene3dJob {
    pub key: String,
    pub width: u32,
    pub height: u32,
    pub yaw: f32,
    pub pitch: f32,
    pub fov_deg: f32,
    pub camera_z: f32,
    pub bg: [f32; 4],
    pub light: Scene3dLight,
    pub material: Scene3dMaterial,
    /// Optional `jadix_text_msdf_v2:` / bake key for a floating label billboard.
    pub text_tex_key: Option<String>,
}

#[cfg(feature = "scene3d")]
fn parse_light(attrs: &std::collections::HashMap<String, String>) -> Scene3dLight {
    attrs
        .get(ATTR_SCENE3D_LIGHT)
        .and_then(|s| {
            let p: Vec<f32> = s.split(',').filter_map(|x| x.parse().ok()).collect();
            if p.len() >= 5 {
                Some(Scene3dLight {
                    ambient: p[0],
                    directional: p[1],
                    dir: [p[2], p[3], p[4]],
                })
            } else {
                None
            }
        })
        .unwrap_or_default()
}

#[cfg(feature = "scene3d")]
fn parse_material(attrs: &std::collections::HashMap<String, String>) -> Scene3dMaterial {
    attrs
        .get(ATTR_SCENE3D_MATERIAL)
        .and_then(|s| {
            let p: Vec<f32> = s.split(',').filter_map(|x| x.parse().ok()).collect();
            if p.len() >= 2 {
                Some(Scene3dMaterial {
                    metallic: p[0],
                    roughness: p[1],
                })
            } else {
                None
            }
        })
        .unwrap_or_default()
}

#[cfg(feature = "scene3d")]
thread_local! {
    static JOBS: std::cell::RefCell<Vec<Scene3dJob>> = std::cell::RefCell::new(Vec::new());
}

#[cfg(feature = "scene3d")]
static SCENE3D_EPOCH: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// 布局期：有 `jadix_scene3d` 则排队 GPU 立方并返回贴图键。无 feature / 无标记 → `None`。
#[cfg(not(feature = "scene3d"))]
pub fn scene3d_tex_from_element_attrs(
    attrs: &std::collections::HashMap<String, String>,
    leaf_w: f32,
    leaf_h: f32,
) -> Option<(String, f32, f32)> {
    let _ = (attrs, leaf_w, leaf_h);
    None
}

/// 布局期：有 `jadix_scene3d` 则排队 GPU 立方并返回贴图键。
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
    use std::hash::{Hash, Hasher};
    let mut identity = std::collections::hash_map::DefaultHasher::new();
    for attr in [
        ATTR_SCENE3D_FOV_DEG,
        ATTR_SCENE3D_YAW_SPEED,
        ATTR_SCENE3D_PITCH,
        ATTR_SCENE3D_BG,
        ATTR_SCENE3D_CAMERA_Z,
        ATTR_SCENE3D_LIGHT,
        ATTR_SCENE3D_MATERIAL,
        ATTR_SCENE3D_LABEL,
    ] {
        attrs.get(attr).hash(&mut identity);
    }
    let key = format!("jadix_scene3d:{:016x}:{w}x{h}", identity.finish());
    let yaw_speed = attrs
        .get(ATTR_SCENE3D_YAW_SPEED)
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(0.85);
    let pitch = attrs
        .get(ATTR_SCENE3D_PITCH)
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(0.35 + (t * 0.4).sin() * 0.12);
    let fov_deg = attrs
        .get(ATTR_SCENE3D_FOV_DEG)
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(45.0);
    let bg = attrs
        .get(ATTR_SCENE3D_BG)
        .map(|s| parse_color(s))
        .unwrap_or(parse_color("secondary"));
    let camera_z = attrs
        .get(ATTR_SCENE3D_CAMERA_Z)
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(2.35);
    let light = parse_light(attrs);
    let material = parse_material(attrs);
    let text_tex_key = attrs.get(ATTR_SCENE3D_LABEL).and_then(|label| {
        let label = label.trim();
        if label.is_empty() {
            return None;
        }
        crate::renderer::bake_text_texture_key_for_3d(
            label,
            22.0,
            600,
            [248, 250, 252, 255],
            160.0,
            36.0,
        )
    });
    JOBS.with(|j| {
        j.borrow_mut().push(Scene3dJob {
            key: key.clone(),
            width: w,
            height: h,
            yaw: t * yaw_speed,
            pitch,
            fov_deg,
            camera_z,
            bg,
            light,
            material,
            text_tex_key,
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

#[cfg(feature = "scene3d")]
pub(crate) fn has_scene3d_jobs() -> bool {
    JOBS.with(|j| !j.borrow().is_empty())
}

#[cfg(test)]
#[path = "tests/scene3d.rs"]
mod tests;
