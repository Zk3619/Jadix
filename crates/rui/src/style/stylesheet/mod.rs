//! Global stylesheet loader (utility distributor). Not a CSSOM.
//!
//! External source is **TOML only**. Phase 2: [`apply_stylesheet_rules`] matches
//! `data-slot` / `data-*` / class / id.

mod error;
mod ir;
mod load_file;
mod match_sel;
mod registry;
mod watch;

use std::path::Path;

pub use error::{parse_selector, StylesheetError};
pub use ir::{MatchedStyleRule, Selector, StyleRule, Stylesheet};
pub use load_file::parse_stylesheet;
pub use match_sel::{apply_stylesheet_rules, apply_stylesheet_rules_tree, collect_matching_rules};
pub use registry::{clear_stylesheets, register_stylesheet, stylesheet_count};
pub(crate) use watch::take_style_hot_rebuild;
pub use watch::{is_theme_override_file, watch_stylesheets};

use error::with_path;

pub fn load_stylesheet(path: impl AsRef<Path>) -> Result<Stylesheet, StylesheetError> {
    let path = path.as_ref();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "toml" {
        return Err(with_path(
            path.to_path_buf(),
            StylesheetError::UnsupportedExtension(ext.into()),
        ));
    }
    let raw = std::fs::read_to_string(path).map_err(StylesheetError::Io)?;
    let mut sheet = parse_stylesheet(&raw)?;
    sheet.source = Some(path.to_path_buf());
    Ok(sheet)
}

/// Reload each path (keeping the previous registry on failure), then request a redraw.
pub fn reload_stylesheets<I, P>(paths: I) -> Result<(), StylesheetError>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let batch: Vec<_> = paths
        .into_iter()
        .map(|p| p.as_ref().to_path_buf())
        .collect();
    apply_style_file_batch(&batch)?;
    crate::app::runtime::request_redraw();
    Ok(())
}

/// Load a mixed batch: `theme.toml` → token patch; other `.toml` → stylesheet IR.
/// Does **not** redraw; watchers / HMR rebuild the tree afterwards.
///
/// Parse the whole batch **before** mutating live state. Any load or commit
/// failure restores the previous registry and theme overrides
/// (`error_handling.md` E6).
pub fn apply_style_file_batch(paths: &[impl AsRef<Path>]) -> Result<(), StylesheetError> {
    let mut sheet_paths = Vec::new();
    let mut theme_paths = Vec::new();
    for p in paths {
        let p = p.as_ref();
        if watch::is_theme_override_file(p) {
            theme_paths.push(p.to_path_buf());
        } else {
            sheet_paths.push(p.to_path_buf());
        }
    }

    let mut loaded_sheets = Vec::with_capacity(sheet_paths.len());
    for s in &sheet_paths {
        loaded_sheets.push(load_stylesheet(s)?);
    }
    let mut loaded_themes = Vec::with_capacity(theme_paths.len());
    for t in &theme_paths {
        loaded_themes.push(crate::style::load_theme_overrides(t)?);
    }
    if loaded_sheets.is_empty() && loaded_themes.is_empty() {
        return Ok(());
    }

    let sheet_snap = registry::snapshot_stylesheets();
    let theme_snap = crate::style::snapshot_theme_overrides();
    let commit = (|| -> Result<(), StylesheetError> {
        if !loaded_sheets.is_empty() {
            clear_stylesheets();
            for sheet in loaded_sheets {
                register_stylesheet(sheet)?;
            }
        }
        for patch in loaded_themes {
            crate::style::install_theme_overrides(patch)?;
        }
        Ok(())
    })();
    if let Err(err) = commit {
        registry::restore_stylesheets(sheet_snap);
        crate::style::restore_theme_overrides(theme_snap);
        return Err(err);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tree::Element;
    use crate::style::{
        clear_theme_overrides, get_theme, parse_color, set_theme_mode, Theme, ThemeMode,
    };

    fn reset() {
        clear_stylesheets();
        clear_theme_overrides();
        set_theme_mode(ThemeMode::Light);
    }

    #[test]
    fn toml_rules_roundtrip_ir() {
        reset();
        let toml = r#"
[[rule]]
selectors = ["[data-slot=bubble-content]"]
classes = "rounded-2xl px-3"

[[rule]]
selectors = ["[data-variant=destructive][data-slot=bubble-content]"]
classes = "bg-destructive/10"
priority = 10
"#;
        let a = parse_stylesheet(toml).expect("toml");
        assert_eq!(a.rules.len(), 2);
        let round = serde_json::to_string(&a.rules)
            .and_then(|s| serde_json::from_str::<Vec<StyleRule>>(&s));
        assert_eq!(round.unwrap(), a.rules);
    }

    #[test]
    fn unsupported_descendant_is_hard_error() {
        let err = parse_stylesheet(
            r#"
[[rule]]
selectors = ["div [data-slot=x]"]
classes = "p-2"
"#,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unsupported selector"), "{msg}");
    }

    #[test]
    fn match_data_slot_applies_classes() {
        reset();
        let sheet = parse_stylesheet(
            r#"
[[rule]]
selectors = ["[data-slot=bubble-content]"]
classes = "px-3"
"#,
        )
        .unwrap();
        register_stylesheet(sheet).unwrap();
        let mut el = Element::new();
        el.set_attr("data-slot", "bubble-content");
        el = apply_stylesheet_rules(el);
        let cs = el.class_set.as_ref().expect("class_set");
        assert!(cs.base.iter().any(|c| c == "px-3"), "got {:?}", cs.base);
        reset();
    }

    #[test]
    fn later_registration_and_priority() {
        reset();
        let first = parse_stylesheet(
            r#"
[[rule]]
selectors = ["[data-slot=x]"]
classes = "p-1"
"#,
        )
        .unwrap();
        let second = parse_stylesheet(
            r#"
[[rule]]
selectors = ["[data-slot=x]"]
classes = "p-4"
priority = 5
"#,
        )
        .unwrap();
        register_stylesheet(first).unwrap();
        register_stylesheet(second).unwrap();
        let mut el = Element::new();
        el.set_attr("data-slot", "x");
        el = apply_stylesheet_rules(el);
        let joined = el.class_set.as_ref().unwrap().base.join(" ");
        assert!(
            joined.find("p-1").unwrap() < joined.find("p-4").unwrap(),
            "{joined}"
        );
        reset();
    }

    #[test]
    fn theme_section_install_changes_destructive() {
        reset();
        let toml = r#"
[theme.light]
destructive = "oklch(0.62 0.25 30)"
"#;
        let sheet = parse_stylesheet(toml).unwrap();
        let base = Theme::light().colors.destructive;
        register_stylesheet(sheet).unwrap();
        let now = get_theme().colors.destructive;
        assert_ne!(now, base);
        assert_eq!(now, parse_color("oklch(0.62 0.25 30)"));
        reset();
    }

    #[test]
    fn match_1000x50_under_budget() {
        reset();
        let mut rules = Vec::new();
        for i in 0..50 {
            rules.push(StyleRule {
                selectors: vec![Selector::DataSlot(format!("slot-{i}"))],
                classes: "px-2".into(),
                priority: 0,
            });
        }
        register_stylesheet(Stylesheet {
            theme: None,
            rules,
            source: None,
        })
        .unwrap();

        let mut els: Vec<Element> = (0..1000)
            .map(|i| {
                let mut el = Element::new();
                el.set_attr("data-slot", format!("slot-{}", i % 50));
                el
            })
            .collect();
        let t = std::time::Instant::now();
        for el in &mut els {
            let taken = std::mem::replace(el, Element::new());
            *el = apply_stylesheet_rules(taken);
        }
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let budget = if cfg!(debug_assertions) { 25.0 } else { 1.0 };
        assert!(
            ms < budget,
            "match 1000×50 took {ms:.3}ms (budget {budget}ms)"
        );
        reset();
    }

    #[test]
    fn apply_theme_toml_batch_patches_destructive() {
        reset();
        let dir = std::env::temp_dir().join(format!("rui-style-hot-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("theme.toml");
        std::fs::write(&path, "[light]\ndestructive = \"oklch(0.62 0.25 30)\"\n").unwrap();
        apply_style_file_batch(&[&path]).unwrap();
        let base = Theme::light().colors.destructive;
        assert_ne!(get_theme().colors.destructive, base);
        let _ = std::fs::remove_file(&path);
        reset();
    }

    #[test]
    fn apply_stylesheet_rules_tree_covers_descendant_slots() {
        reset();
        let sheet = parse_stylesheet(
            r#"
[[rule]]
selectors = ["[data-slot=inner]"]
classes = "px-3"
"#,
        )
        .unwrap();
        register_stylesheet(sheet).unwrap();
        let mut inner = Element::new();
        inner.set_attr("data-slot", "inner");
        let mut root = Element::new();
        root.children.push(inner);
        root = apply_stylesheet_rules_tree(root);
        let cs = root.children[0].class_set.as_ref().expect("class_set");
        assert!(cs.base.iter().any(|c| c == "px-3"), "got {:?}", cs.base);
        reset();
    }

    #[test]
    fn apply_stylesheet_rules_is_idempotent() {
        reset();
        let sheet = parse_stylesheet(
            r#"
[[rule]]
selectors = ["[data-slot=x]"]
classes = "px-3"
"#,
        )
        .unwrap();
        register_stylesheet(sheet).unwrap();
        let mut el = Element::new();
        el.set_attr("data-slot", "x");
        el = apply_stylesheet_rules(el);
        el = apply_stylesheet_rules(el);
        let px = el
            .class_set
            .as_ref()
            .unwrap()
            .base
            .iter()
            .filter(|c| *c == "px-3")
            .count();
        assert_eq!(px, 1, "second apply must no-op via data-rui-sheet");
        assert_eq!(
            el.attrs.get("data-rui-sheet").map(String::as_str),
            Some("1")
        );
        reset();
    }

    #[test]
    fn data_variant_matches_rui_bubble_alias() {
        reset();
        let sheet = parse_stylesheet(
            r#"
[[rule]]
selectors = ["[data-variant=destructive][data-slot=bubble-content]"]
classes = "ring-2"
"#,
        )
        .unwrap();
        register_stylesheet(sheet).unwrap();
        let mut el = Element::new();
        el.set_attr("data-slot", "bubble-content");
        el.set_attr("rui_bubble_variant", "destructive");
        el = apply_stylesheet_rules(el);
        let hits = collect_matching_rules(&el);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].classes.contains("ring-2"));
        let cs = el.class_set.as_ref().unwrap();
        assert!(cs.base.iter().any(|c| c == "ring-2"));
        reset();
    }

    #[test]
    fn sheet_classes_keep_responsive_and_dark_variants() {
        reset();
        let sheet = parse_stylesheet(
            r#"
[[rule]]
selectors = ["[data-slot=card-title]"]
classes = "px-2 sm:px-4 dark:text-muted-foreground"
"#,
        )
        .unwrap();
        register_stylesheet(sheet).unwrap();
        let mut el = Element::new();
        el.set_attr("data-slot", "card-title");
        el = apply_stylesheet_rules(el);
        let cs = el.class_set.as_ref().expect("class_set");
        assert!(cs.base.iter().any(|c| c == "px-2"), "got {:?}", cs.base);
        assert!(
            cs.variants.iter().any(|(_, u)| u == "px-4"),
            "sm: must stay on class_set.variants; got {:?}",
            cs.variants
        );
        assert!(
            cs.variants
                .iter()
                .any(|(_, u)| u == "text-muted-foreground"),
            "dark: must stay on class_set.variants; got {:?}",
            cs.variants
        );
        reset();
    }

    #[test]
    fn reject_non_toml_extension() {
        let err = load_stylesheet("styles.css").unwrap_err();
        assert!(err.to_string().contains("unsupported"), "{}", err);
    }

    fn temp_style_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rui-style-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn apply_style_file_batch_keeps_previous_on_parse_error() {
        reset();
        let sheet = parse_stylesheet(
            r#"
[[rule]]
selectors = ["[data-slot=keep]"]
classes = "px-3"
"#,
        )
        .unwrap();
        register_stylesheet(sheet).unwrap();
        assert_eq!(stylesheet_count(), 1);

        let dir = temp_style_dir("keep");
        let bad = dir.join("bad.toml");
        std::fs::write(&bad, "this is not toml [[[").unwrap();
        assert!(apply_style_file_batch(&[&bad]).is_err());
        assert_eq!(
            stylesheet_count(),
            1,
            "failed reload must not clear registry"
        );

        let mut el = Element::new();
        el.set_attr("data-slot", "keep");
        el = apply_stylesheet_rules(el);
        let cs = el.class_set.as_ref().expect("class_set");
        assert!(cs.base.iter().any(|c| c == "px-3"), "got {:?}", cs.base);
        let _ = std::fs::remove_dir_all(&dir);
        reset();
    }

    #[test]
    fn apply_style_file_batch_does_not_commit_partial_batch() {
        reset();
        let sheet = parse_stylesheet(
            r#"
[[rule]]
selectors = ["[data-slot=x]"]
classes = "p-1"
"#,
        )
        .unwrap();
        register_stylesheet(sheet).unwrap();

        let dir = temp_style_dir("partial");
        let good = dir.join("good.toml");
        let bad = dir.join("bad.toml");
        std::fs::write(
            &good,
            r#"
[[rule]]
selectors = ["[data-slot=x]"]
classes = "p-8"
"#,
        )
        .unwrap();
        std::fs::write(
            &bad,
            r#"
[[rule]]
selectors = ["div [data-slot=x]"]
classes = "p-4"
"#,
        )
        .unwrap();
        assert!(apply_style_file_batch(&[&good, &bad]).is_err());
        assert_eq!(stylesheet_count(), 1);

        let mut el = Element::new();
        el.set_attr("data-slot", "x");
        el = apply_stylesheet_rules(el);
        let joined = el.class_set.as_ref().unwrap().base.join(" ");
        assert!(joined.contains("p-1"), "{joined}");
        assert!(
            !joined.contains("p-8"),
            "partial batch must not replace: {joined}"
        );
        let _ = std::fs::remove_dir_all(&dir);
        reset();
    }

    #[test]
    fn apply_style_file_batch_theme_fail_keeps_previous_theme() {
        reset();
        let dir = temp_style_dir("theme-keep");
        let path = dir.join("theme.toml");
        std::fs::write(&path, "[light]\ndestructive = \"oklch(0.62 0.25 30)\"\n").unwrap();
        apply_style_file_batch(&[&path]).unwrap();
        let patched = get_theme().colors.destructive;
        assert_eq!(patched, parse_color("oklch(0.62 0.25 30)"));

        std::fs::write(&path, "[light]\nthis is broken").unwrap();
        assert!(apply_style_file_batch(&[&path]).is_err());
        assert_eq!(
            get_theme().colors.destructive,
            patched,
            "failed theme reload must keep last good override"
        );
        let _ = std::fs::remove_dir_all(&dir);
        reset();
    }

    #[test]
    fn apply_style_file_batch_restores_on_theme_install_error() {
        reset();
        let dir = temp_style_dir("theme-install");
        let path = dir.join("theme.toml");
        std::fs::write(&path, "[light]\ndestructive = \"oklch(0.62 0.25 30)\"\n").unwrap();
        apply_style_file_batch(&[&path]).unwrap();
        let patched = get_theme().colors.destructive;

        std::fs::write(&path, "[light]\ndestructive = \"not-a-color\"\n").unwrap();
        assert!(apply_style_file_batch(&[&path]).is_err());
        assert_eq!(
            get_theme().colors.destructive,
            patched,
            "install failure must restore last good theme"
        );
        let _ = std::fs::remove_dir_all(&dir);
        reset();
    }

    #[test]
    fn apply_style_file_batch_replaces_on_success() {
        reset();
        let sheet = parse_stylesheet(
            r#"
[[rule]]
selectors = ["[data-slot=x]"]
classes = "p-1"
"#,
        )
        .unwrap();
        register_stylesheet(sheet).unwrap();

        let dir = temp_style_dir("replace");
        let path = dir.join("styles.toml");
        std::fs::write(
            &path,
            r#"
[[rule]]
selectors = ["[data-slot=x]"]
classes = "p-8"
"#,
        )
        .unwrap();
        apply_style_file_batch(&[&path]).unwrap();
        assert_eq!(stylesheet_count(), 1);

        let mut el = Element::new();
        el.set_attr("data-slot", "x");
        el = apply_stylesheet_rules(el);
        let joined = el.class_set.as_ref().unwrap().base.join(" ");
        assert!(joined.contains("p-8"), "{joined}");
        assert!(!joined.contains("p-1"), "{joined}");
        let _ = std::fs::remove_dir_all(&dir);
        reset();
    }
}
