//! Load `i18n.toml` / `locales/<tag>.toml` (same TOML-only policy as `theme.toml`).

use std::fs;
use std::path::{Path, PathBuf};

use super::catalog::MessageBundle;
use super::locale_id::Locale;
use super::runtime;

const META_KEYS: &[&str] = &["locale", "fallback", "dir", "timezone"];

/// Load an i18n config or a single locale file.
///
/// **Config file** (`i18n.toml`):
/// ```toml
/// locale = "zh-CN"
/// fallback = "en"
/// dir = "locales"          # optional: load `locales/<tag>.toml`
///
/// [rui.pagination]         # optional inline messages for `locale`
/// previous = "上一页"
/// ```
///
/// **Locale file** (`locales/zh-CN.toml`): message tables only; the file stem is the tag
/// unless `locale =` is set.
pub fn load(path: impl AsRef<Path>) -> Result<(), String> {
    let path = path.as_ref();
    let raw = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    apply_toml(path, &raw)
}

/// Parse TOML already in memory (`include_str!`). `origin` is used to resolve `dir`.
pub fn load_str(origin: impl AsRef<Path>, raw: &str) -> Result<(), String> {
    apply_toml(origin.as_ref(), raw)
}

/// Load every `*.toml` in `dir`; file stem is the locale tag (`zh-CN.toml` → `zh-CN`).
pub fn load_dir(dir: impl AsRef<Path>) -> Result<(), String> {
    let dir = dir.as_ref();
    let entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let raw = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let tag = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("{}: missing file stem", path.display()))?;
        add_messages_toml(tag, &raw)?;
    }
    Ok(())
}

fn apply_toml(origin: &Path, raw: &str) -> Result<(), String> {
    let value: toml::Value = toml::from_str(raw).map_err(|e| e.to_string())?;
    let table = value
        .as_table()
        .ok_or_else(|| "i18n root must be a TOML table".to_string())?;

    let locale_key = table
        .get("locale")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let fallback_key = table
        .get("fallback")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let dir_key = table
        .get("dir")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let timezone_key = table
        .get("timezone")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    if let Some(ref fb) = fallback_key {
        runtime::set_fallback_inner(Locale::from(fb.as_str()));
    }

    let inferred = origin
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|stem| !stem.eq_ignore_ascii_case("i18n"));
    let message_locale = locale_key
        .clone()
        .or_else(|| inferred.map(|s| s.to_string()))
        .unwrap_or_else(|| "en".into());

    let mut bundle = MessageBundle::new(message_locale.as_str());
    flatten_skip_meta("", &value, &mut bundle);
    if !bundle.is_empty() {
        runtime::add_bundle_inner(bundle);
    }

    if let Some(dir) = dir_key {
        let base = resolve_dir(origin, &dir);
        load_dir(&base)?;
        if origin.is_file() {
            let extras = dir_toml_paths(&base);
            super::watch::watch_i18n_root(origin, &extras);
        }
    } else if origin.is_file() {
        super::watch::watch_i18n_root(origin, &[]);
    }

    if let Some(tz) = timezone_key {
        runtime::set_timezone_inner(Some(tz));
    }

    if let Some(loc) = locale_key {
        runtime::set_global_locale_inner(Locale::from(loc.as_str()));
    }
    Ok(())
}

pub(super) fn reload(path: &Path) -> Result<(), String> {
    load(path)
}

fn dir_toml_paths(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            out.push(path);
        }
    }
    out
}

fn resolve_dir(origin: &Path, dir: &str) -> PathBuf {
    let p = Path::new(dir);
    if p.is_absolute() {
        p.to_path_buf()
    } else if let Some(parent) = origin.parent() {
        parent.join(p)
    } else {
        p.to_path_buf()
    }
}

fn add_messages_toml(locale: &str, raw: &str) -> Result<(), String> {
    let value: toml::Value = toml::from_str(raw).map_err(|e| e.to_string())?;
    let mut bundle = MessageBundle::new(locale);
    flatten_skip_meta("", &value, &mut bundle);
    if !bundle.is_empty() {
        runtime::add_bundle_inner(bundle);
    }
    Ok(())
}

fn flatten_skip_meta(prefix: &str, value: &toml::Value, bundle: &mut MessageBundle) {
    match value {
        toml::Value::Table(map) => {
            for (k, v) in map {
                if prefix.is_empty() && META_KEYS.contains(&k.as_str()) {
                    continue;
                }
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten_skip_meta(&key, v, bundle);
            }
        }
        toml::Value::String(s) => {
            if !prefix.is_empty() {
                bundle.insert(prefix, s.clone());
            }
        }
        toml::Value::Integer(n) => bundle.insert(prefix, n.to_string()),
        toml::Value::Float(n) => bundle.insert(prefix, n.to_string()),
        toml::Value::Boolean(b) => bundle.insert(prefix, b.to_string()),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{register, set_locale, tr};

    #[test]
    fn config_sets_locale_and_messages() {
        crate::i18n::with_lock(|| {
            register("rui.pagination.previous", "Previous");
            load_str(
                "i18n.toml",
                r#"
locale = "zh-CN"
[rui.pagination]
previous = "上一页"
"#,
            )
            .unwrap();
            assert_eq!(crate::i18n::locale().language(), "zh");
            assert_eq!(tr("rui.pagination.previous"), "上一页");
        });
    }

    #[test]
    fn missing_overlay_falls_back_to_register() {
        crate::i18n::with_lock(|| {
            register("rui.pagination.next", "Next");
            set_locale("zh-CN");
            assert_eq!(tr("rui.pagination.next"), "Next");
        });
    }
}
