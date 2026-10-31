#![allow(non_snake_case)]

//! Jadix Gallery — 完整框架能力与组件 / 变体展示。
//!
//! 侧栏按组件、交互导航、布局样式、图形媒体、文档开发、桌面运行时与 Gallery 分为七组。
//! 运行：`cargo run -p gallery`；默认 full 启用 Inspector（F12）、热重载、文档、媒体与桌面能力。
//! 热重载监听：`cargo run -p jadix_hot_reload_cli -- .`；分组与配置见 `examples/gallery/README.md`。
//!
//! `main` 只做启动（主题、可选远程图、窗口几何持久化）；壳在 [`app`]；分区页面在 [`pages`]。

mod app;
#[cfg(feature = "cpu-bench")]
mod cpu_bench;
mod imports;
mod pages;
mod shared;
mod styles;

use shared::checker_rgba_texture;

pub fn prepare_resources(directory: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(directory.join("assets"))?;
    for (name, bytes) in [
        ("theme.toml", include_bytes!("../theme.toml").as_slice()),
        ("styles.toml", include_bytes!("../styles.toml").as_slice()),
        (
            "assets/app-icon.png",
            include_bytes!("../assets/app-icon.png").as_slice(),
        ),
        (
            "assets/firefox-logo.svg",
            include_bytes!("../assets/firefox-logo.svg").as_slice(),
        ),
    ] {
        std::fs::write(directory.join(name), bytes)?;
    }
    shared::ROOT.set(directory.to_path_buf()).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "Gallery resources already initialized",
        )
    })
}

fn build_startup_scene() -> jadix::core::tree::Element {
    #[cfg(feature = "cpu-bench")]
    if std::env::var_os("JADIX_CPU_TELEMETRY").is_some()
        && std::env::var("JADIX_GALLERY_SECTION").as_deref() == Ok("ordinary")
    {
        return cpu_bench::ordinary_scene();
    }
    app::build_gallery_app()
}

fn install_gallery_theme() {
    let root = shared::gallery_resource_root();
    let theme = root.join("theme.toml");
    if let Err(e) = jadix::prelude::load_and_install_theme_overrides(&theme) {
        eprintln!("gallery: theme.toml: {e}");
    }
    jadix::prelude::watch_stylesheets([theme, root.join("styles.toml")]);
}

pub(crate) fn gallery_icon() -> jadix::app::AppIcon {
    jadix::app::AppIcon::from_bytes(include_bytes!("../../../assets/icons/desktop/window.png"))
        .expect("bundled Gallery icon")
}

pub fn run_gallery() {
    jadix::set_chrome_locale(&std::env::var("JADIX_LOCALE").unwrap_or_else(|_| "zh-CN".into()));
    let instance_demo = pages::instance_demo_mode(std::env::args_os().skip(1));
    #[cfg(feature = "cpu-bench")]
    let benchmarking = cpu_bench::install();
    #[cfg(not(feature = "cpu-bench"))]
    let benchmarking = false;
    if !benchmarking {
        shared::install_performance_overlay();
    }
    #[cfg(feature = "runtime-logging")]
    jadix::logging::init_with(jadix::logging::InitOptions {
        default_filter:
            "info,wgpu_core=warn,wgpu_hal=warn,wgpu_core::device::resource=off,fontdb=error",
    });
    jadix_lucide::install().expect("install Lucide icon catalog and font");
    install_gallery_theme();
    let (rgba, w, h) = checker_rgba_texture();
    jadix::queue_image_rgba8("demo-checker", w, h, rgba);

    #[cfg(feature = "remote-asset")]
    {
        let png = jadix::image_ops::encode_png(&jadix::media::DecodedImage {
            width: 2,
            height: 2,
            rgba: vec![
                59, 130, 246, 255, 59, 130, 246, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            ],
            intrinsic_hotspot: None,
            animation: None,
        })
        .expect("gallery demo png");
        jadix::set_remote_asset_fetcher(move |url: &str| {
            if url == shared::GALLERY_REMOTE_PNG {
                Ok(png.clone())
            } else {
                Err(format!("gallery demo fetcher has no mapping for {url}"))
            }
        });
    }

    #[cfg(any(
        not(any(
            target_os = "android",
            target_os = "ios",
            target_env = "ohos",
            target_arch = "wasm32"
        )),
        all(target_env = "ohos", feature = "ohos-desktop")
    ))]
    if !benchmarking && instance_demo.is_none() {
        if let Err(e) = jadix_window_state::Builder::new()
            .with_filename("gallery.window-state.toml")
            .install()
        {
            eprintln!("gallery: window state: {e}");
        }
    }

    let options = jadix::app::WindowOptions::new()
        .title("Jadix Gallery")
        .icon(gallery_icon())
        .follow_theme(true);
    let options = if let Some(mode) = instance_demo {
        jadix::app::set_second_instance_handler(pages::record_second_instance);
        let options = options
            .title(format!("Jadix Gallery — 进程实例 {mode}"))
            .size(800.0, 820.0);
        match mode {
            "single" => options.single_instance("org.jadix.gallery.instance-demo.a"),
            "profile" => options.single_instance("org.jadix.gallery.instance-demo.b"),
            _ => options.instance_policy(jadix::app::InstancePolicy::Multiple),
        }
    } else {
        options
    };
    #[cfg(all(debug_assertions, feature = "devtools"))]
    let options = options.devtools(std::env::var("JADIX_DEVTOOLS").as_deref() != Ok("0"));
    #[cfg(all(debug_assertions, feature = "devtools"))]
    if std::env::var("JADIX_DEVTOOLS_OPEN").as_deref() == Ok("1") {
        jadix::app::devtools::DevToolsState::set_open(true);
    }
    if instance_demo.is_some() {
        jadix::app::run_with(options, pages::gallery_instance_demo_root);
    } else {
        jadix::app::run_with(options, build_startup_scene);
    }
}

#[cfg(target_arch = "wasm32")]
pub fn run_gallery_web() {
    jadix::set_chrome_locale("zh-CN");
    jadix_lucide::install().expect("install Lucide assets");
    let theme = jadix::prelude::parse_theme_overrides_str(include_str!("../theme.toml"))
        .expect("bundled Gallery theme");
    jadix::prelude::install_theme_overrides(theme).expect("install Gallery theme");
    let styles = jadix::style::parse_stylesheet(include_str!("../styles.toml"))
        .expect("bundled Gallery styles");
    jadix::style::register_stylesheet(styles).expect("install Gallery styles");
    let (rgba, width, height) = checker_rgba_texture();
    jadix::queue_image_rgba8("demo-checker", width, height, rgba);
    jadix::run_with(
        jadix::WindowOptions::new()
            .title("Jadix Gallery")
            .canvas_target("jadix-canvas")
            .decorations(false)
            .default_font("Inter Variable, Jadix Gallery Sans"),
        build_startup_scene,
    );
}

#[cfg(test)]
mod mount_tests;

#[cfg(test)]
#[test]
fn gallery_default_icon_decodes_generated_logo() {
    let icon = gallery_icon();
    assert_eq!((icon.width(), icon.height()), (256, 256));
    assert_eq!(icon.rgba()[3], 0);
    assert!(icon.rgba().chunks_exact(4).any(|pixel| pixel[3] == 255));
}
