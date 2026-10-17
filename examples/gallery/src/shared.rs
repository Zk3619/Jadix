use crate::imports::*;

static PERFORMANCE_OVERLAY: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// A slow sampler keeps the HUD from driving a permanent animation loop.
pub(crate) fn install_performance_overlay() {
    use std::sync::atomic::Ordering;
    if std::env::var("JADIX_PERF_OVERLAY").as_deref() == Ok("0")
        || PERFORMANCE_OVERLAY.swap(true, Ordering::Relaxed)
    {
        return;
    }
    jadix::app::run_on_ui_thread(|| jadix::app::frame_stats::enable_frame_timings(true));
    std::thread::spawn(|| {
        let mut previous = std::time::Instant::now();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let now = std::time::Instant::now();
            let seconds = now.duration_since(previous).as_secs_f64();
            previous = now;
            jadix::app::run_on_ui_thread(move || {
                let sample = jadix::app::frame_stats::take_frame_timings();
                let visible = use_signal("gal-perf-visible", || true).peek();
                if !visible {
                    return;
                }
                // A HUD update itself presents one frame. Do not let that one
                // frame keep changing the text and sustain redraws while idle.
                let text = if sample.frames <= 1 {
                    "Idle · 0 FPS\nCPU frame —\nLayout —  Text —".to_string()
                } else {
                    let n = sample.frames as f64;
                    format!(
                        "{:.0} FPS · {:.1} ms interval\nCPU {:.1} ms · max {:.1} ms\nLayout {:.1} ms · Text {:.1} ms",
                        n / seconds,
                        seconds * 1000.0 / n,
                        sample.total_ms / n,
                        sample.worst_ms,
                        sample.layout_ms / n,
                        sample.text_ms / n,
                    )
                };
                let label = use_signal("gal-perf-text", String::new);
                if label.peek() != text {
                    label.set(text);
                }
            });
        }
    });
}

pub(crate) fn gallery_performance_overlay() -> Element {
    if !PERFORMANCE_OVERLAY.load(std::sync::atomic::Ordering::Relaxed) {
        return Element::new();
    }
    jadix::utils::mount_reactive_anchor("gallery:performance-host", gallery_performance_panel)
}

fn gallery_performance_panel() -> Element {
    let visible = use_signal("gal-perf-visible", || true);
    let text = use_signal("gal-perf-text", || "Measuring…".to_string());
    let panel = ui! {
        <VStack absolute={true} right="12px" bottom="12px" z_index={1000000} gap="4px" items="end">
            <Button id="gal-perf-toggle" size="sm" variant="secondary"
                on_click={move || visible.set(!visible.peek())}>"性能 / FPS"</Button>
            <Show when={visible.get()} scope="gallery:performance">
                <Box w="280px" h="82px" p="8px" bg="#111827dd" rounded="6px" pointer_events="none">
                    <Label w="264px" h="66px" text_size="12px" color="#ffffff" leading="1.5">
                        { text.get() }
                    </Label>
                </Box>
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
        <VStack w="100%" min_w="0" p="1rem" bg="background" gap="0.5rem" borderStyle="solid" borderColor="border" borderWidth="0.125" class="rounded-lg">
            <Label w="100%" text_size="1.25rem" font_weight="700" leading="normal">{ title }</Label>
            { child }
        </VStack>
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
