//! 在任意线程将 RGBA 贴图加入队列，由 [`super::state::AppState::render`] 首帧上传至 GPU。

use std::collections::HashSet;
use std::sync::Mutex;

use crate::media::{decode_bytes, load_src, DecodeOptions};

static PENDING: Mutex<Vec<(String, Vec<u8>, u32, u32)>> = Mutex::new(Vec::new());
static QUEUED_SRC: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// 排队上传一张 RGBA8 贴图（`width * height * 4` 字节）。同一 `id` 重复排队时仅保留最后一次。
pub fn queue_image_rgba8(id: impl Into<String>, width: u32, height: u32, rgba: Vec<u8>) {
    let id = id.into();
    let mut g = PENDING.lock().unwrap();
    g.retain(|(k, _, _, _)| k != &id);
    g.push((id, rgba, width, height));
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

/// 从 `data:` / 本地路径解码并排队。同一 `id` 只解码一次。禁止 `http(s)`。
pub fn queue_image_from_src(id: impl Into<String>, src: &str) -> bool {
    let id = id.into();
    {
        let mut g = QUEUED_SRC.lock().unwrap();
        let set = g.get_or_insert_with(HashSet::new);
        if set.contains(&id) {
            return true;
        }
        set.insert(id.clone());
    }
    let Some(img) = load_src(src, DecodeOptions::image()) else {
        QUEUED_SRC.lock().unwrap().as_mut().unwrap().remove(&id);
        return false;
    };
    queue_image_rgba8(id, img.width, img.height, img.rgba);
    true
}

/// `<Image src>` 使用的稳定贴图键。
pub fn texture_key_for_src(src: &str) -> String {
    format!("rui-src:{src}")
}

pub(crate) fn take_pending() -> Vec<(String, Vec<u8>, u32, u32)> {
    let mut g = PENDING.lock().unwrap();
    std::mem::take(&mut *g)
}
