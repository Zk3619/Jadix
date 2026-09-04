//! 在任意线程将 RGBA 贴图加入队列，由 [`super::state::AppState::render`] 首帧上传至 GPU。
//!
//! [`SrcLoadState`] 供 [`crate::components::AvatarImage`] 等在解码失败或 `src` 为空时回退到
//! [`crate::components::AvatarFallback`]。

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use crate::core::reactivity::Signal;
use crate::media::{decode_bytes, load_src, DecodeOptions, DecodedAnimation, DecodedImage};

static PENDING: Mutex<Vec<(String, Vec<u8>, u32, u32)>> = Mutex::new(Vec::new());
/// Decoded `src` images kept on CPU so TexturePool eviction can rehydrate.
static DECODED_SRC: Mutex<Option<HashMap<String, CachedSrcImage>>> = Mutex::new(None);
/// Keys referenced by `<Image src>` this frame (collect).
static SRC_NEEDED: Mutex<Option<HashSet<String>>> = Mutex::new(None);
/// Per-贴图键解码结果（Avatar 失败回退 / 避免重复解码）。
static SRC_LOAD_STATE: Mutex<Option<HashMap<String, SrcLoadState>>> = Mutex::new(None);
// Signal ids live in the current runtime. A process-global list would `set`
// handles from other test threads / disposed roots.
thread_local! {
    static SRC_LOAD_LISTENERS: RefCell<HashMap<String, Vec<Signal<SrcLoadState>>>> =
        RefCell::new(HashMap::new());
}
/// GIF 播放状态（帧索引 + 累积时间）。
static GIF_PLAY: Mutex<Option<HashMap<String, GifPlayState>>> = Mutex::new(None);
/// 动态贴图内容世代：同一 `texture_key` 像素变了（canvas 重绘 / scene3d 旋转 / GIF 帧）
/// 也必须让 present 指纹变化，否则 `last_gpu_frame_fp` 会跳过 swapchain。
static DYNAMIC_TEX_EPOCH: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
struct CachedSrcImage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    animation: Option<DecodedAnimation>,
}

struct GifPlayState {
    frame: usize,
    elapsed_ms: f32,
}

/// `<Image src>` / [`AvatarImage`] 解码状态（对位 Radix `onLoadingStatusChange`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SrcLoadState {
    #[default]
    Unknown,
    /// 已登记、尚未得出结果（预留异步解码）。
    Pending,
    Ready,
    Failed,
}

impl SrcLoadState {
    pub fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }
}

fn src_load_map() -> std::sync::MutexGuard<'static, Option<HashMap<String, SrcLoadState>>> {
    SRC_LOAD_STATE.lock().unwrap()
}

fn publish_src_load_state(key: &str, state: SrcLoadState) {
    {
        let mut g = src_load_map();
        let map = g.get_or_insert_with(HashMap::new);
        if map.get(key) == Some(&state) {
            return;
        }
        map.insert(key.to_string(), state);
    }
    SRC_LOAD_LISTENERS.with(|cell| {
        let mut map = cell.borrow_mut();
        let empty = if let Some(sigs) = map.get_mut(key) {
            sigs.retain(|sig| sig.is_alive());
            for sig in sigs.iter() {
                let _ = sig.try_set(state);
            }
            sigs.is_empty()
        } else {
            false
        };
        if empty {
            map.remove(key);
        }
    });
}

fn cached_src_load_state(key: &str) -> Option<SrcLoadState> {
    src_load_map().as_ref().and_then(|m| m.get(key).copied())
}

fn mark_src_needed(key: impl Into<String>) {
    let key = key.into();
    SRC_NEEDED
        .lock()
        .unwrap()
        .get_or_insert_with(HashSet::new)
        .insert(key);
}

/// 读取贴图键对应解码状态（缺省 `Unknown`）。
pub fn src_load_state_for_key(key: &str) -> SrcLoadState {
    cached_src_load_state(key).unwrap_or(SrcLoadState::Unknown)
}

/// 读取 `src` 对应解码状态。
pub fn src_load_state_for_src(src: &str) -> SrcLoadState {
    if src.trim().is_empty() {
        return SrcLoadState::Failed;
    }
    src_load_state_for_key(&texture_key_for_src(src))
}

/// 注册 Signal，在 [`publish_src_load_state`] 时同步更新（供 Avatar bind 使用）。
pub(crate) fn track_src_load_signal(key: &str, sig: Signal<SrcLoadState>) {
    SRC_LOAD_LISTENERS.with(|cell| {
        let mut map = cell.borrow_mut();
        let list = map.entry(key.to_string()).or_default();
        list.retain(|s| s.is_alive());
        list.push(sig);
    });
}

/// 确保 `src` 已尝试解码并返回最终状态；成功时排队 GPU 上传。
pub fn ensure_src_loaded(key: &str, src: &str) -> SrcLoadState {
    if src.trim().is_empty() {
        publish_src_load_state(key, SrcLoadState::Failed);
        return SrcLoadState::Failed;
    }
    if let Some(state) = cached_src_load_state(key) {
        if state.is_ready() {
            mark_src_needed(key);
        }
        return state;
    }
    publish_src_load_state(key, SrcLoadState::Pending);
    let ok = decode_and_queue_src(key, src);
    let state = if ok {
        SrcLoadState::Ready
    } else {
        SrcLoadState::Failed
    };
    publish_src_load_state(key, state);
    state
}

fn decode_and_queue_src(key: &str, src: &str) -> bool {
    {
        let mut g = DECODED_SRC.lock().unwrap();
        let map = g.get_or_insert_with(HashMap::new);
        if map.contains_key(key) {
            mark_src_needed(key);
            return true;
        }
    }
    let Some(img) = load_src(src, DecodeOptions::image()) else {
        return false;
    };
    cache_decoded(key.to_string(), &img);
    mark_src_needed(key);
    queue_decoded(key, &img);
    true
}

/// Avatar 贴图 alpha 遮罩（与 [`AvatarShape`] 对位）。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AvatarImageMask {
    #[default]
    Circle,
    RoundedSquare,
}

impl AvatarImageMask {
    pub fn as_key(self) -> &'static str {
        match self {
            Self::Circle => "circle",
            Self::RoundedSquare => "rounded",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "rounded" | "rounded-square" | "square" | "rounded_square" => Self::RoundedSquare,
            _ => Self::Circle,
        }
    }
}

fn rounded_rect_coverage(px: f32, py: f32, w: f32, h: f32, radii: [f32; 4]) -> f32 {
    let r_tl = radii[0].min(w * 0.5).min(h * 0.5).max(0.0);
    let r_tr = radii[1].min(w * 0.5).min(h * 0.5).max(0.0);
    let r_br = radii[2].min(w * 0.5).min(h * 0.5).max(0.0);
    let r_bl = radii[3].min(w * 0.5).min(h * 0.5).max(0.0);

    let in_core = px >= r_tl.max(r_bl)
        && px <= w - r_tr.max(r_br)
        && py >= r_tl.max(r_tr)
        && py <= h - r_bl.max(r_br);
    if in_core {
        return 1.0;
    }

    let (cx, cy, r) = if px < r_tl && py < r_tl {
        (r_tl, r_tl, r_tl)
    } else if px > w - r_tr && py < r_tr {
        (w - r_tr, r_tr, r_tr)
    } else if px > w - r_br && py > h - r_br {
        (w - r_br, h - r_br, r_br)
    } else if px < r_bl && py > h - r_bl {
        (r_bl, h - r_bl, r_bl)
    } else if px < 0.0 || py < 0.0 || px > w || py > h {
        return 0.0;
    } else {
        return 1.0;
    };
    if r <= 0.0 {
        return 1.0;
    }
    let d = (px - cx).hypot(py - cy);
    (r + 0.5 - d).clamp(0.0, 1.0)
}

fn apply_shape_alpha_mask(rgba: &mut [u8], width: u32, height: u32, mask: AvatarImageMask) {
    if width == 0 || height == 0 {
        return;
    }
    let w = width as f32;
    let h = height as f32;
    let aa = 1.0_f32;
    for y in 0..height {
        for x in 0..width {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let coverage = match mask {
                AvatarImageMask::Circle => {
                    let cx = w * 0.5;
                    let cy = h * 0.5;
                    let radius = cx.min(cy);
                    let dx = px - cx;
                    let dy = py - cy;
                    let dist = (dx * dx + dy * dy).sqrt();
                    if dist <= radius - aa {
                        1.0
                    } else if dist >= radius {
                        0.0
                    } else {
                        (radius - dist) / aa
                    }
                }
                AvatarImageMask::RoundedSquare => {
                    // ~`rounded-md` on square avatar (6px / 32px side).
                    let r = w.min(h) * 0.1875;
                    rounded_rect_coverage(px, py, w, h, [r, r, r, r])
                }
            };
            let cov = coverage.clamp(0.0, 1.0);
            let i = ((y * width + x) * 4) as usize;
            let src_a = rgba[i + 3] as f32 / 255.0;
            let a = src_a * cov;
            for c in 0..3 {
                rgba[i + c] = (rgba[i + c] as f32 * cov).round() as u8;
            }
            rgba[i + 3] = (a * 255.0).round() as u8;
        }
    }
}

/// 对 RGBA 贴图施加圆形 alpha 遮罩（Avatar `object-cover` 正方形裁切 + 圆角）。
#[cfg(test)]
fn apply_circular_alpha_mask(rgba: &mut [u8], width: u32, height: u32) {
    apply_shape_alpha_mask(rgba, width, height, AvatarImageMask::Circle);
}

fn decode_and_queue_avatar_src(key: &str, src: &str, mask: AvatarImageMask) -> bool {
    {
        let g = DECODED_SRC.lock().unwrap();
        if let Some(map) = g.as_ref() {
            if map.contains_key(key) {
                mark_src_needed(key);
                return true;
            }
        }
    }
    let Some(mut img) = load_src(src, DecodeOptions::image()) else {
        return false;
    };
    apply_shape_alpha_mask(&mut img.rgba, img.width, img.height, mask);
    cache_decoded(key.to_string(), &img);
    mark_src_needed(key);
    queue_decoded(key, &img);
    true
}

/// Avatar 专用贴图键（形状 alpha 遮罩；与 [`texture_key_for_src`] 分离）。
pub fn texture_key_for_avatar_src(src: &str, mask: AvatarImageMask) -> String {
    format!("rui-avatar-src:{}:{src}", mask.as_key())
}

/// Avatar 图片解码：失败回退 + 形状遮罩贴图。
pub fn ensure_avatar_src_loaded(key: &str, src: &str, mask: AvatarImageMask) -> SrcLoadState {
    if src.trim().is_empty() {
        publish_src_load_state(key, SrcLoadState::Failed);
        return SrcLoadState::Failed;
    }
    if let Some(state) = cached_src_load_state(key) {
        if state.is_ready() {
            mark_src_needed(key);
        }
        return state;
    }
    publish_src_load_state(key, SrcLoadState::Pending);
    let ok = decode_and_queue_avatar_src(key, src, mask);
    let state = if ok {
        SrcLoadState::Ready
    } else {
        SrcLoadState::Failed
    };
    publish_src_load_state(key, state);
    state
}

pub(crate) fn bump_dynamic_texture_epoch() {
    DYNAMIC_TEX_EPOCH.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn dynamic_texture_epoch() -> u64 {
    DYNAMIC_TEX_EPOCH.load(Ordering::Relaxed)
}

/// 排队上传一张 RGBA8 贴图（`width * height * 4` 字节）。同一 `id` 重复排队时仅保留最后一次。
pub fn queue_image_rgba8(id: impl Into<String>, width: u32, height: u32, rgba: Vec<u8>) {
    let id = id.into();
    let mut g = PENDING.lock().unwrap();
    g.retain(|(k, _, _, _)| k != &id);
    g.push((id, rgba, width, height));
    bump_dynamic_texture_epoch();
}

fn cache_decoded(id: String, img: &DecodedImage) {
    let mut g = DECODED_SRC.lock().unwrap();
    let map = g.get_or_insert_with(HashMap::new);
    map.insert(
        id.clone(),
        CachedSrcImage {
            width: img.width,
            height: img.height,
            rgba: img.rgba.clone(),
            animation: img.animation.clone(),
        },
    );
    if img.is_animated() {
        GIF_PLAY
            .lock()
            .unwrap()
            .get_or_insert_with(HashMap::new)
            .entry(id)
            .or_insert(GifPlayState {
                frame: 0,
                elapsed_ms: 0.0,
            });
    }
}

fn queue_decoded(id: impl Into<String>, img: &DecodedImage) {
    queue_image_rgba8(id, img.width, img.height, img.rgba.clone());
}

/// Decode any supported image bytes and queue them for upload.
pub fn queue_image_from_bytes(id: impl Into<String>, bytes: &[u8]) -> bool {
    let Some(img) = decode_bytes(bytes, DecodeOptions::image()) else {
        return false;
    };
    let id = id.into();
    cache_decoded(id.clone(), &img);
    queue_decoded(id, &img);
    true
}

/// 从文件路径解码并排队（解析规则与 [`crate::media::resolve_asset_path`] 相同）。
pub fn queue_image_from_path(id: impl Into<String>, path: impl AsRef<str>) -> bool {
    queue_image_from_src(id, path.as_ref())
}

/// Decode and queue from `data:` / local path. The same `id` is decoded only once; after GPU eviction it is
/// [`flush_src_textures`] 从 CPU 缓存回补。禁止 `http(s)`。
pub fn queue_image_from_src(id: impl Into<String>, src: &str) -> bool {
    let id = id.into();
    ensure_src_loaded(&id, src).is_ready()
}

/// Advance all active GIF texture frames; returns `true` when there is a frame switch (redraw on request).
pub fn tick_gif_textures(dt: f32) -> bool {
    if dt <= 0.0 {
        return false;
    }
    let mut play = GIF_PLAY.lock().unwrap();
    let play = play.get_or_insert_with(HashMap::new);
    if play.is_empty() {
        return false;
    }
    let decoded = DECODED_SRC.lock().unwrap();
    let Some(map) = decoded.as_ref() else {
        return false;
    };
    let mut any = false;
    for (key, state) in play.iter_mut() {
        let Some(cached) = map.get(key) else {
            continue;
        };
        let Some(anim) = cached.animation.as_ref() else {
            continue;
        };
        if !anim.is_animated() {
            continue;
        }
        state.elapsed_ms += dt * 1000.0;
        let delay = anim.frame_delay_ms(state.frame) as f32;
        if state.elapsed_ms < delay {
            continue;
        }
        state.elapsed_ms -= delay;
        state.frame = (state.frame + 1) % anim.frames.len();
        let frame = &anim.frames[state.frame];
        queue_image_rgba8(key.clone(), anim.width, anim.height, frame.rgba.clone());
        any = true;
    }
    any
}

/// Is there a GIF playing (for frame pumping).
pub fn any_gif_playing() -> bool {
    GIF_PLAY
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|m| !m.is_empty())
}

/// Re-queue decoded `src` images missing from the GPU pool.
pub(crate) fn flush_src_textures(mut has_gpu: impl FnMut(&str) -> bool) -> bool {
    let needed: Vec<String> = {
        let mut g = SRC_NEEDED.lock().unwrap();
        g.get_or_insert_with(HashSet::new).drain().collect()
    };
    let mut any = false;
    let decoded = DECODED_SRC.lock().unwrap();
    let mut play_guard = GIF_PLAY.lock().unwrap();
    let play = play_guard.get_or_insert_with(HashMap::new);
    let Some(map) = decoded.as_ref() else {
        return false;
    };
    for key in needed {
        if has_gpu(&key) {
            continue;
        }
        let Some(cached) = map.get(&key) else {
            continue;
        };
        if let Some(anim) = cached.animation.as_ref() {
            if anim.is_animated() {
                let frame_idx = play.get(&key).map(|s| s.frame).unwrap_or(0);
                if let Some(frame) = anim.frames.get(frame_idx) {
                    queue_image_rgba8(&key, anim.width, anim.height, frame.rgba.clone());
                    any = true;
                    continue;
                }
            }
        }
        queue_image_rgba8(&key, cached.width, cached.height, cached.rgba.clone());
        any = true;
    }
    any
}

/// `<Image src>` 使用的稳定贴图键。
pub fn texture_key_for_src(src: &str) -> String {
    format!("rui-src:{src}")
}

pub(crate) fn take_pending() -> Vec<(String, Vec<u8>, u32, u32)> {
    let mut g = PENDING.lock().unwrap();
    std::mem::take(&mut *g)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circular_alpha_mask_zeros_outside_radius() {
        let mut rgba = vec![0u8; 4 * 8 * 8];
        for px in rgba.chunks_exact_mut(4) {
            px[0..3].fill(255);
            px[3] = 255;
        }
        apply_circular_alpha_mask(&mut rgba, 8, 8);
        assert_eq!(rgba[(4 * 4 + 4) * 4 + 3], 255, "center pixel stays opaque");
        assert_eq!(rgba[3], 0, "corner pixel is transparent");
    }

    #[test]
    fn ensure_avatar_src_loaded_marks_failed_for_missing_file() {
        let key = "rui-test-avatar-src-fail";
        let state = ensure_avatar_src_loaded(
            key,
            "/nonexistent/rui-avatar-fail-test.png",
            AvatarImageMask::Circle,
        );
        assert_eq!(state, SrcLoadState::Failed);
    }

    #[test]
    fn ensure_src_loaded_marks_failed_for_missing_file() {
        let key = "rui-test-src-load-fail";
        let state = ensure_src_loaded(key, "/nonexistent/rui-avatar-fail-test.png");
        assert_eq!(state, SrcLoadState::Failed);
        assert_eq!(src_load_state_for_key(key), SrcLoadState::Failed);
        assert!(!queue_image_from_src(
            key,
            "/nonexistent/rui-avatar-fail-test.png"
        ));
    }

    #[test]
    fn ensure_src_loaded_empty_src_is_failed() {
        let state = ensure_src_loaded("rui-test-src-empty", "");
        assert_eq!(state, SrcLoadState::Failed);
    }

    #[test]
    fn queue_rgba_bumps_dynamic_texture_epoch() {
        let before = dynamic_texture_epoch();
        queue_image_rgba8("rui-test-tex-epoch", 1, 1, vec![0, 0, 0, 255]);
        assert!(
            dynamic_texture_epoch() > before,
            "same texture key with new pixels must change present fingerprint"
        );
    }
}
