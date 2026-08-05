//! # FilePicker / DirectoryPicker
//!
//! 原生文件/目录对话框（`rfd`）。选中路径写入按 `id` 隔离的信号。

use std::path::PathBuf;

use crate::components::button::button;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use rui_macros::component;

pub fn file_picker_paths_signal_key(id: &str) -> String {
    format!("{id}__file_paths")
}

pub fn file_picker_paths(id: &str) -> Signal<Vec<PathBuf>> {
    use_signal(&file_picker_paths_signal_key(id), Vec::<PathBuf>::new)
}

fn apply_accept(mut dlg: rfd::FileDialog, accept: &str) -> rfd::FileDialog {
    if accept.is_empty() || accept == "*/*" {
        return dlg;
    }
    if accept.contains("image/") {
        return dlg.add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp", "bmp"]);
    }
    let exts: Vec<&str> = accept
        .split([',', ' '])
        .map(|s| s.trim().trim_start_matches('.'))
        .filter(|s| !s.is_empty() && !s.contains('/'))
        .collect();
    if !exts.is_empty() {
        dlg = dlg.add_filter("Files", &exts);
    }
    dlg
}

/// 文件选择触发器（系统对话框）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn FilePicker(
    id: String,
    accept: Option<String>,
    multiple: bool,
    disabled: bool,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let accept = accept.unwrap_or_default();
    let label = if accept.is_empty() {
        if multiple {
            "Choose files…".to_string()
        } else {
            "Choose file…".to_string()
        }
    } else if multiple {
        format!("Choose files ({accept})…")
    } else {
        format!("Choose file ({accept})…")
    };
    let paths = file_picker_paths(&id);
    let accept_c = accept.clone();
    let mut el: Element = button(&id)
        .variant("outline")
        .size("sm")
        .disabled(disabled)
        .text(label)
        .on_click(move || {
            let dlg = apply_accept(rfd::FileDialog::new(), &accept_c);
            let picked = if multiple {
                dlg.pick_files().unwrap_or_default()
            } else {
                dlg.pick_file().into_iter().collect::<Vec<_>>()
            };
            if !picked.is_empty() {
                paths.set(picked);
            }
        })
        .into();
    el.template_id = Some(format!("filepicker:{id}"));
    el
}

/// 目录选择触发器。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DirectoryPicker(id: String, disabled: bool, children: Option<Element>) -> Element {
    let _ = children;
    let paths = file_picker_paths(&id);
    let mut el: Element = button(&id)
        .variant("outline")
        .size("sm")
        .disabled(disabled)
        .text("Choose folder…")
        .on_click(move || {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                paths.set(vec![dir]);
            }
        })
        .into();
    el.template_id = Some(format!("dirpicker:{id}"));
    el
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_picker_signal_key() {
        assert_eq!(file_picker_paths_signal_key("avatar"), "avatar__file_paths");
    }
}
