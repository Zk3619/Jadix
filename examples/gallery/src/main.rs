#![allow(non_snake_case)]
// Release GUI builds should not spawn a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! RUI Gallery — 完整框架能力与组件 / 变体展示。
//!
//! 侧栏分区：基础 · 指针事件 · 表单 · 浮层 · 数据 · 绘制 · 导航（含 Router / 托盘 / 无边框窗） · 布局 · 文本 · 关于。
//! 运行：`cargo run -p gallery`；Inspector：`cargo run -p gallery --features devtools`（F12）；托盘原生：`cargo run -p gallery --features system-tray`
//!
//! 应用结构（与前端「入口 + 页面」相同）：[`main`] 只做启动；壳在 [`app`]；分区页面在 [`pages`]。

mod app;
mod imports;
mod pages;
mod shared;

use app::build_gallery_app;
use shared::checker_rgba_texture;

fn main() {
    #[cfg(feature = "runtime-logging")]
    rui::logging::init_with(rui::logging::InitOptions {
        default_filter:
            "info,wgpu_core=warn,wgpu_hal=warn,wgpu_core::device::resource=off,fontdb=error",
    });
    let (rgba, w, h) = checker_rgba_texture();
    rui::queue_image_rgba8("demo-checker", w, h, rgba);

    #[cfg(feature = "devtools")]
    rui::app::run_with(
        rui::app::WindowOptions::new().devtools(true),
        build_gallery_app,
    );
    #[cfg(not(feature = "devtools"))]
    rui::app::run(build_gallery_app);
}

#[cfg(test)]
mod mount_tests;
