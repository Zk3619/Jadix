//! Theme + stylesheet 分区：覆盖 destructive，并用 styles.toml 统一气泡。

use std::path::PathBuf;

use crate::imports::*;
use crate::shared::*;
use jadix::prelude::{
    clear_stylesheets, clear_theme_overrides, install_theme_overrides,
    load_and_install_theme_overrides, parse_stylesheet, parse_theme_overrides_str,
    register_stylesheet, reload_stylesheets, theme_mode, theme_transition_config,
    toggle_theme_mode, ThemeMode,
};

const THEME_TOML: &str = include_str!("../../theme.toml");
const STYLES_TOML: &str = include_str!("../../styles.toml");

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
                    <ThemeShowcase />
                </VStack>
            </ScrollArea>
        }
    })
}
