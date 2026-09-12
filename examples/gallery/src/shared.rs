use crate::imports::*;
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
pub(crate) const GAL_GRID_SORT_COMMIT: &str = "__rui_drag_commit:gal-grid-sort";
pub(crate) const GAL_GRID_SORT_HOVER: &str = "__rui_drag_hover:gal-grid-sort";
pub(crate) const GAL_GRID_LIST_ID: &str = "gal-grid-sort";
/// Demo remote `src` for `remote-asset`（main 里用 fetcher 映射成本地 PNG，不访问外网）。
pub(crate) const GALLERY_REMOTE_PNG: &str = "https://rui.test/gallery-demo.png";

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

pub(crate) fn increment_count(count: rui::core::reactivity::Signal<i32>) {
    count.set(count.peek() + 1);
}

pub(crate) fn choose_variant(selected: rui::core::reactivity::Signal<String>, variant: &str) {
    selected.set(variant.to_string());
}

#[component]
pub(crate) fn MyCard(
    title: String,
    children: Option<rui::core::tree::Element>,
) -> rui::core::tree::Element {
    let child = children.unwrap_or_else(rui::core::tree::Element::new);
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
    label: rui::core::reactivity::Signal<String>,
    ok: rui::core::reactivity::Signal<bool>,
) -> rui::core::tree::Element {
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
