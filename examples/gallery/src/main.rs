#![allow(non_snake_case)]
// Release GUI builds should not spawn a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! RUI Gallery — 完整框架能力与组件 / 变体展示。
//!
//! 侧栏分区：基础 · 指针事件 · 表单 · 浮层 · 数据 · 绘制 · 导航（含 Router / 托盘 / 无边框窗） · 布局 · 文本 · 主题 · 关于。
//! 运行：`cargo run -p gallery`；Inspector：`cargo run -p gallery --features devtools`（F12）；托盘原生：`--features system-tray`；远程图入口：`--features remote-asset`
//!
//! `main` 只做启动（主题、可选远程图、窗口几何持久化）；壳在 [`app`]；分区页面在 [`pages`]。

mod app;
mod imports;
mod pages;
mod shared;
mod styles;

use std::path::PathBuf;

use app::build_gallery_app;
use shared::checker_rgba_texture;

fn install_gallery_theme() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let theme = root.join("theme.toml");
    if let Err(e) = rui::prelude::load_and_install_theme_overrides(&theme) {
        eprintln!("gallery: theme.toml: {e}");
    }
    rui::prelude::watch_stylesheets([theme, root.join("styles.toml")]);
}

fn main() {
    #[cfg(feature = "runtime-logging")]
    rui::logging::init_with(rui::logging::InitOptions {
        default_filter:
            "info,wgpu_core=warn,wgpu_hal=warn,wgpu_core::device::resource=off,fontdb=error",
    });
    install_gallery_theme();
    let (rgba, w, h) = checker_rgba_texture();
    rui::queue_image_rgba8("demo-checker", w, h, rgba);

    #[cfg(feature = "remote-asset")]
    {
        let png = rui::image_ops::encode_png(&rui::media::DecodedImage {
            width: 2,
            height: 2,
            rgba: vec![
                59, 130, 246, 255, 59, 130, 246, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            ],
            intrinsic_hotspot: None,
            animation: None,
        })
        .expect("gallery demo png");
        rui::set_remote_asset_fetcher(move |url: &str| {
            if url == shared::GALLERY_REMOTE_PNG {
                Ok(png.clone())
            } else {
                Err(format!("gallery demo fetcher has no mapping for {url}"))
            }
        });
    }

    if let Err(e) = jadix_window_state::Builder::new()
        .with_filename("gallery.window-state.toml")
        .install()
    {
        eprintln!("gallery: window state: {e}");
    }

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
