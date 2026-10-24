# 主题系统

主题由设计语言、明暗语义色、通用样式、组件 slot 样式和切换动画组成。已有 `Theme::light()` / `Theme::dark()` 和旧版 `theme.toml` 保持 shadcn 默认外观；不指定新样式时不会统一覆盖组件原有尺寸。

## 四种预设

| `preset` | 风格 | 主要差异 |
| --- | --- | --- |
| `shadcn` | 中性、精简 | 保留现有组件默认值 |
| `fluent`（兼容别名 `winui3` / `winui-3`） | Fluent 设计语言 | Segoe UI Variable 字体栈、中性表面、品牌操作、32px 控件、4px 控件圆角、8px 浮层圆角、分层阴影 |
| `material3`（别名 `md3`） | Google Material Design 3 风格 | 紫色调、胶囊操作、宽松留白、较大面板圆角 |
| `liquid-glass` | Liquid Glass 跨平台近似 | 控件、分组、导航、Card 与浮层使用光学材质；文字独立清晰绘制，页面底层保持稳定 |

这些是 Jadix 的跨平台风格预设，不是各平台原生控件或官方设计系统的逐像素实现。字体使用本机字体栈回退，不自动下载字体。

Windows 的 GPU alpha 与原生透明窗口属性分开管理：`follow_theme(true)` 预留支持 alpha 的合成 surface，切换预设时复用同一个 HWND 和 surface；只有显式请求桌面穿透时才设置 winit 的原生透明属性。Gallery 默认启用 WebView，使用独立的 DirectComposition 内容树，Fluent 的系统 Mica 仍需通过 `DwmExtendFrameIntoClientArea` 扩展到客户区，不能因 swapchain 支持 alpha 而跳过。未启用 WebView 时，DX12 玻璃窗口使用自定义 DComp 背景与内容视觉树，不再叠加系统材质或 AccentPolicy。自定义配方不可用时按当前明暗模式的 `colors.background` token 回退。

本地 DWMBlurGlass 参考实现有两条不同路径：`Section/DwmAPIEffect.cpp` 设置系统 `DWMWA_SYSTEMBACKDROP_TYPE`；`Backdrops/BackdropFactory.cpp` 的自定义配方则由注入 DWM 的视觉树承载。Jadix 在应用进程中合成，复用了配方思路，并不注入 DWM，也不需要创建额外的透明顶层窗口。原生回归：构建 Gallery 后运行 `python scripts/window-material-check.py`，检查 Fluent 的实际合成像素、明暗切换、resize、重新进入预设及 HWND 数量；PNG、日志和 JSON 写入 `.tmp/window-material/`。

WinUI 3 是 Fluent 的 Windows 落地组件库，不再作为第五种设计语言。旧配置名仍可读取，重新序列化统一写为 `fluent`；`ThemePreset::WinUi3` 保留为弃用的源码兼容常量。统一预设不依赖 WinUI 或 windows-reactor 运行时，也不把跨平台阴影与模糊称为系统 Acrylic / Mica。Tooltip 使用 4px 圆角，内容 Card 使用 4px，常规浮层使用 8px；操作标签沿用 Fluent Semibold，正文保持 Regular。

Material 3 明确采用 baseline 子集，而非完整 Expressive：胶囊按钮、14/20/500 操作标签、16/24/400 输入正文、4px 输入与菜单、12px 卡片和 28px 对话框。Hover/press 使用 on-color 8%/10% 的状态层；禁用填充容器和文字分别采用 on-surface 12%/38%，不对整个按钮乘 0.5。40px 是按钮视觉高度，不代表所有控件的移动端触摸命中区。调用方仍可覆盖这些尺度与状态。

Liquid Glass 使用实时背景采样、曲面法线与 Snell 折射、RGB 分波长偏移、Schlick Fresnel 与屏幕空间环境反射，再叠加镜面、边缘高光和近似聚光。它不是 Apple 专有渲染器，也不是物理光线追踪；遮挡物外的环境信息不可得。Gallery 的“验收光学浮层”和“验收模态面板”使用真实 Portal 浮层，可检验材质、可读性与关闭行为。

```rust
use jadix::prelude::*;

set_theme_preset(ThemePreset::Material3);
set_theme_mode(ThemeMode::Dark);

let mut overrides = theme_overrides();
overrides.styles.control_radius = Some(12.0);
overrides.components.insert("button".into(), "hover:bg-secondary".into());
install_theme_overrides(overrides)?;
```

`set_theme_preset` 保留用户覆盖；`clear_theme_overrides` 清除覆盖并恢复 shadcn。独立构造主题可使用 `Theme::preset(preset, mode)`，再通过 `set_theme` 安装；需要明暗切换时保留定制，应使用 `ThemeOverrides`。

字号、行高、字重和圆角先按组件角色确定默认值，再合并通用与明暗覆盖。对应的 `Theme.styles` 字段为 `None` 表示“使用角色默认”，不是所有组件都没有样式。显式覆盖即使数值恰好等于某个预设的基线，也仍然优先；不再用浮点默认值相等与否猜测用户意图。

明暗偏好由 `ThemePreference::{Light, Dark, System}` 表示，默认 `System`。用 `set_theme_preference(ThemePreference::System)` 恢复跟随系统，启动时读取系统明暗，运行中响应系统主题变更并同步所有窗口。`set_theme_preference(Light / Dark)` 固定明暗；原有 `set_theme_mode(ThemeMode::Light / Dark)` 和 `toggle_theme_mode()` 也会进入手动模式。`theme_preference()` 返回用户选择，`theme_mode()` 返回已解析的实际明暗。预设、通用/明暗覆盖和焦点环设置在切换时保留；系统不提供主题时使用浅色。Gallery 主题页提供“浅色 / 深色 / 跟随系统”独立按钮。

## TOML 配置

```toml
preset = "liquid-glass"

[styles]
font-family = "Segoe UI, sans-serif"
font-size = 14
font-weight = 500
line-height = 20
control-height = 36
control-padding-x = 16
control-radius = 18
panel-radius = 24
panel-padding = 24
gap = 8
border-width = 1
shadow = "0 8px 32px 0 #00000024"
backdrop-blur = 2
surface-opacity = 0.16
disabled-opacity = 0.5

[styles.glass]
refraction-index = 1.46
thickness = 12
dispersion = 0.025
highlight = 0.6

[dark-styles]
surface-opacity = 0.22

[light]
primary = "#0066cc"
primary-foreground = "#ffffff"

[dark]
primary = "#8bc5ff"
primary-foreground = "#082c50"

[components]
button = "rounded-[14px] hover:bg-secondary"
card = "shadow-none"
input = "focus:ring-2"
checkbox = "rounded-[4px]"
dialog-content = "max-w-[640px]"

[transition]
enabled = true
duration-ms = 180
easing = "ease-in-out"
```

样式长度均为逻辑像素；`line-height` 是绝对行高；两项 opacity 为 0–1。旧的 `[light].radius` / `[dark].radius` 仍以 rem 表示。所有新样式字段均可省略，逐字段叠加到预设，不会因只改圆角而重置预设字体或阴影。`light-styles` / `dark-styles` 在通用 `styles` 之上覆盖。

通用造型覆盖 Button、Badge、Input、Textarea、Select 触发器、Card 及其分区和已有 slot 的弹层（Dialog、Popover、Select、菜单等），以及 Sidebar 页壳、导航表面、导航项、Tabs 和菜单选项的角色造型。SidebarInset 的桌面圆角分别为 Fluent 0px、MD3 28px、Liquid Glass 20px；窄屏仍取消 inset 圆角。Fluent 默认 `surface-opacity` 浅色 `0.35`、深色 `0.18`，Card 和常规浮层结合低对比度半透明填色与 `20px` 背景模糊；侧栏外轨与内部背景完全透明、无边框，不额外叠加模糊，以露出窗口的 Mica 材质。Liquid Glass 默认浅色 `0.44`、深色 `0.38`，SidebarInset、导航与 Card 保留独立的玻璃参数、边框和阴影。Sidebar 外轨和 wrapper 透明，Fluent 页壳透明；DX12 合成宿主同步更新 DWM 材质，切换预设和明暗不重建窗口。Fluent 滚动条默认轨道宽 `8px`、滑块宽 `6px`，轨道与描边透明，切换主题时丢弃旧滚动条颜色缓存；显式宽度与颜色覆盖继续生效。透明填色不等于将整个组件的 opacity 设为 0；品牌操作、交互状态和模态遮罩仍有自己的颜色。显式 `surface-opacity`、组件样式和调用方背景覆盖继续生效，`surface-opacity = 1` 关闭面板的自动背景模糊。字体令牌主要作用于控件文字，保留 Lucide 字体和嵌套 slot；标题层级仍由组件自身定义。按钮 xs/sm/lg 和 icon 尺寸按基准高度缩放，inline 不强制高度，Textarea 不覆盖其行数高度。

`components` 的键是精确的 `data-slot`，也可指定应用自建的 slot；值使用现有 utility 语法，包含 `hover:`、`focus:`、`disabled:`、`dark:` 和响应式变体。它与 `styles.toml` registry 独立，清除样式表不会移除主题。非法数值、玻璃参数和未知组件 utility 在安装前验证，失败不改变当前主题或已存覆盖。

样式优先级从低到高：组件默认值 → 预设与通用/明暗令牌 → `components` slot utility → `styles.toml` → 调用方 `class` → 调用方 `.style()` / RSX 样式。支持运行时 `bind_class` 替换，不会丢失主题层。主题切换复用现有热重建和动画机制，切换动画遵循 reduced-motion。

所有预设的 `Button variant="outline"` 在静止和禁用状态下均使用透明背景，Liquid Glass 不为它自动添加玻璃填色或背景模糊；悬停仍保留状态反馈，调用方显式背景覆盖继续生效。Fluent 侧栏选中项使用 Card 的填色、透明度与背景模糊，悬停时略微提高填色的不透明度，使其比选中态更亮，离开后恢复当前选中状态。

## 液态玻璃的光学路径

材质在绘制该面板之前捕获已绘制背景，同时保留未滤波 scene 和滤波后的背景；文字及后续子节点在材质之后绘制。GPU 从圆角/形状 SDF 推导边缘法线和凸起截面，通过折射率与厚度计算采样位移，并对 RGB 分别采样产生色散；另外计算 Schlick Fresnel、方向性镜面反射、轮廓高光和内侧聚光近似。透射光在整个表面采样同一扩散背景，未滤波 scene 仅供独立反射光照使用，避免出现清晰外壳和模糊内芯的材质断层。扁平中心不额外位移，不扭曲面板文字。常规 backdrop-filter 仍只绑定滤波背景，未调用光学捕获的既有接口保持兼容。

| 参数 | 范围 | 作用 |
| --- | --- | --- |
| `refraction-index` | 1–2.5 | 光线弯折；1 表示无折射 |
| `thickness` | 0–64 逻辑 px | 透镜边缘宽度与位移，随 DPI 缩放 |
| `dispersion` | 0–0.2 | RGB 光路间距 |
| `highlight` | 0–1 | Fresnel 反射、镜面与轮廓高光强度 |

也可直接给任何具有背景盒的元素使用 `class="glass"` 或 `class="glass-[1.46_12_0.025_0.6]"`。`glass-none` 关闭光学材质；完整不透明降级为 `glass-none backdrop-blur-none bg-card`。不支持背景捕获的路径使用自身背景色作为降级，因此部署时应保留合适的底色。纯透明玻璃不保证任意背景上的文字对比度；可提高 `surface-opacity`，或对正文面板采用不透明降级。

这是实时屏幕空间的光学近似，不包含场景几何追踪、真实环境探针、多次内部反射或物理级焦散，也不读取窗口外的桌面背景。全屏原生窗口材质与组件材质是独立能力。

## 开发参考与验证记录

主题研究依据、Gallery 验证与历史复验已集中到 development-docs：

- [官方参考与视觉验收依据](../../development-docs/comparison/theme-references-20261020.md)。
- [Gallery 验证与历史复验](../../development-docs/engine/theme-validation-20261021.md)。

开发文档为独立仓库内容；公开主题 API 与配置仍以本手册和源码为准。
