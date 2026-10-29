//! 原生文件 / 目录对话框能力（`rfd`）。
//!
//! **非 UI 组件**：由任意控件的任意事件调用。选中结果可直接使用，也可写入按 `id` 隔离的信号。

use std::path::PathBuf;

use crate::core::reactivity::{use_signal, Signal};

/// 按实例键缓存的路径列表信号 id。
pub fn file_dialog_paths_signal_key(id: &str) -> String {
    format!("{id}__file_paths")
}

/// 取得（或创建）与 `id` 绑定的路径列表信号。
pub fn file_dialog_paths(id: &str) -> Signal<Vec<PathBuf>> {
    use_signal(&file_dialog_paths_signal_key(id), Vec::<PathBuf>::new)
}

// rfd only supplies desktop dialogs. Keep the signal API available to mobile hosts.
#[cfg(all(
    feature = "file-dialog",
    any(
        target_os = "windows",
        target_os = "macos",
        all(target_os = "linux", not(target_env = "ohos"))
    )
))]
mod native {
    use super::PathBuf;
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

    /// 打开单文件对话框；取消则 `None`。
    pub fn pick_file(accept: &str) -> Option<PathBuf> {
        apply_accept(rfd::FileDialog::new(), accept).pick_file()
    }

    /// 打开多文件对话框；取消或未选则为空 `Vec`。
    pub fn pick_files(accept: &str) -> Vec<PathBuf> {
        apply_accept(rfd::FileDialog::new(), accept)
            .pick_files()
            .unwrap_or_default()
    }

    /// 打开目录对话框；取消则 `None`。
    pub fn pick_folder() -> Option<PathBuf> {
        rfd::FileDialog::new().pick_folder()
    }
}
#[cfg(all(
    feature = "file-dialog",
    any(
        target_os = "windows",
        target_os = "macos",
        all(target_os = "linux", not(target_env = "ohos"))
    )
))]
pub use native::{pick_file, pick_files, pick_folder};

#[cfg(not(all(
    feature = "file-dialog",
    any(
        target_os = "windows",
        target_os = "macos",
        all(target_os = "linux", not(target_env = "ohos"))
    )
)))]
mod native {
    use super::PathBuf;
    pub fn pick_file(_: &str) -> Option<PathBuf> {
        log::warn!(
            "Native file dialogs require the file-dialog feature and a supported desktop host"
        );
        None
    }
    pub fn pick_files(accept: &str) -> Vec<PathBuf> {
        pick_file(accept).into_iter().collect()
    }
    pub fn pick_folder() -> Option<PathBuf> {
        pick_file("")
    }
}
#[cfg(not(all(
    feature = "file-dialog",
    any(
        target_os = "windows",
        target_os = "macos",
        all(target_os = "linux", not(target_env = "ohos"))
    )
)))]
pub use native::{pick_file, pick_files, pick_folder};

/// `pick_file` / `pick_files`，结果非空时写入 `file_dialog_paths(id)`。
pub fn pick_files_into(id: &str, accept: &str, multiple: bool) -> Vec<PathBuf> {
    let picked = if multiple {
        pick_files(accept)
    } else {
        pick_file(accept).into_iter().collect()
    };
    if !picked.is_empty() {
        file_dialog_paths(id).set(picked.clone());
    }
    picked
}

/// `pick_folder`，选中时写入 `file_dialog_paths(id)`（单元素列表）。
pub fn pick_folder_into(id: &str) -> Option<PathBuf> {
    let dir = pick_folder()?;
    file_dialog_paths(id).set(vec![dir.clone()]);
    Some(dir)
}

#[cfg(test)]
#[path = "tests/file_dialog.rs"]
mod tests;
