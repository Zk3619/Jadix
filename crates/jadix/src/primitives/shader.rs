//! # 公开 `<shader>` 叶（experimental）
//!
//! 用户 WGSL **fragment** 画到离屏 RT，再当 `jadix_shader:` 贴图合成。默认构建可编译
//!（GLES 一等），**不是** Cargo feature。失败保留上帧或品红，禁止 panic。
//! `.texture("src", key)` / `ui!` `texture={…}` 把 compositor 槽绑到 `t_src` / `s_src`；
//! `.texture("src1", key)` / `ui!` `src1={…}` 绑 `t_src1`；`src2` 绑 `t_src2`（缺键 1×1 白）。
//! `.preset("blur")` 用具名 EffectKind FS；`.buffer(true)` 把上帧绑到空槽
//!（不要绑本叶正在画的 `jadix_shader:`）。双书写：[`shader`] Builder；
//! `ui!` `<shader wgsl={…}>`（见 `jadix_macros`）。

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::core::tree::{Element, IntoCssDim, IntoElementList};
use crate::style::parse_color;

/// 叶上登记 WGSL 源的 attr（hex id）。
pub const ATTR_SHADER: &str = "jadix_shader";
pub const ATTR_SHADER_TIME: &str = "jadix_shader_time";
pub const ATTR_SHADER_POINTER: &str = "jadix_shader_pointer";
pub const ATTR_SHADER_CUSTOM: &str = "jadix_shader_custom";
/// Optional sampled texture key (`jadix_path:` / `jadix_canvas:` / `jadix_capture:` / media).
pub const ATTR_SHADER_TEX: &str = "jadix_shader_tex";
/// Slot 1 sampled texture (`t_src1` / `s_src1`).
pub const ATTR_SHADER_TEX1: &str = "jadix_shader_tex1";
/// Slot 2 sampled texture (`t_src2` / `s_src2`).
pub const ATTR_SHADER_TEX2: &str = "jadix_shader_tex2";
/// `1` = ping-pong Buffer: previous frame bound to an empty src slot.
pub const ATTR_SHADER_BUFFER: &str = "jadix_shader_buffer";

const PRESET_FS: &str = include_str!("../renderer/shader_preset.wgsl");

const MAX_SHADER_PX: u32 = 1024 * 1024;
const DEFAULT_W: f32 = 400.0;
const DEFAULT_H: f32 = 300.0;

/// `.uniform` 值：具名 `time` / `pointer` 走固定槽，其余依次填 `custom[16]`。
#[derive(Clone, Copy, Debug)]
pub enum ShaderUniform {
    F32(f32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
}

impl From<f32> for ShaderUniform {
    fn from(v: f32) -> Self {
        Self::F32(v)
    }
}

impl From<[f32; 2]> for ShaderUniform {
    fn from(v: [f32; 2]) -> Self {
        Self::Vec2(v)
    }
}

impl From<[f32; 3]> for ShaderUniform {
    fn from(v: [f32; 3]) -> Self {
        Self::Vec3(v)
    }
}

impl From<[f32; 4]> for ShaderUniform {
    fn from(v: [f32; 4]) -> Self {
        Self::Vec4(v)
    }
}

pub struct ShaderBuilder {
    el: Element,
    has_w: bool,
    has_h: bool,
    wgsl: Option<String>,
    time: Option<f32>,
    pointer: Option<[f32; 2]>,
    custom: [f32; 16],
    custom_len: usize,
    tex: Option<String>,
    tex1: Option<String>,
    tex2: Option<String>,
    buffer: bool,
}

pub fn shader() -> ShaderBuilder {
    ShaderBuilder {
        el: Element::new(),
        has_w: false,
        has_h: false,
        wgsl: None,
        time: None,
        pointer: None,
        custom: [0.0; 16],
        custom_len: 0,
        tex: None,
        tex1: None,
        tex2: None,
        buffer: false,
    }
}

impl ShaderBuilder {
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

    /// User fragment. Must define `@fragment fn fs_main(in: VertexOutput) -> @location(0) vec4<f32>`.
    pub fn wgsl(mut self, src: impl Into<String>) -> Self {
        self.wgsl = Some(src.into());
        self
    }

    pub fn uniform(mut self, name: impl AsRef<str>, value: impl Into<ShaderUniform>) -> Self {
        let name = name.as_ref().trim().to_ascii_lowercase();
        let value = value.into();
        match name.as_str() {
            "time" => {
                if let ShaderUniform::F32(t) = value {
                    self.time = Some(t);
                }
            }
            "pointer" => match value {
                ShaderUniform::Vec2(p) => self.pointer = Some(p),
                ShaderUniform::F32(x) => self.pointer = Some([x, 0.0]),
                _ => {}
            },
            _ => self.push_custom(value),
        }
        self
    }

    /// Sample a compositor texture in `fs_main`.
    ///
    /// `name`: `"src"` / `"src0"` / empty → slot 0 (`t_src`). `"src1"` / `"1"` / `"buf"` →
    /// slot 1 (`t_src1`). `"src2"` / `"2"` → slot 2 (`t_src2`). Keys: `jadix_path:` /
    /// `jadix_canvas:` / `jadix_capture:` / `queue_media_frame` slots. Missing key → 1×1 white.
    /// Do not pass this leaf's own `jadix_shader:` key — use [`.buffer(true)`](Self::buffer)
    /// for feedback.
    pub fn texture(mut self, name: impl AsRef<str>, key: impl Into<String>) -> Self {
        let key = key.into();
        match shader_tex_slot(name.as_ref()) {
            1 => self.tex1 = Some(key),
            2 => self.tex2 = Some(key),
            _ => self.tex = Some(key),
        }
        self
    }

    /// Named EffectKind as a `shader()` preset (same pipeline cache as user WGSL).
    /// `custom[0]` = mode, `custom[1]` = strength (default 0.45). Empty `.wgsl` uses the builtin FS.
    pub fn preset(mut self, name: impl AsRef<str>) -> Self {
        let kind = crate::renderer::effect::EffectKind::parse(name.as_ref());
        self.custom[0] = kind.as_mode() as f32;
        if self.custom_len < 2 {
            self.custom[1] = 0.45;
            self.custom_len = 2;
        }
        if self.wgsl.as_ref().is_none_or(|s| s.trim().is_empty()) {
            self.wgsl = Some(PRESET_FS.to_string());
        }
        self
    }

    /// Ping-pong this leaf's RT. Previous frame fills the first empty src slot
    /// (`t_src` / `t_src1` / `t_src2` in order).
    pub fn buffer(mut self, on: bool) -> Self {
        self.buffer = on;
        self
    }

    pub fn opacity(mut self, o: f32) -> Self {
        self.el = self.el.opacity(o);
        self
    }

    pub fn child(self, _child: impl Into<Element>) -> Self {
        self
    }

    pub fn children(self, _children: impl IntoIterator<Item = Element>) -> Self {
        self
    }

    fn push_custom(&mut self, value: ShaderUniform) {
        let vals: &[f32] = match &value {
            ShaderUniform::F32(v) => std::slice::from_ref(v),
            ShaderUniform::Vec2(v) => v,
            ShaderUniform::Vec3(v) => v,
            ShaderUniform::Vec4(v) => v,
        };
        for &v in vals {
            if self.custom_len >= 16 {
                break;
            }
            self.custom[self.custom_len] = v;
            self.custom_len += 1;
        }
    }
}

impl From<ShaderBuilder> for Element {
    fn from(builder: ShaderBuilder) -> Self {
        let ShaderBuilder {
            mut el,
            has_w,
            has_h,
            wgsl,
            time,
            pointer,
            custom,
            custom_len: _,
            tex,
            tex1,
            tex2,
            buffer,
        } = builder;
        el.children.clear();
        if !has_w {
            el = el.w(DEFAULT_W);
        }
        if !has_h {
            el = el.h(DEFAULT_H);
        }
        el = el.bg(parse_color("secondary"));
        let source = wgsl.unwrap_or_default();
        let id = register_source(source);
        el.set_attr(ATTR_SHADER, format!("{id:x}"));
        if let Some(t) = time {
            el.set_attr(ATTR_SHADER_TIME, format!("{t}"));
        }
        if let Some([x, y]) = pointer {
            el.set_attr(ATTR_SHADER_POINTER, format!("{x},{y}"));
        }
        if custom.iter().any(|v| *v != 0.0) {
            let joined = custom
                .iter()
                .map(|v| format!("{v}"))
                .collect::<Vec<_>>()
                .join(",");
            el.set_attr(ATTR_SHADER_CUSTOM, joined);
        }
        if let Some(key) = tex {
            if !key.trim().is_empty() {
                el.set_attr(ATTR_SHADER_TEX, key);
            }
        }
        if let Some(key) = tex1 {
            if !key.trim().is_empty() {
                el.set_attr(ATTR_SHADER_TEX1, key);
            }
        }
        if let Some(key) = tex2 {
            if !key.trim().is_empty() {
                el.set_attr(ATTR_SHADER_TEX2, key);
            }
        }
        if buffer {
            el.set_attr(ATTR_SHADER_BUFFER, "1");
        }
        el.on_unmount(move || unregister_source(id))
    }
}

impl IntoElementList for ShaderBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

/// GPU 作业：布局收集排队，渲染期画到 `jadix_shader:` 贴图。
#[derive(Clone, Debug)]
pub struct ShaderJob {
    pub key: String,
    pub width: u32,
    pub height: u32,
    pub source: Arc<str>,
    pub time: Option<f32>,
    pub pointer: Option<[f32; 2]>,
    pub custom: [f32; 16],
    pub tex_key: Option<String>,
    pub tex_key1: Option<String>,
    pub tex_key2: Option<String>,
    pub buffer: bool,
}

thread_local! {
    static SOURCES: RefCell<HashMap<u64, Arc<str>>> = RefCell::new(HashMap::new());
    static JOBS: RefCell<Vec<ShaderJob>> = const { RefCell::new(Vec::new()) };
}

static NEXT_SHADER_ID: AtomicU64 = AtomicU64::new(1);

fn register_source(src: String) -> u64 {
    let id = NEXT_SHADER_ID.fetch_add(1, Ordering::Relaxed);
    SOURCES.with(|s| {
        s.borrow_mut().insert(id, Arc::<str>::from(src));
    });
    id
}

fn unregister_source(id: u64) {
    SOURCES.with(|s| {
        s.borrow_mut().remove(&id);
    });
}

/// Sources owned by mounted leaves, including leaves below the viewport.
pub(crate) fn mounted_shader_sources() -> Vec<Arc<str>> {
    SOURCES.with(|sources| sources.borrow().values().cloned().collect())
}

/// 布局期：有 `jadix_shader` 则排队 GPU 叶并返回贴图键。
pub fn shader_tex_from_element_attrs(
    attrs: &std::collections::HashMap<String, String>,
    leaf_w: f32,
    leaf_h: f32,
) -> Option<(String, f32, f32)> {
    let id = u64::from_str_radix(attrs.get(ATTR_SHADER)?.trim(), 16).ok()?;
    let w = leaf_w.round() as i32;
    let h = leaf_h.round() as i32;
    if w < 2 || h < 2 {
        return None;
    }
    let w = w as u32;
    let h = h as u32;
    if w.saturating_mul(h) > MAX_SHADER_PX {
        return None;
    }
    let source = SOURCES.with(|s| s.borrow().get(&id).cloned())?;
    let time = attrs
        .get(ATTR_SHADER_TIME)
        .and_then(|s| s.parse::<f32>().ok());
    let pointer = attrs.get(ATTR_SHADER_POINTER).and_then(|s| {
        let p: Vec<f32> = s.split(',').filter_map(|x| x.parse().ok()).collect();
        if p.len() >= 2 {
            Some([p[0], p[1]])
        } else {
            None
        }
    });
    let mut custom = [0.0f32; 16];
    if let Some(raw) = attrs.get(ATTR_SHADER_CUSTOM) {
        for (i, v) in raw
            .split(',')
            .filter_map(|x| x.parse::<f32>().ok())
            .take(16)
            .enumerate()
        {
            custom[i] = v;
        }
    }
    let tex_key = attrs
        .get(ATTR_SHADER_TEX)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let tex_key1 = attrs
        .get(ATTR_SHADER_TEX1)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let tex_key2 = attrs
        .get(ATTR_SHADER_TEX2)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let buffer = matches!(
        attrs.get(ATTR_SHADER_BUFFER).map(|s| s.trim()),
        Some("1" | "true" | "yes")
    );
    let key = format!("jadix_shader:{id:016x}");
    JOBS.with(|j| {
        j.borrow_mut().push(ShaderJob {
            key: key.clone(),
            width: w,
            height: h,
            source,
            time,
            pointer,
            custom,
            tex_key,
            tex_key1,
            tex_key2,
            buffer,
        });
    });
    // Do not call `runtime::request_redraw()` here: collect runs while `AppState` is
    // already `borrow_mut()`. Continuous time is pumped after GPU flush via
    // `window.request_redraw()`.
    Some((key, w as f32, h as f32))
}

pub(crate) fn take_shader_jobs() -> Vec<ShaderJob> {
    JOBS.with(|j| std::mem::take(&mut *j.borrow_mut()))
}

pub(crate) fn has_shader_jobs() -> bool {
    JOBS.with(|j| !j.borrow().is_empty())
}

pub(crate) fn enrich_shader_graph(graph: &mut crate::renderer::RenderGraph) {
    JOBS.with(|j| {
        for job in j.borrow().iter() {
            let mut node = crate::renderer::RenderPassNode::new(
                crate::renderer::PassKind::Shader,
                format!("shader-{}", job.key),
            )
            .write(job.key.clone());
            if let Some(k) = &job.tex_key {
                if k != &job.key {
                    node = node.read(k.clone());
                }
            }
            if let Some(k) = &job.tex_key1 {
                if k != &job.key {
                    node = node.read(k.clone());
                }
            }
            if let Some(k) = &job.tex_key2 {
                if k != &job.key {
                    node = node.read(k.clone());
                }
            }
            let ping = job.tex_key.is_none() || job.tex_key1.is_none() || job.tex_key2.is_none();
            if job.buffer && ping {
                node = node.read(format!("{}:buf", job.key));
            }
            graph.push(node);
        }
    });
}

fn shader_tex_slot(name: &str) -> u32 {
    match name.trim().to_ascii_lowercase().as_str() {
        "src1" | "1" | "buf" | "ichannel1" => 1,
        "src2" | "2" | "ichannel2" => 2,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::layout_tree::LayoutTree;

    const OK_FS: &str = r#"
        @fragment
        fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
            return vec4<f32>(in.uv, 0.2, 1.0);
        }
    "#;

    #[test]
    fn shader_leaf_layout_bounds() {
        let leaf: Element = shader().w(120.0).h(90.0).wgsl(OK_FS).into();
        assert!(leaf.children.is_empty());
        assert!(leaf.get_attr(ATTR_SHADER).is_some());
        let mut root = Element::new().w(200.0).h(140.0).child(leaf);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 200.0, 140.0, [-1.0, -1.0]);
        let [_, _, w, h] = root.children.first().expect("leaf").layout_bounds;
        assert!(
            w > 110.0 && w < 130.0 && h > 80.0 && h < 100.0,
            "shader leaf bounds [{w}, {h}]"
        );
        let jobs = take_shader_jobs();
        assert!(
            !jobs.is_empty(),
            "layout collect must queue a GPU job without calling runtime::request_redraw"
        );
        assert!(result
            .textured_draws
            .iter()
            .any(|d| d.texture_key.starts_with("jadix_shader:")));
    }

    #[test]
    fn shader_resize_queues_new_extent() {
        let mut attrs = std::collections::HashMap::new();
        let id = register_source(OK_FS.into());
        attrs.insert(ATTR_SHADER.to_string(), format!("{id:x}"));
        let (k0, w0, h0) = shader_tex_from_element_attrs(&attrs, 64.0, 48.0).expect("64x48");
        let jobs0 = take_shader_jobs();
        assert_eq!(jobs0[0].width, 64);
        assert_eq!(jobs0[0].height, 48);
        let (k1, w1, h1) = shader_tex_from_element_attrs(&attrs, 128.0, 96.0).expect("128x96");
        let jobs1 = take_shader_jobs();
        assert_eq!(k0, k1, "texture key is id-stable across resize");
        assert_eq!((w1, h1), (128.0, 96.0));
        assert_ne!((w0, h0), (w1, h1));
        assert_eq!(jobs1[0].width, 128);
        unregister_source(id);
    }

    #[test]
    fn shader_collect_does_not_reenter_appstate_borrow() {
        let mut attrs = std::collections::HashMap::new();
        let id = register_source(OK_FS.into());
        attrs.insert(ATTR_SHADER.to_string(), format!("{id:x}"));
        crate::app::runtime::call_while_app_state_mutably_borrowed(|| {
            assert!(shader_tex_from_element_attrs(&attrs, 64.0, 48.0).is_some());
        });
        assert!(!take_shader_jobs().is_empty());
        unregister_source(id);
    }

    #[test]
    fn shader_hit_covers_whole_leaf() {
        let leaf: Element = shader().w(80.0).h(60.0).wgsl(OK_FS).into();
        let mut root = Element::new().w(200.0).h(140.0).child(leaf);
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 200.0, 140.0, [-1.0, -1.0]);
        let [x, y, w, h] = root.children[0].layout_bounds;
        assert!(w >= 79.0 && h >= 59.0, "leaf must occupy its box");
        let cx = x + w * 0.5;
        let cy = y + h * 0.5;
        assert!(cx >= x && cx <= x + w && cy >= y && cy <= y + h);
        let _ = take_shader_jobs();
    }

    #[test]
    fn shader_texture_queues_src_key() {
        let _ = take_shader_jobs();
        let leaf: Element = shader()
            .w(40.0)
            .h(24.0)
            .wgsl(OK_FS)
            .texture("src", "jadix_canvas:deadbeef")
            .into();
        assert_eq!(
            leaf.get_attr(ATTR_SHADER_TEX).map(|s| s.as_str()),
            Some("jadix_canvas:deadbeef")
        );
        shader_tex_from_element_attrs(&leaf.attrs, 40.0, 24.0).expect("job");
        let mut graph = crate::renderer::RenderGraph::default();
        enrich_shader_graph(&mut graph);
        assert!(
            graph
                .nodes
                .iter()
                .any(|n| n.reads.iter().any(|k| k == "jadix_canvas:deadbeef")),
            "graph must read the sampled compositor key before ShaderGpu flush"
        );
        let jobs = take_shader_jobs();
        assert_eq!(
            jobs[0].tex_key.as_deref(),
            Some("jadix_canvas:deadbeef"),
            "layout collect must pass the sampled key to ShaderGpu"
        );
    }

    #[test]
    fn shader_preset_blur_sets_builtin_and_mode() {
        let _ = take_shader_jobs();
        let leaf: Element = shader().w(32.0).h(24.0).preset("blur").into();
        assert!(leaf.get_attr(ATTR_SHADER).is_some());
        shader_tex_from_element_attrs(&leaf.attrs, 32.0, 24.0).expect("job");
        let jobs = take_shader_jobs();
        assert!(
            jobs[0].source.contains("blur9") || jobs[0].custom[0] > 1.5,
            "preset blur must ship builtin FS, custom={:?}",
            jobs[0].custom
        );
        assert!((jobs[0].custom[0] - 2.0).abs() < 0.1, "blur mode=2");
        assert!(jobs[0].custom[1] > 0.2, "default strength");
    }

    #[test]
    fn shader_buffer_queues_feedback_read() {
        let _ = take_shader_jobs();
        let leaf: Element = shader().w(16.0).h(12.0).wgsl(OK_FS).buffer(true).into();
        assert_eq!(
            leaf.get_attr(ATTR_SHADER_BUFFER).map(String::as_str),
            Some("1")
        );
        let (key, _, _) = shader_tex_from_element_attrs(&leaf.attrs, 16.0, 12.0).expect("job");
        let mut graph = crate::renderer::RenderGraph::default();
        enrich_shader_graph(&mut graph);
        let buf = format!("{key}:buf");
        assert!(
            graph
                .nodes
                .iter()
                .any(|n| n.reads.iter().any(|k| k == &buf)),
            "buffer leaf must read previous ping texture"
        );
        let jobs = take_shader_jobs();
        assert!(jobs[0].buffer);
    }

    #[test]
    fn shader_texture_src1_queues_slot1() {
        let _ = take_shader_jobs();
        let leaf: Element = shader()
            .w(40.0)
            .h(24.0)
            .wgsl(OK_FS)
            .texture("src1", "jadix_path:slot1")
            .into();
        assert_eq!(
            leaf.get_attr(ATTR_SHADER_TEX1).map(|s| s.as_str()),
            Some("jadix_path:slot1")
        );
        assert!(leaf.get_attr(ATTR_SHADER_TEX).is_none());
        shader_tex_from_element_attrs(&leaf.attrs, 40.0, 24.0).expect("job");
        let mut graph = crate::renderer::RenderGraph::default();
        enrich_shader_graph(&mut graph);
        assert!(
            graph
                .nodes
                .iter()
                .any(|n| n.reads.iter().any(|k| k == "jadix_path:slot1")),
            "graph must read the slot-1 compositor key"
        );
        let jobs = take_shader_jobs();
        assert_eq!(jobs[0].tex_key.as_deref(), None);
        assert_eq!(jobs[0].tex_key1.as_deref(), Some("jadix_path:slot1"));
    }

    #[test]
    fn shader_texture_src2_queues_slot2() {
        let _ = take_shader_jobs();
        let leaf: Element = shader()
            .w(40.0)
            .h(24.0)
            .wgsl(OK_FS)
            .texture("src2", "jadix_path:slot2")
            .into();
        assert_eq!(
            leaf.get_attr(ATTR_SHADER_TEX2).map(|s| s.as_str()),
            Some("jadix_path:slot2")
        );
        assert!(leaf.get_attr(ATTR_SHADER_TEX).is_none());
        shader_tex_from_element_attrs(&leaf.attrs, 40.0, 24.0).expect("job");
        let mut graph = crate::renderer::RenderGraph::default();
        enrich_shader_graph(&mut graph);
        assert!(
            graph
                .nodes
                .iter()
                .any(|n| n.reads.iter().any(|k| k == "jadix_path:slot2")),
            "graph must read the slot-2 compositor key"
        );
        let jobs = take_shader_jobs();
        assert_eq!(jobs[0].tex_key2.as_deref(), Some("jadix_path:slot2"));
    }

    #[test]
    fn shader_buffer_with_src_reads_ping_on_slot1() {
        let _ = take_shader_jobs();
        let leaf: Element = shader()
            .w(16.0)
            .h(12.0)
            .wgsl(OK_FS)
            .texture("src", "jadix_canvas:a")
            .buffer(true)
            .into();
        let (key, _, _) = shader_tex_from_element_attrs(&leaf.attrs, 16.0, 12.0).expect("job");
        let mut graph = crate::renderer::RenderGraph::default();
        enrich_shader_graph(&mut graph);
        let buf = format!("{key}:buf");
        assert!(
            graph.nodes.iter().any(|n| {
                n.reads.iter().any(|k| k == "jadix_canvas:a") && n.reads.iter().any(|k| k == &buf)
            }),
            "user src + buffer must read compositor and previous ping"
        );
        let jobs = take_shader_jobs();
        assert_eq!(jobs[0].tex_key.as_deref(), Some("jadix_canvas:a"));
        assert!(jobs[0].buffer);
        assert!(jobs[0].tex_key1.is_none());
    }
}
