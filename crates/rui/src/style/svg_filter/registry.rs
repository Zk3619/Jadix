//! Global registry for SVG `<filter>` graphs referenced by `url(#id)` / `url(file.svg#id)`.

use std::cell::RefCell;
use std::collections::HashMap;

use super::graph::FilterGraph;
use super::parse::{FilterParseError, parse_svg_filters_document};

thread_local! {
    static REGISTRY: RefCell<HashMap<String, FilterGraph>> = RefCell::new(HashMap::new());
    static FILE_CACHE: RefCell<HashMap<String, Result<(), String>>> = RefCell::new(HashMap::new());
}

/// Register a filter graph under `id` (without leading `#`).
pub fn register_svg_filter_graph(id: impl Into<String>, graph: FilterGraph) {
    let mut id = id.into();
    if let Some(rest) = id.strip_prefix('#') {
        id = rest.to_string();
    }
    let mut g = graph;
    if g.id.is_empty() {
        g.id = id.clone();
    }
    REGISTRY.with(|r| {
        r.borrow_mut().insert(id, g);
    });
}

/// Parse a single `<filter>...</filter>` (or a document containing one) and register by id.
pub fn register_svg_filter(id: impl Into<String>, markup: &str) -> Result<(), FilterParseError> {
    let id = id.into();
    let graphs = parse_svg_filters_document(markup)?;
    if graphs.is_empty() {
        return Err(FilterParseError::Message(
            "no <filter> elements found".into(),
        ));
    }
    // Prefer matching id; otherwise register first under provided id.
    let key = id.strip_prefix('#').unwrap_or(id.as_str());
    if let Some(g) = graphs.iter().find(|g| g.id == key) {
        register_svg_filter_graph(key, g.clone());
        return Ok(());
    }
    let mut g = graphs.into_iter().next().unwrap();
    g.id = key.to_string();
    register_svg_filter_graph(key, g);
    Ok(())
}

/// Look up a registered filter by id (`#foo` or `foo`).
pub fn lookup_svg_filter(id: &str) -> Option<FilterGraph> {
    let key = id.strip_prefix('#').unwrap_or(id);
    REGISTRY.with(|r| r.borrow().get(key).cloned())
}

/// Resolve a CSS `url()` reference to a [`FilterGraph`].
///
/// - `#id` / `id` → registry
/// - `path.svg#id` → load file (once), parse all filters, then lookup id
pub fn resolve_svg_filter_url(url: &str) -> Option<FilterGraph> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    if let Some(frag) = url.strip_prefix('#') {
        return lookup_svg_filter(frag);
    }
    if !url.contains('#') && !url.contains('.') {
        // bare id
        return lookup_svg_filter(url);
    }

    let (path, frag) = match url.rsplit_once('#') {
        Some((p, f)) => (p, f),
        None => (url, ""),
    };

    if !path.is_empty() {
        ensure_file_loaded(path);
    }
    if frag.is_empty() {
        None
    } else {
        lookup_svg_filter(frag)
    }
}

fn ensure_file_loaded(path: &str) {
    let already = FILE_CACHE.with(|c| c.borrow().contains_key(path));
    if already {
        return;
    }
    let result = match std::fs::read_to_string(path) {
        Ok(text) => match parse_svg_filters_document(&text) {
            Ok(graphs) => {
                for g in graphs {
                    let id = g.id.clone();
                    register_svg_filter_graph(id, g);
                }
                Ok(())
            }
            Err(e) => Err(e.to_string()),
        },
        Err(e) => Err(e.to_string()),
    };
    FILE_CACHE.with(|c| {
        c.borrow_mut().insert(path.to_string(), result);
    });
}

/// Clear registry (tests).
#[cfg(test)]
#[allow(dead_code)]
pub fn clear_svg_filter_registry() {
    REGISTRY.with(|r| r.borrow_mut().clear());
    FILE_CACHE.with(|c| c.borrow_mut().clear());
}
