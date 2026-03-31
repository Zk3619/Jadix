//! `RUI_TEXT_DEBUG`：观察每段文本的 **请求** 族名/字重/行高与 shaping 后 **fontdb 实际用到的字面**（多 fallback 时会有多个族名）。

use std::collections::HashSet;
use std::sync::OnceLock;
use std::sync::Mutex;

use glyphon::{Buffer, Family, FontSystem, Shaping};

use crate::core::tree::TextLayoutInfo;
use crate::style::tailwind_default_leading_scale;

enum TextDebugFilter {
    All,
    InputOnly,
    IdContains(String),
}

static FILTER: OnceLock<Option<TextDebugFilter>> = OnceLock::new();
static RESHAPE_ONLY: OnceLock<bool> = OnceLock::new();
static WARN_MISMATCH: OnceLock<bool> = OnceLock::new();
static WARNED_MISMATCH_KEYS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn filter() -> &'static Option<TextDebugFilter> {
    FILTER.get_or_init(|| {
        let Ok(raw) = std::env::var("RUI_TEXT_DEBUG") else {
            return None;
        };
        let t = raw.trim();
        if t.is_empty() {
            return None;
        }
        let lower = t.to_ascii_lowercase();
        if matches!(lower.as_str(), "1" | "true" | "yes" | "all") {
            return Some(TextDebugFilter::All);
        }
        if lower == "input" {
            return Some(TextDebugFilter::InputOnly);
        }
        Some(TextDebugFilter::IdContains(lower))
    })
}

pub(crate) fn enabled() -> bool {
    filter().is_some()
}

/// 为 `1` / `true` / `yes` 时：仅在本次 `prepare` 重新跑过 `shape`（`cache_hit == false`）时打印，避免每帧刷屏。
pub(crate) fn reshape_only() -> bool {
    *RESHAPE_ONLY.get_or_init(|| {
        std::env::var("RUI_TEXT_DEBUG_RESHAPE_ONLY")
            .map(|v| {
                matches!(
                    v.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes"
                )
            })
            .unwrap_or(false)
    })
}

/// Input 内联文本（失焦时无 `input_caret`，但仍是输入框里的字）。
pub(crate) fn is_input_inline_text(template_id: Option<&str>) -> bool {
    template_id.is_some_and(|t| t.starts_with("__rui_input:") && t.ends_with(":text"))
}

pub(crate) fn should_log(template_id: Option<&str>, caret_active: bool) -> bool {
    let input_related = caret_active || is_input_inline_text(template_id);
    match filter().as_ref() {
        None => false,
        Some(TextDebugFilter::All) => true,
        Some(TextDebugFilter::InputOnly) => input_related,
        Some(TextDebugFilter::IdContains(sub)) => template_id
            .map(|id| id.to_ascii_lowercase().contains(sub.as_str()))
            .unwrap_or(false),
    }
}

fn family_display(family: Family<'_>) -> String {
    match family {
        Family::Name(n) => n.to_string(),
        Family::Serif => "serif".into(),
        Family::SansSerif => "sans-serif".into(),
        Family::Monospace => "monospace".into(),
        Family::Cursive => "cursive".into(),
        Family::Fantasy => "fantasy".into(),
    }
}

fn face_primary_name(db: &glyphon::fontdb::Database, font_id: glyphon::fontdb::ID) -> String {
    db.face(font_id).map_or_else(
        || "?face".into(),
        |face| {
            face.families
                .first()
                .map(|(n, _)| n.clone())
                .unwrap_or_else(|| "?".into())
        },
    )
}

/// 与 shaping 缓冲一致：按 **glyph 遍历顺序** 收集字面族名（含拉丁/薄空格等）。
fn resolved_face_families(buffer: &Buffer, font_system: &FontSystem) -> String {
    let db = font_system.db();
    let mut seen = HashSet::new();
    let mut names = Vec::new();
    for run in buffer.layout_runs() {
        for g in run.glyphs.iter() {
            if seen.insert(g.font_id) {
                names.push(face_primary_name(db, g.font_id));
            }
        }
    }
    names.join(" | ")
}

fn resolved_face_families_vec(buffer: &Buffer, font_system: &FontSystem) -> Vec<String> {
    let db = font_system.db();
    let mut seen = HashSet::new();
    let mut names = Vec::new();
    for run in buffer.layout_runs() {
        for g in run.glyphs.iter() {
            if seen.insert(g.font_id) {
                names.push(face_primary_name(db, g.font_id));
            }
        }
    }
    names
}

fn warn_mismatch_enabled() -> bool {
    *WARN_MISMATCH.get_or_init(|| {
        std::env::var("RUI_TEXT_WARN_MISMATCH")
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false)
    })
}

/// 运行时字体家族不匹配告警：
/// - 请求家族（`req`）与 shaping 后实际字面族名（`resolved`）不一致时打印 warn。
/// - 默认关闭，设置 `RUI_TEXT_WARN_MISMATCH=1` 启用。
/// - 同一组 `(tid, req, resolved)` 仅打印一次，避免每帧刷屏。
pub(crate) fn maybe_log_family_mismatch(
    info: &TextLayoutInfo,
    attrs_family: Family<'_>,
    weight: u16,
    buffer: &Buffer,
    font_system: &FontSystem,
) {
    if !warn_mismatch_enabled() {
        return;
    }

    let req = info
        .style
        .font_family
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| match attrs_family {
            Family::Name(n) => n,
            Family::SansSerif => "sans-serif",
            Family::Serif => "serif",
            Family::Monospace => "monospace",
            Family::Cursive => "cursive",
            Family::Fantasy => "fantasy",
        });

    let resolved = resolved_face_families_vec(buffer, font_system);
    if resolved.is_empty() {
        return;
    }

    let req_l = req.to_ascii_lowercase();
    let matched = resolved
        .iter()
        .any(|n| n.to_ascii_lowercase() == req_l);
    if matched {
        return;
    }

    let tid = info.template_id.as_deref().unwrap_or("-");
    let resolved_join = resolved.join(" | ");
    let key = format!("{tid}|{req}|{resolved_join}");
    let warned = WARNED_MISMATCH_KEYS
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .ok()
        .map(|mut set| !set.insert(key))
        .unwrap_or(false);
    if warned {
        return;
    }

    let preview: String = info
        .text
        .chars()
        .take(48)
        .collect::<String>()
        .replace('\n', "↵");
    log::warn!(
        "text-family-mismatch tid={tid} req={req} w={} attrs={} resolved=[{}] {:?}",
        weight,
        family_display(attrs_family),
        resolved_join,
        preview,
    );
}

/// 仅统计 **簇内至少有一个 CJK/全角等相关码位** 的 glyph 所用字面，便于与 `=> […]` 全量列表对照
/// （Input 的 `shape_suffix` 薄空格会进全量列表但不进此项）。
fn is_cjk_related_char(c: char) -> bool {
    matches!(
        c as u32,
        0x1100..=0x11FF | // Hangul Jamo
        0x2E80..=0x2EFF | // CJK Radicals Supplement …
        0x2F00..=0x2FDF |
        0x3000..=0x303F | // CJK symbols & punctuation（、：等）
        0x3040..=0x309F | // Hiragana
        0x30A0..=0x30FF | // Katakana
        0x3100..=0x312F |
        0x3130..=0x318F |
        0x3190..=0x319F |
        0x31F0..=0x31FF |
        0x3200..=0x32FF |
        0x3300..=0x33FF |
        0x3400..=0x4DBF | // Ext A
        0x4DC0..=0x4DFF |
        0x4E00..=0x9FFF | // CJK Unified
        0xA960..=0xA97F |
        0xAC00..=0xD7AF | // Hangul syllables
        0xF900..=0xFAFF |
        0xFE10..=0xFE1F |
        0xFE30..=0xFE4F |
        0xFF00..=0xFFEF // 全角形式
    )
}

fn cluster_touches_cjk(text: &str, start: usize, end: usize) -> bool {
    text.get(start..end)
        .unwrap_or("")
        .chars()
        .any(is_cjk_related_char)
}

fn resolved_face_families_cjk_only(buffer: &Buffer, font_system: &FontSystem) -> String {
    let db = font_system.db();
    let mut seen = HashSet::new();
    let mut names = Vec::new();
    for run in buffer.layout_runs() {
        for g in run.glyphs.iter() {
            if !cluster_touches_cjk(run.text, g.start, g.end) {
                continue;
            }
            if seen.insert(g.font_id) {
                names.push(face_primary_name(db, g.font_id));
            }
        }
    }
    if names.is_empty() {
        "-".into()
    } else {
        names.join(" | ")
    }
}

fn shape_suffix_codes(info: &TextLayoutInfo) -> String {
    match info.shape_suffix.as_deref() {
        None | Some("") => "-".into(),
        Some("\u{2009}") => "U+2009".into(),
        Some(s) => {
            let c = s.chars().next().unwrap_or('\0');
            format!("U+{:X}", c as u32)
        }
    }
}

fn shaping_tag(shaping: Shaping) -> &'static str {
    match shaping {
        Shaping::Advanced => "adv",
        _ => "basic",
    }
}

pub(crate) fn log_line(
    idx: usize,
    info: &TextLayoutInfo,
    attrs_family: Family<'_>,
    weight: u16,
    line_height_logical: f32,
    caret_active: bool,
    buffer: &Buffer,
    font_system: &FontSystem,
    cache_hit: bool,
    shaping: Shaping,
) {
    let tid = info.template_id.as_deref().unwrap_or("-");
    let shaping_family = family_display(attrs_family);
    let req_family = info
        .style
        .font_family
        .as_deref()
        .unwrap_or(shaping_family.as_str());
    let lh_scale = info.style.line_height_px.map(|_| "px".to_string()).unwrap_or_else(|| {
        info.style
            .line_height_scale
            .map(|s| format!("{s:.4}"))
            .unwrap_or_else(|| {
                let tw = tailwind_default_leading_scale(info.style.size);
                format!("tw-v4≈{tw:.4}")
            })
    });
    let preview: String = info
        .text
        .chars()
        .take(48)
        .collect::<String>()
        .replace('\n', "↵");
    let suffix = if info.shape_suffix.is_some() {
        "+shape_suffix"
    } else {
        ""
    };
    let resolved = resolved_face_families(buffer, font_system);
    let resolved_cjk = resolved_face_families_cjk_only(buffer, font_system);
    let hit = if cache_hit { "reuse" } else { "reshape" };
    let inline = is_input_inline_text(info.template_id.as_deref());
    let kind = if caret_active || inline {
        "input"
    } else {
        "text"
    };
    let caret = if caret_active || inline {
        if caret_active {
            "caret=y"
        } else {
            "caret=n"
        }
    } else {
        ""
    };
    let caret_sep = if caret.is_empty() { "" } else { " " };
    let shape_extra = shape_suffix_codes(info);
    let sh = shaping_tag(shaping);
    // `=>` 全量字面顺序随 glyph 遍历；`cjk=>` 只看 CJK 相关簇，便于对比 Input(400) 与 Label(500) 的中文 fallback
    log::warn!(
        "i={idx} tid={tid} kind={kind}{suffix}{caret_sep}{caret} shape={sh} extra={shape_extra} w={} sz={:.1} lh={:.1} lh_src={} hit={hit} req={} attrs={} => [{}] cjk=>[{}] {:?}",
        weight,
        info.style.size,
        line_height_logical,
        lh_scale,
        req_family,
        shaping_family,
        resolved,
        resolved_cjk,
        preview,
    );
}
