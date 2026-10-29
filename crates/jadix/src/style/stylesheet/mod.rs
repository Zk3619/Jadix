//! Global stylesheet loader (utility distributor). Not a CSSOM.
//!
//! External source is **TOML only**. Phase 2: [`apply_stylesheet_rules`] matches
//! `data-slot` / `data-*` / class / id.

mod error;
mod ir;
mod load_file;
mod match_sel;
mod registry;
pub(crate) mod watch;

use std::path::Path;

pub use error::{parse_selector, StylesheetError};
pub use ir::{MatchedStyleRule, Selector, StyleRule, Stylesheet};
pub use load_file::parse_stylesheet;
pub use match_sel::{apply_stylesheet_rules, apply_stylesheet_rules_tree, collect_matching_rules};
pub use registry::{clear_stylesheets, register_stylesheet, stylesheet_count};
pub(crate) use watch::{flush_style_rebuild, track_style_changes};
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
            StylesheetError::UnsupportedExtension(ext),
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
    watch::mark_style_hot_rebuild();
    Ok(())
}

/// Load a mixed batch: `theme.toml` → token patch; other `.toml` → stylesheet IR.
/// Successful commits schedule a rebuild of all mounted window trees.
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
#[path = "tests/mod.rs"]
mod tests;
