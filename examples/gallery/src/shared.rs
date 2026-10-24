use crate::imports::*;

#[cfg(feature = "performance")]
static PERFORMANCE_OVERLAY: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// A slow sampler keeps the HUD from driving a permanent animation loop.
pub(crate) fn install_performance_overlay() {
    #[cfg(feature = "performance")]
    {
        use std::sync::atomic::Ordering;
        if std::env::var("JADIX_PERF_OVERLAY").as_deref() == Ok("0")
            || PERFORMANCE_OVERLAY.swap(true, Ordering::Relaxed)
        {
            return;
        }
        // Capture the actual UI thread, including when the installer is called off-thread.
        jadix::app::run_on_ui_thread(|| {
            jadix::app::frame_stats::enable_frame_timings(true);
            let ui_thread = jadix::performance::current_thread_id();
            std::thread::spawn(move || {
                let mut resources = jadix::performance::Sampler::new(ui_thread);
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    let (tx, rx) = std::sync::mpsc::sync_channel(1);
                    jadix::app::run_on_ui_thread(move || {
                        let snapshot = use_signal("gal-perf-window", || {
                            jadix::app::current_window().map(|w| w.id())
                        })
                        .peek()
                        .map(|window| {
                            let presentation =
                                jadix::app::frame_stats::window_presentation_stats(window);
                            let timings =
                                jadix::app::frame_stats::take_window_frame_timings(window);
                            (window, presentation, timings)
                        });
                        let _ = tx.send(snapshot);
                    });
                    // Only one outstanding sample: a busy UI must not replay stale HUD updates.
                    let Ok(snapshot) = rx.recv() else { break };
                    let resource = resources.sample();
                    let Some((window, presentation, timings)) = snapshot else {
                        continue;
                    };
                    let cadence = presentation.fps.map_or_else(
                    || "窗口呈现请求：采样中…".to_owned(),
                    |fps| {
                        let content = presentation
                            .content_fps
                            .map_or_else(|| "采样中".to_owned(), |value| format!("{value:.1}"));
                        format!(
                            "窗口呈现请求 {fps:.1} FPS · {} 次/{:.2}s\n页面呈现 {content} FPS · {} 次",
                            presentation.sample_frames,
                            presentation.sample_seconds,
                            presentation.content_sample_frames,
                        )
                    },
                );
                    let work = if timings.frames == 0 {
                        "渲染调用：本次无提交".to_owned()
                    } else {
                        let n = timings.frames as f64;
                        format!(
                            "渲染调用 {:.1} ms · 布局 {:.1} · 文本 {:.1}",
                            timings.total_ms / n,
                            timings.layout_ms / n,
                            timings.text_ms / n
                        )
                    };
                    let text = format!(
                        "{cadence}\n{work}\n{}",
                        performance_resource_text(&resource)
                    );
                    let (tx, rx) = std::sync::mpsc::sync_channel(1);
                    jadix::app::run_on_ui_thread(move || {
                        if use_signal("gal-perf-window", || Some(window)).peek() == Some(window)
                            && use_signal("gal-perf-visible", || true).peek()
                        {
                            let label = use_signal("gal-perf-text", String::new);
                            if label.peek() != text {
                                jadix::app::frame_stats::note_performance_overlay_update(window);
                                label.set(text);
                            }
                        }
                        let _ = tx.send(());
                    });
                    if rx.recv().is_err() {
                        break;
                    }
                }
            });
        });
    }
}

#[cfg(feature = "performance")]
fn performance_resource_text(resources: &jadix::performance::Resources) -> String {
    fn percent(value: Option<f64>) -> String {
        value
            .map(|v| format!("{v:.1}%"))
            .unwrap_or_else(|| "N/A".into())
    }
    fn mib(value: Option<u64>) -> String {
        value
            .map(|v| format!("{:.0}", v as f64 / 1_048_576.0))
            .unwrap_or_else(|| "N/A".into())
    }
    let cores = resources.logical_cpus.max(1);
    let equivalent = resources
        .process_percent
        .map(|v| format!("{:.2}", v / 100.0))
        .unwrap_or_else(|| "N/A".into());
    let sample_window = if resources.sample_seconds > 0.0 {
        format!("{:.2}s", resources.sample_seconds)
    } else {
        "N/A".to_owned()
    };
    format!(
        "进程 PID {} · 资源采样 {}\nCPU 进程 {} / {} 逻辑核\nCPU UI 线程 {} / 单核 · 进程 {} 核\nGPU 进程 引擎峰值 {}\nGPU 进程 专用 {} · 共享 {} MiB\n内存 进程 工作集 {} · 私有 {} MiB",
        resources.process_id,
        sample_window,
        percent(resources.process_percent.map(|v| v / cores as f64)),
        cores,
        percent(resources.ui_percent),
        equivalent,
        percent(resources.gpu_percent),
        mib(resources.gpu_dedicated),
        mib(resources.gpu_shared),
        mib(resources.working_set),
        mib(resources.private_bytes),
    )
}

pub(crate) fn gallery_performance_overlay() -> Element {
    #[cfg(feature = "performance")]
    if PERFORMANCE_OVERLAY.load(std::sync::atomic::Ordering::Relaxed) {
        return jadix::utils::mount_reactive_anchor(
            "gallery:performance-host",
            gallery_performance_panel,
        );
    }
    Element::new()
}

#[cfg(feature = "performance")]
fn gallery_performance_panel() -> Element {
    let window = jadix::app::current_window().map(|w| w.id());
    use_signal("gal-perf-window", || window).set_silent(window);
    let visible = use_signal("gal-perf-visible", || true);
    let text = use_signal("gal-perf-text", || "Measuring…".to_string());
    let panel = ui! {
        <VStack absolute={true} right="12px" bottom="12px" max_w="calc(100% - 24px)" z_index={1000000} gap="4px" items="end">
            <Button id="gal-perf-toggle" size="sm" variant="secondary"
                on_click={move || visible.set(!visible.peek())}>"性能 / FPS"</Button>
            <Show when={visible.get()} scope="gallery:performance">
                <GalleryPerformanceResources text={text} />
            </Show>
        </VStack>
    };
    // A viewport portal stays above retained scroll layers and modal captures.
    Element::new()
        .w(0.0)
        .h(0.0)
        .child(jadix::utils::viewport_portal_stage(
            "gallery-performance",
            panel,
            1_000_000,
        ))
}

#[cfg(feature = "performance")]
#[component]
fn GalleryPerformanceResources(text: Signal<String>) -> Element {
    ui! {
        <Box w="320px" max_w="100%" min_w="0" p="8px" shrink="0" bg="#111827ee" rounded="6px" pointer_events="none">
            <Label w="100%" min_w="0" text_size="12px" color="#ffffff" leading="1.5">
                { text.get() }
            </Label>
        </Box>
    }
}

#[cfg(all(test, feature = "performance"))]
#[test]
fn performance_panel_fits_every_resource_line() {
    jadix::core::reactivity::create_root(|| {
        let text = "窗口呈现请求 67.0 FPS · 67 次/1.00s\n页面呈现 65.0 FPS · 65 次\n渲染调用 1.5 ms · 布局 0.4 · 文本 0.2\n进程 PID 25436 · 资源采样 0.50s\nCPU 进程 0.4% / 32 逻辑核\nCPU UI 线程 12.5% / 单核 · 进程 0.12 核\nGPU 进程 引擎峰值 14.2%\nGPU 进程 专用 0 · 共享 125 MiB\n内存 进程 工作集 236 · 私有 210 MiB";
        let mut root = Element::new().flex_col().w(640.0).h(600.0)
            .child(GalleryPerformanceResources(GalleryPerformanceResourcesProps {
                text: Some(Signal::new(text.to_string())),
                ..Default::default()
            }));
        let mut layout = jadix::core::tree::LayoutTree::new();
        let scene = layout.build_and_layout(&mut root, 640.0, 600.0, [-1.0; 2]);
        let label = scene.text_nodes.iter().find(|node| node.text.starts_with("窗口呈现请求"))
            .expect("performance resource label");
        assert!(label.height >= text.lines().count() as f32 * 18.0,
            "performance label clips its last line: height={}", label.height);
        let panel = root.children[0].layout_bounds;
        assert!(label.y + label.height <= panel[1] + panel[3] - 8.0 + 0.5,
            "performance resource text overflows its panel: bottom={}, panel={panel:?}",
            label.y + label.height);
    });
}

#[cfg(all(test, feature = "performance"))]
#[test]
fn performance_resources_subscribe_without_rebuilding_the_panel() {
    use jadix::core::reactivity::{create_effect, create_root, reactivity_debug_snapshot};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    create_root(|| {
        let text = use_signal("performance-resource-binding-regression", || "Measuring…".to_string());
        let panel = Rc::new(RefCell::new(None));
        let builds = Rc::new(Cell::new(0));
        create_effect({
            let panel = panel.clone();
            let builds = builds.clone();
            move || {
                builds.set(builds.get() + 1);
                *panel.borrow_mut() = Some(GalleryPerformanceResources(GalleryPerformanceResourcesProps {
                    text: Some(text),
                    ..Default::default()
                }));
            }
        });
        for sample in ["CPU 1.0%", "CPU 2.0%\nGPU 3.0%"] {
            text.set(sample.to_string());
            let graph = reactivity_debug_snapshot();
            let signal = graph.named_signal_entries.iter()
                .find(|entry| entry.key == "performance-resource-binding-regression").unwrap();
            assert!(graph.signal_effect_edges.iter().any(|edge|
                edge.signal_id == signal.signal_id && !edge.effect_ids.is_empty()),
                "the performance label must subscribe to live samples, not a String snapshot");
            assert_eq!(builds.get(), 1, "sampling must not rebuild the panel or page animations");
        }
    });
}

/// Packaged resources live beside the executable; cargo run uses the source tree.
pub(crate) fn gallery_resource_root() -> &'static std::path::Path {
    static ROOT: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
            .filter(|dir| dir.join("theme.toml").is_file())
            .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")))
    })
}

/// Stable `template_id` for fine-grained reconcile (must stay unique per mounted block).
pub(crate) const PRIMITIVES_SHOWCASE_REACTIVE: &str =
    "examples/gallery/PrimitivesShowcase:controls";
pub(crate) const TEXT_ALIGN_WP21_VIRT: &str = "gal-wp21-virt";
pub(crate) const PRIMITIVES_VIRT: &str = "gal-virt-demo";
/// WP-R.6: stable structure-island `template_id` (hot reload / reconcile slot).
/// Do not switch to `file:line:col` — that moves when the source is edited.
pub(crate) const GRID_DRAG_REACTIVE: &str = "examples/gallery/GridAndDragShowcase:main";
/// WP-R.6: second gallery example of an explicit reactive slot (Navigator body).
pub(crate) const NAVIGATOR_SHOWCASE_REACTIVE: &str = "examples/gallery/NavigatorShowcase:main";
pub(crate) const GAL_SHELL_ROUTER_ID: &str = "gal_shell_router";
pub(crate) const GAL_SHELL_BRIDGE: &str = "gal_shell_bridge_count";
#[cfg(test)]
pub(crate) const GAL_SHELL_TRAY_SHOW: &str = "gal_shell_tray_show";
#[cfg(test)]
pub(crate) const GAL_SHELL_FRAMELESS: &str = "gal_shell_frameless";
#[cfg(test)]
pub(crate) const GAL_SHELL_TRANSPARENT: &str = "gal_shell_transparent";
#[cfg(test)]
pub(crate) const GAL_SHELL_BLUR: &str = "gal_shell_blur";
#[cfg(test)]
pub(crate) const GAL_SHELL_GLASS_BLUR: &str = "gal_shell_glass_blur";
#[cfg(test)]
pub(crate) const GAL_SHELL_GLASS_AERO: &str = "gal_shell_glass_aero";
#[cfg(test)]
pub(crate) const GAL_SHELL_GLASS_ACRYLIC: &str = "gal_shell_glass_acrylic";
#[cfg(test)]
pub(crate) const GAL_SHELL_GLASS_MICA: &str = "gal_shell_glass_mica";
pub(crate) const GAL_GRID_SORT_COMMIT: &str = "__jadix_drag_commit:gal-grid-sort";
pub(crate) const GAL_GRID_SORT_HOVER: &str = "__jadix_drag_hover:gal-grid-sort";
pub(crate) const GAL_GRID_LIST_ID: &str = "gal-grid-sort";
/// Demo remote `src` for `remote-asset`（main 里用 fetcher 映射成本地 PNG，不访问外网）。
pub(crate) const GALLERY_REMOTE_PNG: &str = "https://jadix.test/gallery-demo.png";

pub(crate) fn gal_grid_sort_row(row_label: &str, i: usize, _items: &[String], hl: bool) -> Element {
    ui! {
        <HStack
            w="100%"
            p="0.5rem"
            gap="0.5rem"
            items="center"
            bg={if hl { "accent" } else { "muted" }}
            z_index={if hl { 1 } else { 0 }}
            rounded={6.0}
        >
            <Box
                drag_reorder_handle={(GAL_GRID_LIST_ID, i)}
                p="0.25rem"
                rounded={4.0}
                bg="secondary"
            />
            <Label id={format!("gal-grid-sort-{i}")} grow="1">
                {row_label}
            </Label>
        </HStack>
    }
}

pub(crate) fn checker_rgba_texture() -> (Vec<u8>, u32, u32) {
    let w = 64u32;
    let h = 64u32;
    let mut v = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let dark = ((x / 8) + (y / 8)) % 2 == 0;
            let (r, g, b) = if dark {
                (240u8, 240, 245)
            } else {
                (90, 95, 115)
            };
            let i = ((y * w + x) * 4) as usize;
            v[i..i + 4].copy_from_slice(&[r, g, b, 255]);
        }
    }
    (v, w, h)
}

pub(crate) fn increment_count(count: jadix::core::reactivity::Signal<i32>) {
    count.set(count.peek() + 1);
}

pub(crate) fn choose_variant(selected: jadix::core::reactivity::Signal<String>, variant: &str) {
    selected.set(variant.to_string());
}

#[component]
pub(crate) fn MyCard(
    title: String,
    children: Option<jadix::core::tree::Element>,
) -> jadix::core::tree::Element {
    let child = children.unwrap_or_else(jadix::core::tree::Element::new);
    ui! {
        <Card>
            <CardHeader>
                <CardTitle>{ title }</CardTitle>
            </CardHeader>
            <CardContent>{ child }</CardContent>
        </Card>
    }
}

/// Solid getter-props 试点：`#[component(reactive)]` + Signal 字段；文案/图标走自动 bind，不整卡重建。
/// `id` 为宏要求的实例键（本组件体未再写入 focus/template）。
#[component(reactive)]
pub(crate) fn ReactiveStatusBadge(
    id: String,
    label: jadix::core::reactivity::Signal<String>,
    ok: jadix::core::reactivity::Signal<bool>,
) -> jadix::core::tree::Element {
    let _ = &id; // 实例键由 #[component(reactive)] 占用；徽标本身展示型可不写 id
    ui! {
        <Badge
            variant={ if ok.get() { "default" } else { "destructive" } }
        >
            <Icon
                name={ if ok.get() { "check".to_string() } else { "ban".to_string() } }
                size="0.75rem"
            />
            { label.get() }
        </Badge>
    }
}

pub(crate) fn gallery_feedback_toast(title: impl Into<String>, description: impl Into<String>) {
    toast(title).description(description).duration(5000).show();
}

pub(crate) fn gallery_goto_section(key: &str) {
    use_signal("gal_nav_section", || "basics".to_string()).set(key.to_string());
}

#[cfg(all(test, feature = "performance"))]
mod performance_tests {
    #[test]
    fn resource_text_distinguishes_core_usage_and_machine_capacity() {
        let text = super::performance_resource_text(&jadix::performance::Resources {
            process_percent: Some(400.0),
            ui_percent: Some(80.0),
            logical_cpus: 16,
            ..Default::default()
        });
        assert!(text.contains("进程 4.00 核"));
        assert!(text.contains("25.0% / 16 逻辑核"));
        assert!(text.contains("GPU 进程 引擎峰值 N/A"));
    }
}
