//! # FilePicker / DirectoryPicker
//!
//! 文件与目录选择占位。对应 HTML `input type="file"`，以及桌面端常见的「选文件夹」。
//! 与颜色 / 日期选择器**无共享业务状态**——落地时走系统对话框、权限与路径 API。
//!
//! **当前仅为 API + 视觉壳**：可聚焦、可点，不打开原生文件/目录对话框。
//!
//! ## 用法
//! ```ignore
//! <FilePicker id="avatar" accept="image/*" multiple=false />
//! <DirectoryPicker id="project-root" />
//! ```

use crate::components::picker_stub::{lucide, stub_label, stub_trigger};
use crate::core::tree::Element;
use rui_macros::component;

/// 文件选择触发器（HTML `type="file"`）。
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
    let icon = lucide(0xe0c0); // file
    stub_trigger(&id, icon, stub_label(&label, true), disabled)
}

/// 目录选择触发器（桌面「Open Folder」；非 HTML 标准，但与 FilePicker 同属路径选择）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DirectoryPicker(
    id: String,
    disabled: bool,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let icon = lucide(0xe247); // folder-open
    stub_trigger(
        &id,
        icon,
        stub_label("Choose folder…", true),
        disabled,
    )
}
