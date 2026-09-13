//! Wave 2 P: product chrome must not copy portal / presence / flush recipes.
//!
//! Allowed: `viewport_modal_layer` / `anchored_overlay_content` / `keyed_overlay`.
//! `#[cfg(test)]` items may still drive the stack directly.

use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN: &[&str] = &[
    "portal_sync(",
    "portal_sync_presence(",
    "use_modal_presence(",
    "use_floating_presence(",
    "flush_modal_shell_motion_subtree(",
    "flush_floating_panel_motion(",
];

fn components_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/components")
}

fn walk_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_rs(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

fn strip_comments(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                if bytes[i] == b'\n' {
                    out.push('\n');
                }
                i += 1;
            }
            i = i.saturating_add(2);
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn skip_ws(src: &[u8], mut i: usize) -> usize {
    while i < src.len() && src[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn skip_braced(src: &[u8], mut i: usize) -> usize {
    i = skip_ws(src, i);
    while i < src.len() && src[i] != b'{' {
        i += 1;
    }
    if i >= src.len() {
        return i;
    }
    let mut depth = 0i32;
    while i < src.len() {
        match src[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return i;
                }
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    i
}

fn strip_cfg_test_items(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    let marker = b"#[cfg(test)]";
    while i < bytes.len() {
        if bytes[i..].starts_with(marker) {
            i += marker.len();
            i = skip_ws(bytes, i);
            while bytes[i..].starts_with(b"#[") {
                while i < bytes.len() && bytes[i] != b']' {
                    i += 1;
                }
                i = i.saturating_add(1);
                i = skip_ws(bytes, i);
            }
            i = skip_braced(bytes, i);
            out.push('\n');
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn production_source(src: &str) -> String {
    strip_cfg_test_items(&strip_comments(src))
}

#[test]
fn product_components_do_not_copy_portal_recipes() {
    let root = components_root();
    assert!(root.is_dir(), "missing {}", root.display());
    let mut files = Vec::new();
    walk_rs(&root, &mut files);
    assert!(
        !files.is_empty(),
        "expected component sources under {}",
        root.display()
    );

    let mut hits = Vec::new();
    for path in &files {
        let Ok(src) = fs::read_to_string(path) else {
            continue;
        };
        let prod = production_source(&src);
        for needle in FORBIDDEN {
            if prod.contains(needle) {
                hits.push(format!("{}: `{needle}`", path.display()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "product files must use viewport_modal / anchored_overlay / keyed_overlay, not portal recipes:\n{}",
        hits.join("\n")
    );
}
