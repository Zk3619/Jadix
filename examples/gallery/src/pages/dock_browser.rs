use crate::imports::*;
use crate::shared::*;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum GalleryPageRoute {
    Gallery,
    DockBrowser,
}

pub(crate) fn gallery_page_navigator() -> Navigator<GalleryPageRoute> {
    Navigator::new(
        "gal_pages",
        GalleryPageRoute::Gallery,
        [GalleryPageRoute::Gallery],
    )
}

#[component]
pub(crate) fn DockBrowserShowcase() -> Element {
    let navigator = gallery_page_navigator();
    ui! {
        <MyCard title="Dock 工作区 · WebView · 页面栈">
            <VStack w="100%" gap="0.75rem">
                <Label>"打开一个覆盖整个窗口内容区的工作区。拖动分隔条调整布局，中央显示哔哩哔哩，返回后保留 Gallery 状态。"</Label>
                <Button id="gal-open-dock" on_click={move || {
                    navigator.push(GalleryPageRoute::DockBrowser, PushOptions::default());
                }}>"打开 Dock 网页工作区"</Button>
            </VStack>
        </MyCard>
    }
}

fn browser_content() -> Element {
    #[cfg(feature = "webview")]
    {
        let url = std::env::var("JADIX_GALLERY_WEBVIEW_URL")
            .unwrap_or_else(|_| "https://www.bilibili.com/".into());
        jadix_components::components::webview("gal-dock-browser", url)
    }
    #[cfg(not(feature = "webview"))]
    {
        ui! { <Label>"启用网页：cargo run -p gallery --features webview"</Label> }
    }
}

pub(crate) fn dock_browser_page(navigator: Navigator<GalleryPageRoute>) -> Element {
    navigator.register_active();
    let dialog_open = use_signal("gal_dock_overlay", || false);
    let dock = DockState::new(
        "gal-dock-workspace",
        DockLayout::split(
            "gal-dock-columns",
            DockOrientation::Horizontal,
            [18.0, 82.0],
            [
                DockLayout::tabs(
                    "gal-dock-files",
                    [
                        DockTab::new("explorer", "资源管理器", || {
                            ui! {
                                <VStack w="100%" h="100%" p="0.75rem" gap="0.5rem" bg="muted">
                                    <Label font_weight="600">"GALAXY"</Label>
                                    <Label>"资源管理器"</Label>
                                    <Label color="muted-foreground" text_size="0.75rem">"BILIBILI"</Label>
                                    <Label>"  网站工作区"</Label>
                                    <Label>"  ├─ 首页"</Label>
                                    <Label>"  └─ WebView"</Label>
                                </VStack>
                            }
                        }),
                        DockTab::new("search", "搜索", || {
                            ui! {
                                <VStack w="100%" h="100%" p="0.75rem" gap="0.5rem" bg="muted">
                                    <Label font_weight="600">"搜索"</Label>
                                    <Label color="muted-foreground" text_size="0.75rem">"当前工作区"</Label>
                                </VStack>
                            }
                        }),
                    ],
                ),
                DockLayout::split(
                    "gal-dock-editor-and-panel",
                    DockOrientation::Vertical,
                    [76.0, 24.0],
                    [
                        DockLayout::tabs(
                            "gal-dock-center",
                            [
                                DockTab::new("browser", "Bilibili", browser_content),
                                DockTab::new("notes", "欢迎", || {
                                    ui! {
                                        <VStack w="100%" h="100%" p="1rem" gap="0.75rem">
                                            <Label font_weight="600">"网页工作区"</Label>
                                            <Label>"中央编辑区承载原生 WebView；切换标签后网页继续保留浏览状态。"</Label>
                                            <Label>"拖动标签到边缘可拆分，拖动到标签栏可重排。"</Label>
                                        </VStack>
                                    }
                                }),
                            ],
                        ),
                        DockLayout::tabs(
                            "gal-dock-bottom-panel",
                            [
                                DockTab::new("terminal", "终端", || {
                                    ui! {
                                        <VStack w="100%" h="100%" p="0.75rem" gap="0.5rem" bg="muted">
                                            <Label font_weight="600">"终端"</Label>
                                            <Label color="muted-foreground" text_size="0.75rem">"Dock 布局 · 页面栈 · 原生 WebView"</Label>
                                        </VStack>
                                    }
                                }),
                                DockTab::new("problems", "问题", || {
                                    ui! {
                                        <VStack w="100%" h="100%" p="0.75rem" gap="0.5rem" bg="muted">
                                            <Label>"没有发现问题"</Label>
                                        </VStack>
                                    }
                                }),
                            ],
                        ),
                    ],
                ),
            ],
        ),
    );
    let layout = dock.clone().into_element();
    let restore_dock = dock.clone();
    ui! {
        <VStack id="gal-dock-page" w="100%" h="100%" min_w="0" min_h="0" bg="background" gap="0">
            <HStack w="100%" h="3.5rem" shrink="0" p="0.5rem" gap="0.75rem" items="center" bg="muted">
                <Button id="gal-dock-back" variant="outline" on_click={move || { let _ = navigator.pop(); }}>"返回 Gallery"</Button>
                <Label grow="1" min_w="0" font_weight="600">"Dock 网页工作区"</Label>
                <Button id="gal-dock-restore" variant="ghost" on_click={move || restore_dock.restore_closed()}>"恢复关闭的标签"</Button>
                <Button id="gal-dock-overlay-open" variant="secondary" on_click={move || dialog_open.set(true)}>"浮层验证"</Button>
            </HStack>
            <HStack w="100%" grow="1" flex_basis="0" min_w="0" min_h="0" gap="0" bg="muted">
                <VStack w="3rem" h="100%" shrink="0" gap="0.5rem" p="0.25rem" items="center" bg="muted">
                    <Button id="gal-dock-activity-explorer" size="sm" variant="secondary" aria_label="资源管理器">"▤"</Button>
                    <Button id="gal-dock-activity-search" size="sm" variant="ghost" aria_label="搜索">"⌕"</Button>
                    <Button id="gal-dock-activity-web" size="sm" variant="ghost" aria_label="网页">"◎"</Button>
                </VStack>
                <Box w="100%" grow="1" flex_basis="0" min_w="0" min_h="0" overflow="hidden">{layout}</Box>
            </HStack>
            <Dialog id="gal-dock-overlay" open={dialog_open}>
                <DialogContent>
                    <DialogHeader>
                        <DialogTitle>"WebView 上方的 Jadix 浮层"</DialogTitle>
                        <DialogDescription>"网页继续显示在遮罩下方；此对话框和按钮由 Jadix 绘制并处理交互。"</DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <Button id="gal-dock-overlay-close" on_click={move || dialog_open.set(false)}>"关闭浮层"</Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </VStack>
    }
}
