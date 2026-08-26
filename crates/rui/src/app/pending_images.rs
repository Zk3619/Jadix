//! 在任意线程将 RGBA 贴图加入队列，由 [`super::state::AppState::render`] 首帧上传至 GPU。

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use crate::media::{decode_bytes, load_src, DecodeOptions};

static PENDING: Mutex<Vec<(String, Vec<u8>, u32, u32)>> = Mutex::new(Vec::new());
/// Decoded `src` images kept on CPU so TexturePool eviction can rehydrate.
static DECODED_SRC: Mutex<Option<HashMap<String, (Vec<u8>, u32, u32)>>> = Mutex::new(None);
/// Keys referenced by `<Image src>` this frame (collect).
static SRC_NEEDED: Mutex<Option<HashSet<String>>> = Mutex::new(None);
/// 动态贴图内容世代：同一 `texture_key` 像素变了（canvas 重绘 / scene3d 旋转）
/// 也必须让 present 指纹变化，否则 `last_gpu_frame_fp` 会跳过 swapchain。
static DYNAMIC_TEX_EPOCH: AtomicU64 = AtomicU64::new(0);

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

/// 解码任意支持的图像字节并排队上传（上限见 [`DecodeOptions::image`]）。
pub fn queue_image_from_bytes(id: impl Into<String>, bytes: &[u8]) -> bool {
    let Some(img) = decode_bytes(bytes, DecodeOptions::image()) else {
        return false;
    };
    queue_image_rgba8(id, img.width, img.height, img.rgba);
    true
}

/// 从文件路径解码并排队（解析规则与 [`crate::media::resolve_asset_path`] 相同）。
pub fn queue_image_from_path(id: impl Into<String>, path: impl AsRef<str>) -> bool {
    queue_image_from_src(id, path.as_ref())
}

/// 从 `data:` / 本地路径解码并排队。同一 `id` 只解码一次；GPU 驱逐后由
/// [`flush_src_textures`] 从 CPU 缓存回补。禁止 `http(s)`。
pub fn queue_image_from_src(id: impl Into<String>, src: &str) -> bool {
    let id = id.into();
    {
        let mut g = DECODED_SRC.lock().unwrap();
        let map = g.get_or_insert_with(HashMap::new);
        if map.contains_key(&id) {
            SRC_NEEDED
                .lock()
                .unwrap()
                .get_or_insert_with(HashSet::new)
                .insert(id);
            return true;
        }
    }
    let Some(img) = load_src(src, DecodeOptions::image()) else {
        return false;
    };
    {
        let mut g = DECODED_SRC.lock().unwrap();
        let map = g.get_or_insert_with(HashMap::new);
        map.insert(id.clone(), (img.rgba.clone(), img.width, img.height));
    }
    SRC_NEEDED
        .lock()
        .unwrap()
        .get_or_insert_with(HashSet::new)
        .insert(id.clone());
    queue_image_rgba8(id, img.width, img.height, img.rgba);
    true
}

/// Re-queue decoded `src` images missing from the GPU pool.
pub(crate) fn flush_src_textures(mut has_gpu: impl FnMut(&str) -> bool) -> bool {
    let needed: Vec<String> = {
        let mut g = SRC_NEEDED.lock().unwrap();
        g.get_or_insert_with(HashSet::new)
            .drain()
            .collect()
    };
    let mut any = false;
    let decoded = DECODED_SRC.lock().unwrap();
    let Some(map) = decoded.as_ref() else {
        return false;
    };
    for key in needed {
        if has_gpu(&key) {
            continue;
        }
        if let Some((rgba, w, h)) = map.get(&key) {
            queue_image_rgba8(&key, *w, *h, rgba.clone());
            any = true;
        }
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
    fn queue_rgba_bumps_dynamic_texture_epoch() {
        let before = dynamic_texture_epoch();
        queue_image_rgba8("rui-test-tex-epoch", 1, 1, vec![0, 0, 0, 255]);
        assert!(
            dynamic_texture_epoch() > before,
            "same texture key with new pixels must change present fingerprint"
        );
    }
}
