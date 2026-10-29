//! Theme + stylesheet 分区：覆盖 destructive，并用 styles.toml 统一气泡。

use std::path::PathBuf;

use crate::imports::*;
use crate::shared::*;
use jadix::prelude::{
    clear_stylesheets, clear_theme_overrides, get_theme, install_theme_overrides,
    load_and_install_theme_overrides, parse_stylesheet, parse_theme_overrides_str,
    register_stylesheet, reload_stylesheets, set_theme_preference, set_theme_preset, theme_mode,
    theme_overrides, theme_preference, theme_preset, theme_transition_config, toggle_theme_mode,
    ThemeMode, ThemePreference, ThemePreset, ThemeStyles,
};

const THEME_TOML: &str = include_str!("../../theme.toml");
const STYLES_TOML: &str = include_str!("../../styles.toml");

fn edit_styles(tick: Signal<u32>, edit: impl FnOnce(&mut ThemeStyles)) {
    let mut overrides = theme_overrides();
    edit(&mut overrides.styles);
    if let Err(error) = install_theme_overrides(overrides) {
        log::warn!("gallery theme edit: {error}");
    }
    bump(tick);
}

#[component]
pub(crate) fn ThemeStyleShowcase() -> Element {
    let tick = use_signal("gal_theme_style_tick", || 0u32);
    let _ = tick.get();
    let current = get_theme();
    let selected = theme_preset();
    let design_note = match selected {
        ThemePreset::Shadcn => "保留 shadcn 默认：中性色与精简的组件层级。",
        ThemePreset::Fluent => "Fluent：统一 Windows / WinUI 设计入口；4px 控件与卡片、8px 弹层，中性表面与品牌操作。",
        ThemePreset::Material3 => "MD3 baseline：胶囊按钮、16/24 输入正文、on-color 状态层、12px 卡片与 28px 对话框。",
        ThemePreset::LiquidGlass => "Liquid Glass：控件、导航与面板共享光学材质，分组底片在项目之间流动；下方网格用于检查弯曲、放大、色散与重绘稳定性。",
    };
    let optical_pattern: Vec<Element> = (0..24)
        .map(|stripe| {
            Element::new()
                .absolute()
                .left(stripe as f32 * 24.0)
                .top(0.0)
                .w(if stripe % 6 == 0 { 12.0 } else { 1.0 })
                .h("100%")
                .bg(if stripe % 6 == 0 { "primary" } else { "border" })
        })
        .chain((0..7).map(|stripe| {
            Element::new()
                .absolute()
                .left(0.0)
                .top(stripe as f32 * 24.0)
                .w("100%")
                .h(if stripe % 3 == 0 { 6.0 } else { 1.0 })
                .bg(if stripe % 3 == 0 { "primary" } else { "border" })
        }))
        .collect();
    let checked = use_signal("gal_theme_preview_checked", || true);
    let popover_open = use_signal("gal_theme_preview_popover", || false);
    let dialog_open = use_signal("gal_theme_preview_dialog", || false);
    let presets: Vec<Element> = ThemePreset::ALL
        .into_iter()
        .map(|preset| {
            let title = match preset {
                ThemePreset::Shadcn => "shadcn / 精简中性",
                ThemePreset::Fluent => "Fluent / Windows 设计体系",
                ThemePreset::Material3 => "Google MD3 baseline / 柔和圆润",
                ThemePreset::LiquidGlass => "Liquid Glass / 光学近似",
            };
            button(format!("gal-theme-preset-{}", preset.name()))
                .variant(if selected == preset {
                    "secondary"
                } else {
                    "outline"
                })
                .text(title)
                .on_click(move || {
                    set_theme_preset(preset);
                    bump(tick);
                })
                .into()
        })
        .collect();
    let appearance: Vec<Element> = [
        (ThemePreference::Light, "light", "浅色"),
        (ThemePreference::Dark, "dark", "深色"),
        (ThemePreference::System, "system", "跟随系统"),
    ]
    .into_iter()
    .map(|(preference, name, title)| {
        button(format!("gal-theme-mode-{name}"))
            .variant(if theme_preference() == preference {
                "secondary"
            } else {
                "outline"
            })
            .size("sm")
            .text(title)
            .on_click(move || {
                set_theme_preference(preference);
                bump(tick);
            })
            .into()
    })
    .collect();
    let density = move || {
        edit_styles(tick, |styles| {
            let height = get_theme().styles.control_height.unwrap_or(36.0);
            styles.control_height = Some(if height < 32.0 {
                36.0
            } else if height < 40.0 {
                44.0
            } else {
                28.0
            });
        })
    };
    let radius = move || {
        edit_styles(tick, |styles| {
            let radius = get_theme().styles.control_radius.unwrap_or(6.0);
            let radius = if radius < 8.0 {
                12.0
            } else if radius < 20.0 {
                24.0
            } else {
                0.0
            };
            styles.control_radius = Some(radius);
            styles.panel_radius = Some(radius + 8.0);
        })
    };
    let refraction = move || {
        edit_styles(tick, |styles| {
            let mut glass = get_theme().styles.glass.unwrap_or_default();
            glass.refraction_index = if glass.refraction_index < 1.1 {
                1.46
            } else if glass.refraction_index < 1.7 {
                1.9
            } else {
                1.0
            };
            styles.glass = Some(glass);
        })
    };
    let dispersion = move || {
        edit_styles(tick, |styles| {
            let mut glass = get_theme().styles.glass.unwrap_or_default();
            glass.dispersion = if glass.dispersion < 0.05 { 0.12 } else { 0.0 };
            styles.glass = Some(glass);
        })
    };
    let opaque = move || {
        edit_styles(tick, |styles| {
            styles.surface_opacity =
                Some(if get_theme().styles.surface_opacity.unwrap_or(1.0) < 1.0 {
                    1.0
                } else {
                    jadix::style::Theme::preset(theme_preset(), theme_mode())
                        .styles
                        .surface_opacity
                        .unwrap_or(1.0)
                });
        })
    };
    let blur = move || {
        edit_styles(tick, |styles| {
            let value = get_theme().styles.backdrop_blur.unwrap_or(0.0);
            styles.backdrop_blur = Some(if value < 4.0 {
                8.0
            } else if value < 12.0 {
                16.0
            } else {
                2.0
            });
        })
    };
    let thickness = move || {
        edit_styles(tick, |styles| {
            let mut glass = get_theme().styles.glass.unwrap_or_default();
            glass.thickness = if glass.thickness < 12.0 {
                16.0
            } else if glass.thickness < 20.0 {
                24.0
            } else {
                6.0
            };
            styles.glass = Some(glass);
        })
    };
    let highlight = move || {
        edit_styles(tick, |styles| {
            let mut glass = get_theme().styles.glass.unwrap_or_default();
            glass.highlight = if glass.highlight < 0.4 {
                0.6
            } else if glass.highlight < 0.8 {
                1.0
            } else {
                0.0
            };
            styles.glass = Some(glass);
        })
    };
    let opacity = move || {
        edit_styles(tick, |styles| {
            let value = get_theme().styles.surface_opacity.unwrap_or(1.0);
            styles.surface_opacity = Some(if value < 0.4 {
                0.6
            } else if value < 0.8 {
                1.0
            } else {
                0.25
            });
        })
    };
    let reset = move || edit_styles(tick, |styles| *styles = ThemeStyles::default());

    ui! {
        <VStack w="100%" min_w="0" gap="1rem">
            <MyCard title="主题工作台 · 四种设计语言">
                <VStack w="100%" min_w="0" gap="0.75rem">
                    <Label color="muted-foreground" leading="normal">
                        "预设 → 通用样式 → 明暗样式 → 组件 slot → styles.toml → 调用方 class / style。切换风格会应用到整个 Gallery。"
                    </Label>
                    <HStack w="100%" min_w="0" gap="0.5rem" wrap="wrap">{ presets }</HStack>
                    <Label id="gal-theme-style-summary" leading="normal">
                        { format!("{} · {:?} · 控件高度 {} · 圆角 {} · 折射 {}", selected.name(), current.mode, current.styles.control_height.map(|value| format!("{value:.0}px")).unwrap_or_else(|| "组件默认".into()), current.styles.control_radius.map(|value| format!("{value:.0}px")).unwrap_or_else(|| "组件默认".into()), current.styles.glass.map(|glass| format!("{:.2}", glass.refraction_index)).unwrap_or_else(|| "关闭".into())) }
                    </Label>
                    <Label id="gal-theme-design-note" color="muted-foreground" leading="normal">{ design_note }</Label>
                    <HStack w="100%" gap="0.5rem" wrap="wrap">
                        <Button id="gal-theme-density" variant="outline" size="sm" on_click={density}>"密度 / 高度"</Button>
                        <Button id="gal-theme-radius" variant="outline" size="sm" on_click={radius}>"圆角尺度"</Button>
                        <Button id="gal-theme-refraction" variant="outline" size="sm" on_click={refraction}>"折射率 1 / 1.46 / 1.9"</Button>
                        <Button id="gal-theme-dispersion" variant="outline" size="sm" on_click={dispersion}>"色散强度"</Button>
                        <Button id="gal-theme-blur" variant="outline" size="sm" on_click={blur}>"材质模糊"</Button>
                        <Button id="gal-theme-thickness" variant="outline" size="sm" on_click={thickness}>"透镜厚度"</Button>
                        <Button id="gal-theme-highlight" variant="outline" size="sm" on_click={highlight}>"边缘高光"</Button>
                        <Button id="gal-theme-surface-opacity" variant="outline" size="sm" on_click={opacity}>"表面透明度"</Button>
                        <Button id="gal-theme-opacity" variant="outline" size="sm" on_click={opaque}>"不透明降级"</Button>
                        <Button id="gal-theme-styles-reset" variant="ghost" size="sm" on_click={reset}>"恢复预设造型"</Button>
                    </HStack>
                    <HStack id="gal-theme-appearance" w="100%" gap="0.5rem" wrap="wrap">{ appearance }</HStack>
                </VStack>
            </MyCard>
            <VStack w="100%" min_w="0">
                <Card class="relative w-full max-w-[36rem]">
                    <CardHeader>
                        <CardTitle>"同一组组件，不同设计语言"</CardTitle>
                        <CardDescription>"内容卡不叠加玻璃。打开网格上的浮层，观察真实背景在边缘的弯曲与放大；浮层文字不参与折射。"</CardDescription>
                    </CardHeader>
                    <CardContent>
                        <VStack w="100%" min_w="0" gap="0.75rem">
                            <Input id="gal-theme-preview-input" placeholder="输入内容，体验主题排版与焦点环" w="100%" />
                            <HStack w="100%" gap="0.5rem" wrap="wrap">
                                <Button id="gal-theme-preview-primary">"Primary"</Button>
                                <Button variant="secondary">"Secondary"</Button>
                                <Button variant="outline">"Outline"</Button>
                                <Button variant="ghost">"Ghost"</Button>
                                <Button variant="destructive">"Destructive"</Button>
                                <Button disabled=true>"Disabled"</Button>
                            </HStack>
                            <HStack gap="0.5rem" wrap="wrap">
                                <Badge>"Default"</Badge><Badge variant="secondary">"Secondary"</Badge><Badge variant="destructive">"Alert"</Badge>
                            </HStack>
                            <Checkbox id="gal-theme-preview-checkbox" checked={checked}>"保留表单状态"</Checkbox>
                            <Switch id="gal-theme-preview-switch" checked={checked}>"共享受控状态"</Switch>
                            <VStack id="gal-theme-optical-stage" w="100%" h="176px" bg="background" class="relative">
                                <Box id="gal-theme-optical-grid" class="absolute inset-0 overflow-hidden" pointer_events="none">
                                    { optical_pattern }
                                </Box>
                                <Label class="absolute left-4 top-4" color="muted-foreground">"24px 网格 · 真实背景采样"</Label>
                                <Box class="absolute bottom-4 left-4">
                                <Popover id="gal-theme-popover" open={popover_open}>
                                    <PopoverTrigger>
                                        <Button id="gal-theme-preview-popover" variant="outline">"验收光学浮层"</Button>
                                    </PopoverTrigger>
                                    <PopoverContent side="right" align="end" sideOffset=12>
                                        <PopoverHeader>
                                            <PopoverTitle>"实时背景 · 光学浮层"</PopoverTitle>
                                            <PopoverDescription>"观察边缘折射、色散与反射；按 Escape 关闭。"</PopoverDescription>
                                        </PopoverHeader>
                                        <Input id="gal-theme-popover-input" placeholder="文字与输入不被折射" />
                                    </PopoverContent>
                                </Popover>
                                </Box>
                            </VStack>
                            <HStack gap="0.5rem" wrap="wrap">
                                <Dialog id="gal-theme-dialog" open={dialog_open}>
                                    <DialogTrigger>
                                        <Button id="gal-theme-preview-dialog" variant="outline">"验收模态面板"</Button>
                                    </DialogTrigger>
                                    <DialogContent>
                                        <DialogHeader>
                                            <DialogTitle>"主题化模态面板"</DialogTitle>
                                            <DialogDescription>"面板材质跟随全局预设；背景遮罩与内容保持独立。"</DialogDescription>
                                        </DialogHeader>
                                        <DialogBody><Input id="gal-theme-dialog-input" placeholder="测试焦点、对比度与清晰度" /></DialogBody>
                                    </DialogContent>
                                </Dialog>
                            </HStack>
                        </VStack>
                    </CardContent>
                </Card>
            </VStack>
            <MyCard title="连体组与状态切换验收">
                <HStack w="100%" gap="1.5rem" items="start" wrap="wrap">
                    <VStack gap="0.5rem">
                        <Label>"ButtonGroup · vertical"</Label>
                        <ButtonGroup orientation="vertical">
                            <Button id="gal-theme-group-top" variant="outline">"Top"</Button>
                            <Button id="gal-theme-group-middle" variant="outline">"Middle"</Button>
                            <Button id="gal-theme-group-bottom" variant="outline">"Bottom"</Button>
                        </ButtonGroup>
                    </VStack>
                    <VStack gap="0.5rem">
                        <Label>"ToggleGroup · vertical"</Label>
                        { Element::from(toggle_group("gal-theme-toggle-vertical")
                            .variant("outline").orientation("vertical").spacing(0).default_value("middle")
                            .options([("top", "Top"), ("middle", "Middle"), ("bottom", "Bottom")])) }
                    </VStack>
                    <VStack gap="0.5rem" w="20rem" min_w="0">
                        <Label>"Tabs · selection"</Label>
                        <Tabs id="gal-theme-tabs" default_value="first">
                            <TabsList>
                                <TabsTrigger value="first">"First"</TabsTrigger>
                                <TabsTrigger value="second">"Second"</TabsTrigger>
                                <TabsTrigger value="third">"Third"</TabsTrigger>
                            </TabsList>
                        </Tabs>
                        <Label color="muted-foreground" leading="normal">"连续改变悬停项或选中项，检查接缝、圆角和文字是否稳定。"</Label>
                    </VStack>
                </HStack>
            </MyCard>
        </VStack>
    }
}

fn gallery_file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(name)
}

fn bump(tick: jadix::core::reactivity::Signal<u32>) {
    tick.set(tick.get().saturating_add(1));
}

#[component]
pub(crate) fn ThemeShowcase() -> Element {
    let tick = use_signal("gal_theme_tick", || 0u32);
    let status = use_signal("gal_theme_status", || {
        "启动已装 theme.toml（destructive → 品红）· 未注册样式表".to_string()
    });
    let _ = tick.get();
    let mode = match theme_mode() {
        ThemeMode::Light => "light",
        ThemeMode::Dark => "dark",
    };
    let transition = theme_transition_config();

    let apply_embedded = move || match parse_theme_overrides_str(THEME_TOML) {
        Ok(patch) => match install_theme_overrides(patch) {
            Ok(()) => {
                status.set("已安装内嵌 theme.toml（destructive → 品红）".into());
                bump(tick);
            }
            Err(e) => status.set(format!("安装失败：{e}")),
        },
        Err(e) => status.set(format!("解析失败：{e}")),
    };
    let reload_disk = move || match load_and_install_theme_overrides(gallery_file("theme.toml")) {
        Ok(()) => {
            status.set("已从磁盘重载 theme.toml".into());
            bump(tick);
        }
        Err(e) => status.set(format!("磁盘重载失败：{e}")),
    };
    let reset_theme = move || {
        clear_theme_overrides();
        status.set("已清除主题覆盖".into());
        bump(tick);
    };
    let apply_sheet = move || match parse_stylesheet(STYLES_TOML) {
        Ok(sheet) => {
            clear_stylesheets();
            match register_stylesheet(sheet) {
                Ok(()) => {
                    status.set("已注册 styles.toml（[data-slot=bubble-content]）".into());
                    bump(tick);
                }
                Err(e) => status.set(format!("注册失败：{e}")),
            }
        }
        Err(e) => status.set(format!("样式表解析失败：{e}")),
    };
    let reload_sheet_disk = move || match reload_stylesheets([gallery_file("styles.toml")]) {
        Ok(()) => {
            status.set("已从磁盘重载 styles.toml".into());
            bump(tick);
        }
        Err(e) => status.set(format!("样式表磁盘重载失败：{e}")),
    };
    let clear_sheet = move || {
        clear_stylesheets();
        status.set("已清除样式表 registry".into());
        bump(tick);
    };
    let toggle_transition = move || {
        let mut config = theme_transition_config();
        config.enabled = !config.enabled;
        jadix::style::set_theme_transition_config(config);
        bump(tick);
    };
    let cycle_duration = move || {
        let mut config = theme_transition_config();
        config.duration_ms = if config.duration_ms < 300.0 {
            400.0
        } else if config.duration_ms < 700.0 {
            800.0
        } else {
            180.0
        };
        jadix::style::set_theme_transition_config(config);
        bump(tick);
    };
    let toggle_mode = move || {
        toggle_theme_mode();
        status.set(format!("主题模式 → {:?}", theme_mode()));
        bump(tick);
    };

    ui! {
        <VStack w="100%" min_w="0" gap="1rem">
        <MyCard title="主题令牌覆盖（theme.toml）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    { format!("只改 DesignTokens.destructive；Button / Badge / Bubble 不用改源码。当前模式：{mode}") }
                </Label>
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    { format!("主题平滑切换：{} · {:.0}ms · {}（可在 theme.toml 的 [transition] 配置）", if transition.enabled { "启用" } else { "停用" }, transition.duration_ms, transition.easing) }
                </Label>
                <Label id="gal-theme-status" w="100%" text_size="0.75rem" leading="normal">
                    { status.get() }
                </Label>
                <HStack w="100%" min_w="0" gap="0.5rem" wrap="wrap">
                    <Button id="gal-theme-apply" variant="default" size="sm" on_click={apply_embedded}>
                        "应用 theme.toml"
                    </Button>
                    <Button id="gal-theme-reload" variant="outline" size="sm" on_click={reload_disk}>
                        "从磁盘重载"
                    </Button>
                    <Button id="gal-theme-reset" variant="ghost" size="sm" on_click={reset_theme}>
                        "清除覆盖"
                    </Button>
                    <Button id="gal-theme-mode" variant="secondary" size="sm" on_click={toggle_mode}>
                        "切换 light/dark"
                    </Button>
                </HStack>
                <HStack w="100%" min_w="0" gap="0.5rem" wrap="wrap">
                    <Button id="gal-theme-transition" variant="outline" size="sm" on_click={toggle_transition}>
                        { if transition.enabled { "关闭平滑切换" } else { "开启平滑切换" } }
                    </Button>
                    <Button id="gal-theme-duration" variant="outline" size="sm" on_click={cycle_duration}>
                        { format!("切换时长（{:.0}ms）", transition.duration_ms) }
                    </Button>
                </HStack>
                <HStack w="100%" min_w="0" gap="0.5rem" items="center" wrap="wrap">
                    <Button variant="destructive">"Destructive"</Button>
                    <Button>"Primary"</Button>
                    <Badge variant="destructive">"Badge"</Badge>
                    <Badge>"Default"</Badge>
                </HStack>
            </VStack>
        </MyCard>
        <MyCard title="样式表（styles.toml → data-slot）">
            <VStack w="100%" min_w="0" gap="0.75rem">
                <Label w="100%" text_size="0.75rem" color="muted-foreground" leading="normal">
                    "气泡未写 RSX class。注册后 [data-slot=bubble-content] 得 rounded-2xl；destructive 另加 ring-2。匹配也认 jadix_bubble_variant。"
                </Label>
                <HStack w="100%" min_w="0" gap="0.5rem" wrap="wrap">
                    <Button id="gal-theme-sheet" variant="default" size="sm" on_click={apply_sheet}>
                        "应用 styles.toml"
                    </Button>
                    <Button id="gal-theme-sheet-disk" variant="outline" size="sm" on_click={reload_sheet_disk}>
                        "从磁盘重载"
                    </Button>
                    <Button id="gal-theme-sheet-clear" variant="ghost" size="sm" on_click={clear_sheet}>
                        "清除样式表"
                    </Button>
                </HStack>
                <VStack w="100%" min_w="0" max_w="28rem" gap="0.5rem">
                    <Bubble variant="destructive" align="start">
                        <BubbleContent>"destructive · 无 RSX class"</BubbleContent>
                    </Bubble>
                    <Bubble variant="muted" align="start">
                        <BubbleContent>"muted 对照"</BubbleContent>
                    </Bubble>
                    <Bubble variant="muted" align="start">
                        {
                            Element::from(
                                bubble_content()
                                    .class(crate::styles::BUBBLE_CONTENT)
                                    .text("Rust 模块 styles::BUBBLE_CONTENT（无加载器亦可）"),
                            )
                        }
                    </Bubble>
                </VStack>
            </VStack>
        </MyCard>
        </VStack>
    }
}

#[component]
pub(crate) fn GallerySectionTheme() -> Element {
    jadix::core::reactivity::isolate_reads(|| {
        ui! {
            <ScrollArea id="scroll-theme" flex_grow="1" min_h="0" overflow_y="auto" overflow_x="auto" scroll_behavior="smooth">
                <VStack w="100%" gap="1rem" p="1.5rem">
                    <ThemeStyleShowcase />
                    <ThemeShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}
