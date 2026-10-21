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

通用造型覆盖 Button、Badge、Input、Textarea、Select 触发器、Card 及其分区和已有 slot 的弹层（Dialog、Popover、Select、菜单等），以及 Sidebar 页壳、导航表面、导航项、Tabs 和菜单选项的角色造型。SidebarInset 的桌面圆角分别为 Fluent 8px、MD3 28px、Liquid Glass 20px；窄屏仍取消 inset 圆角。导航背景、页面背景、内容卡和功能浮层分别使用对应颜色角色；Liquid Glass 按本项目展示需求为 Card 与常用控件提供自动玻璃，页面根背景本身不自动折射。字体令牌主要作用于控件文字，保留 Lucide 字体和嵌套 slot；标题层级仍由组件自身定义。按钮 xs/sm/lg 和 icon 尺寸按基准高度缩放，inline 不强制高度，Textarea 不覆盖其行数高度。

`components` 的键是精确的 `data-slot`，也可指定应用自建的 slot；值使用现有 utility 语法，包含 `hover:`、`focus:`、`disabled:`、`dark:` 和响应式变体。它与 `styles.toml` registry 独立，清除样式表不会移除主题。非法数值、玻璃参数和未知组件 utility 在安装前验证，失败不改变当前主题或已存覆盖。

样式优先级从低到高：组件默认值 → 预设与通用/明暗令牌 → `components` slot utility → `styles.toml` → 调用方 `class` → 调用方 `.style()` / RSX 样式。支持运行时 `bind_class` 替换，不会丢失主题层。主题切换复用现有热重建和动画机制，切换动画遵循 reduced-motion。

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

## Gallery 与验证

主题页面提供四种风格切换、明暗切换、控件高度、圆角、折射率、色散、模糊、厚度、高光、表面透明度、不透明降级与恢复预设。所有尺度与光学设置集中于 `ThemeStyles` / TOML `[styles]`，密度通过控件高度、水平 padding、panel padding 和 gap 调节，并非另一套并行状态。页面可打开真实 Popover 和 Dialog；双轴蓝色标尺和 24px 网格用于观察四边和圆角的弯曲与放大。Gallery 通用展示卡也使用同一 Card slot，其他页面随全局主题重建。

玻璃默认 tint 为浅色 44%、深色 38%，blur 为 8px；品牌操作保留较高底色以保证对比度。透射与环境反射共用同一扩散背景，避免清晰反射与柔化内部形成暗色材质断层；降低镜面/轮廓能量，移除宽环形假焦散。曲面只改变采样坐标，不扭曲文字。这些数值是工程默认，不是 Apple 官方常量。自动材质覆盖普通控件、Card、导航和分组容器及浮层；内嵌 Command 和 Tooltip 不自动折射。连体组抑制成员的重复玻璃背景，由容器与共享指示器绘制，减少叠层；复杂内容仍推荐 `glass-none backdrop-blur-none bg-card` 降级，不能把扩展 Card 覆盖宣称为 Apple 官方层级规范。

采样纹理已在线性光空间中；材质与底色在线性光空间合成后才转换回现有绘制路径的显示颜色，避免二次 sRGB 解码造成发暗、脏色和层叠后失真。透明/尚未可用的捕获不参与颜色权重，输出按合成 alpha 归一化，保留原面板底色与透明度，而不是闪出黑色或降低透明度的边缘。透镜采用连续平滑的边缘法线截面；无折射率/零厚度、平坦中心和圆角裁剪都有 GPU 回归断言。

`surface-opacity = 1` 同时关闭自动玻璃与 backdrop blur，提供完整的不透明降级；当前不会自动跟随操作系统“减少透明度/增强对比度”设置，应用需自行安装覆盖。`surface-opacity` 保留真实 alpha，不使用语义色 `/N` 的不透明预混行为。每个玻璃绘制带保有独立的局部背景采样，依序包含更低层的玻璃；非模态光学面板不触发全屏模糊。当前玻璃可见时采用完整合成，避免局部节点重绘丢失背景依赖；采样与滤波仍限制在材质边界及其光学/滤波支撑范围。其余预设保留原有增量呈现路径。

```text
cargo test -p jadix --lib style::theme -- --test-threads=1
cargo test -p jadix --test liquid_glass -- --test-threads=1
cargo test -p gallery theme_section -- --test-threads=1
```

GPU 测试分别比较无材质、折射率 1、折射、色散和高光输出，并断言四边的色散、双轴渐变标尺的采样位移方向与幅度、两轴圆角弯曲、统一扩散/染色，以及捕获不可用时的底色与 alpha 不变，而不是仅比较半透明颜色。可通过 `JADIX_GLASS_EVIDENCE` 指定输出目录保存像素证据。无 GPU adapter 时该测试不执行，不能据此宣称验证了 GPU 输出。

Liquid Glass 在 SidebarMenu、TabsList、ButtonGroup、ToggleGroup 上使用一个共享玻璃指示器，通过现有 LayoutMotion 弹簧从当前矩形流向新悬停项/选中项；中途重定向和响应式树重建保留当前位置与速度，不移动或缩放标签。多选组保留每一个选中项，悬停指示器不会吞掉其他选择。独立控件及前景颜色使用 180ms 过渡。遵循减少动态效果设置；这仍是跨平台共享轮廓近似，不宣称原生玻璃融合/分裂与光学算法等价。

颜色过渡的中途重定向使用与正常呈现相同的感知颜色插值，避免用线性分量重建起点而瞬间变暗。连体组的分隔条跟随相邻悬停/选中背景；分隔条更新同时失效自身绘制缓存，并以整组 chrome 为增量重绘单元，防止小数 DPI 的 1px 接缝残留。竖组的外框圆角按端部单项的半尺寸约束，不把 `9999px` 胶囊值按整个组高度重新解释。

Dialog 的进入动画在最后一次 CSS tick 内完成姿态归一化、清理关键帧后再收集绘制，避免在缩放捕获层和实时文字层之间隔一帧切换。该修复通过真实 AppState / Windows GPU 表面比较，不仅检查纯 SDF 背景。

## 2026-10-21 验收记录

以下为先前迭代的历史证据；其中“五种预设”“清晰边缘/柔化内部”的实现已由四种预设和统一扩散材质取代，不代表当前实现或本轮验收结果。

环境：Windows NT 10.0.26300、Rust 1.95.0、原生 winit / wgpu DX12、144 DPI（150%）。Gallery 使用 `--no-default-features`，包含 SVG、Lucide 与文本编辑；本次不覆盖可选 WebView 和 performance 功能。

| 验证 | 结果 | 证据（仓库根目录相对路径） |
| --- | --- | --- |
| Jadix 全库 | 3,997 passed / 0 failed / 78 ignored | `.theme-optimized-library-final.log` |
| Gallery 主题分区 | 3 passed，覆盖五种预设、两种模式、表单状态保留、内容层与光学演示隔离、不透明降级 | `.theme-optimized-gallery-theme.log` |
| GPU 光学集成 | 1 passed；本轮生成四张 GPU 输出，不是无 adapter 跳过 | `.theme-optimized-gpu.log`、`.theme-evidence/gpu-20261021/` |
| 原生主视图 | 五种预设 × Light/Dark，逐图检查并核对 canvas 像素，2338×1374 物理像素 | `.theme-evidence/native-20261021/theme-matrix.png`、`metrics.json` |
| 原生交互 | 玻璃 Popover 中英文输入与焦点、明暗 Dialog、关闭按钮与 Escape、参数调节、不透明降级、978×844 窄窗口重排 | 同目录 `text.png`、`glass-dialog-*.png`、`glass-popover-dark.png`、`glass-opaque-fallback.png`、`glass-narrow.png` |
| 格式与差异 | 本轮 Rust 文件 rustfmt check、`git diff --check` 通过 | 命令输出 |

非模态玻璃打开前后，指定左侧背景区域 385,000 个像素全部不变，排除该区域被全屏模糊或污染。原生 Popover 区域的折射率与色散调整均产生非零像素变化；这些是局部观察，不单独证明 Apple 算法等价。GPU 测试独立隔离光学参数；精确变化量、区域、每张截图和验收可执行文件 SHA-256 见 `metrics.json`。

本轮原生检查还发现并修复浅色模态遮罩发白：四种新预设的中性/语义模态默认使用黑色 scrim，保留显式颜色与 opacity 覆盖，shadcn 原行为不变。公共遮罩入口统一复用同一规则，覆盖 Dialog、Sheet、Drawer 和 CommandDialog，不只修 Gallery 演示。遮罩 alpha 仍采用 Jadix 的 0.25 工程默认，不冒称各平台统一官方值。

最终复现命令（PowerShell 中设置 `CARGO_PROFILE_DEV_DEBUG=0`、`CARGO_PROFILE_TEST_DEBUG=0`）：

```text
cargo test -p jadix --lib --no-default-features --features backend-dx12,bundled-lucide,lucide-animated,svg,text-undo --config profile.dev.package.jadix.opt-level=0 -- --test-threads=1
cargo test -p gallery --no-default-features --config profile.dev.package.jadix.opt-level=0 theme_section -- --test-threads=1
cargo test -p jadix --test liquid_glass --no-default-features --features backend-dx12,bundled-lucide,lucide-animated,svg,text-undo --config profile.dev.package.jadix.opt-level=0 -- --test-threads=1
cargo build -p gallery --no-default-features --config profile.dev.package.jadix.opt-level=0
```

**未通过项与边界：** Gallery 全量测试为 69 passed / 4 failed / 1 ignored（`.theme-optimized-gallery.log`）。失败为 `forms_section_does_not_crush_sidebar_width`、`gallery_main_pane_uses_remaining_width`、`gallery_sidebar_collapses_to_icon_rail`、`headless_mount_gallery_app_does_not_overflow`，均报告首页静态根树缺少内容或 sidebar；最后一项单独执行也失败，见 `.theme-gallery-isolated-navigation.log`。未将这些失败视为通过，也未顺带修改导航结构或 motion 工作。

本次不构成所有 Gallery 页面、所有控件交互、所有 DPI 或目标平台的穷尽验收；未实测 Apple 原生窗口或 WinUI 原生控件。系统减少透明度/增强对比度自动接入、完整 MD3 Expressive、真实系统 Acrylic/Mica 与 Apple 自适应材质仍不在已验证范围内。构建产物清理使用工作区 `cargo clean -p jadix -p gallery`；源码、锁文件、日志及 target 外的像素证据均保留。

## 容器与光学复验（2026-10-21）

本轮针对父级容器仍沿用旧造型、内容背景偏蓝、透镜不明显和打开时暗帧的反馈继续修正：

- 页壳与导航获得独立 slot 造型和背景角色，移除非 shadcn 页壳的通用投影；补齐导航项、Tabs 和菜单选项圆角，保留显式调用方覆盖与窄屏规则。
- Liquid Glass 改为中性内容色，Popover 使用 24px 圆角和更连续的凸面截面；普通内容卡保持实色，消除演示中的 glass-on-glass。
- 修复捕获纹理二次 sRGB 解码，以及透明捕获未按 alpha 权重归一化的暗帧路径。两者分别有先失败、修正后通过的 GPU 断言；不把未复现的整段原生闪烁归因全部归到同一个问题。
- 分离清晰 scene 与滤波背景：外侧边缘保留真实细线/图案进行折射与色散，向内部平滑扩散为柔化背景。新增的“清晰边缘/柔化内部”测试在只采样预模糊纹理时失败，双源实现后通过。
- 光学网格仅裁剪背景绘制，不裁剪 Portal 浮层；非交互背景不参与指针命中。
- 窄窗口复验另定位到侧栏的未打开模态层误关闭新 Popover：仅在模态层实际离栈时清理其独占浮层，未打开的模态层不再影响 Menu/Popover/Tooltip。新增回归先失败后通过，原有真实模态关闭联动保持通过；这不是折射采样问题。

最终窗口验收使用 Windows 自绘 Gallery、150% DPI、2338×1374 和 978×844 客户区。证据位于 `.theme-evidence/theme-final-20261021/`，GPU 光学小场景与原生逐帧快照分别位于 `gpu/`、`native/`；全部在 Cargo target 外保留。验收副本与最终 `target/debug/gallery.exe` 的 SHA-256 相同：`66a7950b843d1f55454f15f5d00c7e53a812a5623fffe735777c8c9b7fb90239`。

| 验证 | 本轮结果 | 证据 |
| --- | --- | --- |
| Jadix 全库 | 3,999 passed / 0 failed / 79 ignored | `.theme-glass-library-final.log` |
| 浮层栈 | 44 passed，覆盖未打开模态层不干扰浮层与真实模态关闭联动 | `.theme-glass-overlay-final.log` |
| 真实 GPU 光学集成 | 4 passed：参数隔离、渐变标尺/双轴圆角、清晰边缘/柔化内部、无捕获降级 | `.theme-glass-gpu-final.log` |
| 原生逐帧呈现 | 明暗模式共 200 帧，包含两轮打开/悬停/关闭及未打开模态层同步；与同进度完整合成比对，单通道容差 2/255，无超阈值像素 | `.theme-glass-native-frames-final.log` |
| 实际屏幕连续采样 | 明暗打开与焦点/鼠标移动四组，加窄屏打开一组，各 90 帧，共 450 帧；稳定背景区域均无变化，未检测到浮层已出现后退回无效帧 | `temporal-*/metrics.json` |
| 四种风格明暗 | 8 个原生状态，客户区尺寸与页面背景像素断言通过 | `theme-surface-matrix.png`、`metrics.json` |
| 不透明/窄屏 | 不透明回退正常；窄屏背景网格不外溢，滚动后可打开光学浮层，Portal 不受背景网格裁剪 | `glass-opaque-popover.png`、`glass-narrow-popover.png` |
| Gallery 主题挂载/状态 | 3 passed，包含切换后表单状态与不透明降级 | `.theme-glass-gallery-final.log` |
| Gallery 全量 | 69 passed / 4 failed / 1 ignored；仍是前述四项首页/侧栏静态根树测试，未算作通过 | `.theme-glass-gallery-full-final.log` |
| 构建与格式 | Gallery build、样式/Pipeline/本轮测试/Gallery 文件 rustfmt check、正常仓库配置下 git diff --check 通过；`state.rs` 整文件检查仍有既有格式差异，未作无关重排 | `.theme-glass-build-final.log`、`.theme-glass-diff-check.log`、`.theme-glass-renderer-format.log` |

连续屏幕采样检查的是明确的浮层背景区域，不是所有显示帧的高速录屏；原生 GPU 回归则逐帧读取完整合成结果。两类证据不能互相替代，也不证明所有显卡、DPI 和操作系统均无闪烁。没有 macOS 27 原机对照，本轮仍是按当前 Apple HIG 改进的跨平台近似，未实现 Apple 全部动态亮度适应、系统交互形变和环境反射。

显式运行原生逐帧回归（Windows GPU；普通全库测试默认忽略此项）：

```text
cargo test -p jadix --lib --no-default-features --features backend-dx12,bundled-lucide,lucide-animated,svg,text-undo --config profile.dev.package.jadix.opt-level=0 optical_popover_open_hover_close_frames -- --ignored --nocapture --test-threads=1
```

## 分组、材质与过渡复验（2026-10-21，历史轮次）

本轮以整块透射表面统一扩散和染色，替代上一轮记录的“清晰边缘/柔化内部”透射方案。曲面折射、四边色散、放大与 Fresnel 表面反射仍保留；清晰场景纹理只服务独立的表面反射，避免明暗模式下出现两个材质层。

Fluent 与 WinUI 3 合并为一个 `fluent` 预设；旧名称作为兼容别名读取。Gallery 显示四种设计语言，并在主题页增加竖向 ButtonGroup、ToggleGroup 和 Tabs 验收区域。Fluent Card 保持 4px，MD3 Card 保持 12px，浮层和页壳仍分别使用各自的 slot 造型。

竖组的胶囊圆角现在按端部单项尺寸约束；相邻分隔条延续悬停或选中项背景，避免中性白线穿过状态层。一次原生回归还发现：鼠标进入已选中的 ToggleGroup 项时，颜色可以完全不变，但接缝抗锯齿仍需要重新合成。因此组级失效同时跟踪悬停项变化、接缝颜色变化和正在推进/刚结束的过渡，不仅检查颜色是否不同。该复现先失败，修复后通过完整矩阵；诊断日志代码已移除。

Liquid Glass 的四类交互 slot 增加可中断的 240ms 连续颜色过渡，重定向从当前画面续接。重定向与常规 tick 共用同一感知颜色采样，修复中途变暗/回跳；标签尺寸和布局不参与该状态动画。Dialog 在最后一个动画 tick 清理关键帧并归一化姿态，再收集文字，消除缩放层退场时的文字差帧。

| 验证 | 最终观察 | 本轮证据（仓库根目录下） |
| --- | --- | --- |
| Jadix 全库 | 4,005 passed / 0 failed / 80 ignored | `.theme-evidence/continuation-lib-verified.log` |
| 实际 GPU 光学 | 4 passed，覆盖统一扩散/染色、双轴曲面放大、四边色散和不可用捕获降级 | `.theme-evidence/continuation-gpu-tests-final.log` |
| 原生组控件逐帧 | 四预设 × 明暗 × 横竖 × ButtonGroup/ToggleGroup × 五个悬停阶段 × 16 帧，共 2,560 帧；对同进度完整合成比较，单通道容差 2/255，无超阈值像素 | `.theme-evidence/continuation-groups-verified.log` |
| 原生光学浮层逐帧 | Popover/Dialog 打开、悬停、关闭及输入焦点检查，共 200 帧；相同比较口径，无超阈值像素 | `.theme-evidence/continuation-optical-verified.log` |
| Gallery 主题 | 三项主题挂载/切换/降级测试通过 | `.theme-evidence/continuation-gallery-tests-final.log` |
| Gallery 全量 | 原始结果 69 passed / 5 failed / 1 ignored；四项既有根树/侧栏断言失败，另有一次性能阈值失败 | 同上 |
| Gallery 性能独立复跑 | 停止并发原生帧采样后，原性能测试通过；basics 最大窗口 paint-only 为 25.71ms，门槛 28ms | `.theme-evidence/continuation-gallery-perf-isolated.log` |
| 构建与格式 | 最新 Gallery build、涉及的主题/动画/分组/浮层/原生测试/Gallery 文件 rustfmt check、git diff --check 通过；utility.rs 整文件检查仍有其他响应式断点代码的格式差异，未作无关重排 | `.theme-evidence/continuation-gallery-build-final.log`、`continuation-format-scoped-verified.log`、`continuation-diff-verified.log` |

四项根树/侧栏失败为 `forms_section_does_not_crush_sidebar_width`、`gallery_main_pane_uses_remaining_width`、`gallery_sidebar_collapses_to_icon_rail`、`headless_mount_gallery_app_does_not_overflow`。未将失败计作通过；并发性能测试的原始失败也保留在全量日志中。

实际竖组采样见 `.theme-evidence/continuation-verified-frames/vertical-group-matrix.png`，包含 Fluent、MD3、Liquid Glass 明暗状态。原始 actual/reference PNG 在同一目录保留，测试可执行文件为 `.theme-evidence/continuation-verified-tests.exe`。最新 Gallery 二进制 SHA-256 为 `f9001860ef54bf3a28839382f82e244a064e50f6d90ec7dcec031f4946efc14c`。

本轮交互改进是连续状态层过渡；Apple 原生共享玻璃轮廓融合/分裂、系统自适应光照与 macOS 27 原机逐像素对照仍未完成。原生逐帧比较覆盖 Windows 150% DPI 的自绘路径，不等同于所有 DPI、显卡或操作系统的穷尽验收。

```text
cargo test -p jadix --lib --no-default-features --features backend-dx12,bundled-lucide,lucide-animated,svg,text-undo --config profile.dev.package.jadix.opt-level=0 themed_group_hover_text_frames_match_full_composites -- --ignored --nocapture --test-threads=1
```

可设置 `JADIX_GLASS_EVIDENCE` 保留像素 PNG；`JADIX_GROUP_FRAME_PRESET=liquid-glass` 可单独复验玻璃，完整矩阵需移除该环境变量。无效或空的预设过滤器会因零采样断言失败，避免将未运行误报为通过。

## 当前修复与复验（2026-10-21）

- Avatar fallback 保留 Lucide glyph 叶节点，避免被通用文字包装器重新布局；sm/default/lg 检查两轴居中。
- 共享玻璃指示器复用现有动画引擎，按运动速度有限拉伸，再回弹到目标；重定向延续当前矩形与速度，树重建保留句柄，多选项不丢失选中背景。文字不参与缩放，键盘焦点环保持独立，指示器遵守祖先滚动裁剪。
- 材质的整体透明度独立于 tint；关闭时背景捕获、边缘和文字同步淡出至零，不再由仍不透明的捕获层在卸载时突然消失。透射和反射使用相同扩散背景。
- 统一入口仍是 `ThemeStyles`：控件/面板尺度、gap、透明度、blur 和 `GlassMaterial`，可通过 TOML 通用/明暗覆盖及组件 slot 覆盖；主题页新增 blur、厚度、高光和表面透明度控制。
- Gallery 使用 `WindowOptions::new().follow_theme(true)`，预先创建支持 alpha 的呈现表面，运行时换风格不重建交换链。默认关闭此选项，保留应用显式窗口配置。Fluent 请求 Mica，Liquid Glass 在 Windows 请求 Acrylic；Windows 光学折射仍由组件 shader 模拟，不是系统原生 Liquid Glass。
- Windows D2D 效果改用 windows-rs SDK 常量，修复手写错误 GUID；两个 blend 节点使用不同名称，消除 `Unsupported effect type` / `Duplicate effect name` 导致的错误降级。直接创建明暗 Mica/Acrylic 配方和窗口切换均通过，不只是检查回退分支。
- macOS `native-materials` 特性运行时检测公开 `NSGlassEffectView`，可用时在 GPU 视图下安装原生材质；不可用则回退到 winit vibrancy。其他平台使用可用原生 blur 和自绘组件光学近似。macOS 模块已做 aarch64 独立类型检查，但当前 Windows 主机没有完成 macOS 原机运行验收；完整跨编译受 `psm` 的宿主汇编工具链限制。

| 检查 | 当前结果 | 证据 |
| --- | --- | --- |
| Jadix 全库 | 4,021 passed / 0 failed / 83 ignored | `.theme-evidence/round2-lib-final.log` |
| 实际 DX12 光学 | 6 passed：四边放大/色散、统一扩散/染色、反射扩散、全材质连续淡出、空捕获降级 | `.theme-evidence/round2-gpu-final.log` |
| 四主题组控件 | 2,560 帧，对同进度完整合成，单通道差异容差 2/255，无超阈值像素 | `.theme-evidence/round2-all-groups-final.log` |
| 液态拉伸补验 | 640 帧，无超阈值像素；四类容器、两方向、中途重定向与文字不动另有单元断言 | `.theme-evidence/round2-stretch-groups.log`、`round2-flow-final.log` |
| 原生浮层 | Popover/Dialog 打开、悬停、关闭 200 帧，无超阈值差异 | `.theme-evidence/round2-optical-native.log` |
| Gallery 首帧 | 实际 Fluent/Glass 主题按钮和运动卡片文字有可见像素，首帧位置与完整重绘一致 | `.theme-evidence/round2-first-pixels-final.log` |
| Windows 材质 | 明暗 Mica/Acrylic 配方直接创建通过；四预设明暗切换保留同一 composition surface，无配方降级日志 | `.theme-evidence/round2-window-recipes-final.log`、`round2-window-native-final.log` |
| macOS 模块 | aarch64 独立类型检查通过，不代表完整构建或原机视觉验证 | `.theme-evidence/round2-macos-isolated-final.log` |
| Gallery 全量 | 71 passed / 4 failed / 1 ignored；失败仍是历史根树/侧栏四项，未计作通过 | `.theme-evidence/round2-gallery-final.log` |
| 最新 Gallery 构建 | 通过；保留运行中程序，另行输出 `.theme-evidence/gallery-theme-verified.exe` | `.theme-evidence/round2-gallery-build-isolated-output.log` |

原始截图在 `.theme-evidence/round2-native/` 与 `round2-native-final/`。以上差分测试只证明采样时增量/完整合成一致；连续流动、可见文字、渐隐 alpha、材质扩散另有独立断言，不用两张错误但相同的图代替语义验收。扩展 Card 覆盖为项目取舍，不冒称 Apple HIG 要求；原生融合/分裂、系统自适应照明与 macOS 逐像素等价仍不在完成声明内。
