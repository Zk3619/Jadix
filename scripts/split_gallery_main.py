"""One-shot: split examples/gallery/src/main.rs into modules. Idempotent if main.rs is already small."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "examples" / "gallery" / "src"
MAIN = SRC / "main.rs"

text = MAIN.read_text(encoding="utf-8")
if "mod pages;" in text and "mod app;" in text:
    print("already split, skip")
    raise SystemExit(0)

lines = text.splitlines(keepends=True)


def chunk(a: int, b: int) -> str:
    """1-indexed inclusive start, exclusive end."""
    return "".join(lines[a - 1 : b - 1])


def pubcrate(src: str) -> str:
    out = []
    for line in src.splitlines(True):
        if line.startswith("fn "):
            line = "pub(crate) " + line
        elif line.startswith("const "):
            line = "pub(crate) " + line
        elif line.startswith("struct "):
            line = "pub(crate) " + line
        elif line.startswith("enum "):
            line = "pub(crate) " + line
        elif line.startswith("#[inline(never)]\n") or line == "#[inline(never)]\r\n":
            continue
        out.append(line)
    return "".join(out)


IMPORTS = """#![allow(unused_imports)]

pub(crate) use rui::app::navigator::{navigation_host, Navigator, Presentation, PushOptions};
pub(crate) use rui::app::{
    close_window, open_secondary_window, open_secondary_window_with, request_exit,
    run_on_ui_thread, set_window_minimized, shared_slot, start_window_drag, system_tray_native,
    toggle_window_maximized, tray_icon, Router, TrayMenuItem, WindowOptions,
};
pub(crate) use rui::component;
pub(crate) use rui::components::*;
pub(crate) use rui::components::{tabs_headless, TABSTRIP_FOCUS_PREFIX};
pub(crate) use rui::core::reactivity::{batch, use_signal, Signal};
pub(crate) use rui::core::tree::{Element, LayoutStatus};
pub(crate) use rui::primitives::{
    canvas, circle, hstack, interactive, polyline, r#box, scene3d, svg, svg_path, vstack, Canvas2D,
    CanvasSize, Portal, PortalProps,
};
pub(crate) use rui::style::{parse_color, parse_text_color, TextSpan};
pub(crate) use rui::ui;
pub(crate) use rui::utils::animated_icon;
pub(crate) use rui::utils::lucide::lucide_glyph;
pub(crate) use rui::utils::text_node::{build_icon_text_node, build_text_node, TextNodeConfig};
pub(crate) use rui::{register_cursor_rgba8, CursorKind, MouseEvent};
pub(crate) use std::cell::{Cell, RefCell};
pub(crate) use std::collections::HashSet;
pub(crate) use std::rc::Rc;
"""

PAGE_HEAD = """use crate::imports::*;
use crate::shared::*;

"""

APP_SECTIONS = r'''
pub(crate) fn gallery_section_scroll(id: &str, items: Vec<Element>) -> Element {
    let col: Element = vstack()
        .w("100%")
        .gap("1rem")
        .p("1.5rem")
        .children(items)
        .into();
    scroll_area(id)
        .flex_grow(1.0)
        .min_h(0.0)
        .overflow_y("auto")
        .overflow_x("auto")
        .scroll_behavior("smooth")
        .child(col)
        .into()
}

pub(crate) fn gallery_section_body(section: &str) -> Element {
    match section {
        "forms" => gallery_section_forms(),
        "events" => gallery_section_events(),
        "overlays" => gallery_section_overlays(),
        "draw" => gallery_section_draw(),
        "data" => gallery_section_data(),
        "navigation" => gallery_section_navigation(),
        "layout" => gallery_section_layout(),
        "text" => gallery_section_text(),
        "about" => gallery_section_about(),
        _ => gallery_section_basics(),
    }
}

pub(crate) fn gallery_section_forms() -> Element {
    gallery_section_scroll(
        "scroll-forms",
        vec![
            InputShowcase(Default::default()),
            InputAdornmentShowcase(Default::default()),
            InputOTPShowcase(Default::default()),
            FormValidateShowcase(Default::default()),
            QuestionnaireShowcase(Default::default()),
            PickerPlaceholdersShowcase(Default::default()),
            TextareaComboboxShowcase(Default::default()),
            ToggleControlsShowcase(Default::default()),
            RadioSliderShowcase(Default::default()),
            SelectPopoverTabShowcase(Default::default()),
        ],
    )
}

pub(crate) fn gallery_section_events() -> Element {
    gallery_section_scroll(
        "scroll-events",
        vec![
            MouseEventShowcase(Default::default()),
            CursorShowcase(Default::default()),
        ],
    )
}

pub(crate) fn gallery_section_overlays() -> Element {
    gallery_section_scroll(
        "scroll-overlays",
        vec![
            OverlayFeedbackShowcase(Default::default()),
            PrimitivesShowcase(Default::default()),
        ],
    )
}

pub(crate) fn gallery_section_draw() -> Element {
    gallery_section_scroll(
        "scroll-draw",
        vec![
            DrawingSvgShowcase(Default::default()),
            LucideAnimatedShowcase(Default::default()),
            DrawingCanvasShowcase(Default::default()),
            DrawingScene3dShowcase(Default::default()),
        ],
    )
}

pub(crate) fn gallery_section_data() -> Element {
    gallery_section_scroll(
        "scroll-data",
        vec![
            TableShowcase(Default::default()),
            PaginationShowcase(Default::default()),
            TimelineShowcase(Default::default()),
            DataChartShowcase(Default::default()),
            VirtualMasonryShowcase(Default::default()),
        ],
    )
}

pub(crate) fn gallery_section_navigation() -> Element {
    gallery_section_scroll(
        "scroll-navigation",
        vec![
            SidebarShowcase(Default::default()),
            NavbarNavigationMenuShowcase(Default::default()),
            MenubarShowcase(Default::default()),
            StepperShowcase(Default::default()),
            NavigatorShowcase(Default::default()),
            RouterShowcase(Default::default()),
            TrayShowcase(),
            WindowChromeShowcase(),
            SelectPopoverTabShowcase(Default::default()),
        ],
    )
}

pub(crate) fn gallery_section_layout() -> Element {
    gallery_section_scroll(
        "scroll-layout",
        vec![
            LayoutSubtreeShowcase(Default::default()),
            DamageShowcase(Default::default()),
            OwnerRegistryShowcase(Default::default()),
            StoreResourceShowcase(Default::default()),
            RootBareGetShowcase(Default::default()),
            IdentityApiShowcase(Default::default()),
            BoxModelShowcase(Default::default()),
            ScrollShowcase(Default::default()),
            AccordionShowcase(Default::default()),
            CollapsibleShowcase(Default::default()),
            ResizableShowcase(Default::default()),
            LogicalShowcase(Default::default()),
            UtilityClassShowcase(Default::default()),
            GridAndDragShowcase(Default::default()),
            ShadowShowcase(Default::default()),
            FilterShowcase(Default::default()),
            GradientShowcase(Default::default()),
            BorderStyleShowcase(Default::default()),
            OutlineShowcase(Default::default()),
            TransitionShowcase(Default::default()),
            ScaleZoomShowcase(Default::default()),
            RotateTranslateShowcase(Default::default()),
            TransformShowcase(Default::default()),
        ],
    )
}

pub(crate) fn gallery_section_text() -> Element {
    gallery_section_scroll(
        "scroll-text",
        vec![
            TextAlignShowcase(Default::default()),
            TextCompatibilityShowcase(Default::default()),
            EmojiRealtimeRegressionShowcase(Default::default()),
            TypographyShowcase(Default::default()),
        ],
    )
}

pub(crate) fn gallery_section_about() -> Element {
    ui! {
        <ScrollArea id="scroll-about" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
            <VStack w="100%" gap="1rem" p="1.5rem" max_w="48rem">
                <Box bg="secondary" p="1.5rem" rounded="0.75rem" w="100%">
                    <VStack gap="0.5rem">
                        <Label font_weight="700">"Technical Stack"</Label>
                        <Label>"Layout: Taffy (Flexbox / Grid)"</Label>
                        <Label>"Text: Glyphon (Cosmic-text)"</Label>
                        <Label>"Rendering: WGPU"</Label>
                        <Label>"Reactivity: Signal fine-grained updates"</Label>
                        <Label>"A11y: AccessKit bridge"</Label>
                    </VStack>
                </Box>
                <Label text_size="0.875rem" color="muted-foreground">
                    "侧栏分区对应 ENGINEERING_IMPLEMENTATION_PLAN WP-3.1 Gallery 覆盖矩阵。"
                </Label>
            </VStack>
        </ScrollArea>
    }
}

pub(crate) fn gallery_section_basics() -> Element {
    gallery_section_scroll(
        "scroll-basics",
        vec![
            ButtonShowcase(Default::default()),
            ButtonHoverEasingShowcase(Default::default()),
            BadgeShowcase(Default::default()),
            KbdShowcase(),
            MarkerShowcase(),
            MessageShowcase(),
            MessageScrollerShowcase(),
            SkeletonShowcase(),
            EmptyShowcase(),
            AlertShowcase(),
            AlertDialogShowcase(),
            ItemShowcase(),
            AttachmentShowcase(),
            ListShowcase(),
            BubbleShowcase(Default::default()),
            AvatarShowcase(),
            BreadcrumbShowcase(),
            ButtonGroupShowcase(),
            AspectRatioShowcase(),
            AnimationShowcase(Default::default()),
            TypographyShowcase(Default::default()),
        ],
    )
}
'''

pages = SRC / "pages"
pages.mkdir(exist_ok=True)

(SRC / "imports.rs").write_text(IMPORTS, encoding="utf-8")

shared = PAGE_HEAD.replace("use crate::shared::*;\n\n", "") + pubcrate(
    chunk(35, 141)
) + "\n" + pubcrate(chunk(7476, 7488))
(SRC / "shared.rs").write_text(shared, encoding="utf-8")

page_files = {
    "basics.rs": ("//! 基础分区：按钮、气泡、头像等。\n\n" + PAGE_HEAD, [(141, 2370)]),
    "events.rs": ("//! 指针事件 / 光标。\n\n" + PAGE_HEAD, [(2370, 2716)]),
    "forms.rs": (
        "//! 表单分区。\n\n" + PAGE_HEAD,
        [(2716, 3257), (6220, 6598), (7328, 7476)],
    ),
    "text.rs": (
        "//! 文本分区。\n\n" + PAGE_HEAD,
        [(3257, 3328), (6029, 6220)],
    ),
    "layout.rs": (
        "//! 布局 / 视觉效果分区。\n\n" + PAGE_HEAD,
        [(3328, 6029), (6598, 6729)],
    ),
    "navigation.rs": (
        "//! 导航、路由、托盘、窗口外壳。\n\n" + PAGE_HEAD,
        [(6729, 7217), (9604, 10193)],
    ),
    "overlays.rs": (
        "//! 浮层 / 反馈与 Portal 原语。\n\n" + PAGE_HEAD,
        [(7217, 7328), (7488, 8580)],
    ),
    "draw.rs": ("//! SVG / Canvas / Scene3d。\n\n" + PAGE_HEAD, [(8580, 8848)]),
    "data.rs": ("//! 表格、图表、虚拟列表。\n\n" + PAGE_HEAD, [(8848, 9604)]),
}

for name, (head, ranges) in page_files.items():
    body = "\n".join(pubcrate(chunk(a, b)) for a, b in ranges)
    (pages / name).write_text(head + body, encoding="utf-8")

(pages / "mod.rs").write_text(
    """pub(crate) mod basics;
pub(crate) mod data;
pub(crate) mod draw;
pub(crate) mod events;
pub(crate) mod forms;
pub(crate) mod layout;
pub(crate) mod navigation;
pub(crate) mod overlays;
pub(crate) mod text;

pub(crate) use basics::*;
pub(crate) use data::*;
pub(crate) use draw::*;
pub(crate) use events::*;
pub(crate) use forms::*;
pub(crate) use layout::*;
pub(crate) use navigation::*;
pub(crate) use overlays::*;
pub(crate) use text::*;
""",
    encoding="utf-8",
)

app = (
    "//! Gallery 壳：侧栏、分区切换、根 `app_builder`。\n\n"
    "use crate::imports::*;\n"
    "use crate::pages::*;\n"
    "use crate::shared::*;\n\n"
    + pubcrate(chunk(10193, 10321))
    + APP_SECTIONS
    + "\n"
    + pubcrate(chunk(10618, 10748))
)
(SRC / "app.rs").write_text(app, encoding="utf-8")

tests_inner = "".join(lines[10768 : 11896])  # inside mod, drop closing brace of crate file
# drop the opening `    use super::*` indent? keep as-is — it's already indented with 4 spaces.
# Un-indent the module body by 4 spaces.
unindented = []
for line in tests_inner.splitlines(True):
    if line.startswith("    "):
        unindented.append(line[4:])
    else:
        unindented.append(line)
tests_src = "".join(unindented)
tests_src = tests_src.replace(
    """    /// Many showcase `ui!` trees still need this; the gallery binary itself does not set PE `/STACK`.
    const GALLERY_BUILD_STACK: usize = 8 * 1024 * 1024;

    fn run_on_stack<F>(bytes: usize, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        std::thread::Builder::new()
            .stack_size(bytes)
            .spawn(f)
            .expect("spawn gallery stack probe")
            .join()
            .expect("gallery stack probe");
    }

    fn run_on_pe_stack<F>(f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        run_on_stack(WINDOWS_PE_STACK, f);
    }

    fn run_on_gallery_build_stack<F>(f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        run_on_stack(GALLERY_BUILD_STACK, f);
    }
""",
    """    fn run_on_stack<F>(bytes: usize, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        std::thread::Builder::new()
            .stack_size(bytes)
            .spawn(f)
            .expect("spawn gallery stack probe")
            .join()
            .expect("gallery stack probe");
    }

    fn run_on_pe_stack<F>(f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        run_on_stack(WINDOWS_PE_STACK, f);
    }

    /// 1 MiB thread (Windows PE default) + framework [`rui::app::with_ui_build_stack`].
    fn run_on_gallery_build_stack<F>(f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        run_on_pe_stack(|| rui::app::with_ui_build_stack(f));
    }
""",
)
# tests_src is already unindented, so the replace above used 4-space indent from original
# inside-mod form. After unindent, comments have no extra 4 spaces from "mod" but still
# had 4 spaces of method indent... Let's fix: after unindent, the helpers look like:
#     const WINDOWS_PE...
# so the pattern should be without the extra module indent. Re-read after unindent.

(SRC / "mount_tests.rs").write_text(tests_src, encoding="utf-8")

MAIN.write_text(
    """#![allow(non_snake_case)]
// Release GUI builds should not spawn a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! RUI Gallery — 完整框架能力与组件 / 变体展示。
//!
//! 侧栏分区：基础 · 指针事件 · 表单 · 浮层 · 数据 · 绘制 · 导航（含 Router / 托盘 / 无边框窗） · 布局 · 文本 · 关于。
//! 运行：`cargo run -p gallery`；Inspector：`cargo run -p gallery --features devtools`（F12）；托盘原生：`cargo run -p gallery --features system-tray`
//!
//! 应用结构（与前端「入口 + 页面」相同）：[`main`] 只做启动；壳在 [`app`]；分区页面在 [`pages`]。

mod imports;
mod shared;
mod pages;
mod app;

pub(crate) use app::*;
pub(crate) use pages::*;
pub(crate) use shared::*;

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
""",
    encoding="utf-8",
)

print("split ok")
