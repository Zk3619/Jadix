# 主题官方参考与原生窗口视觉验收

参考研究日期：2026-10-20；实现复核日期：2026-10-21。依据实际获取的官方正文、JSON 与源码，不依赖搜索摘要。第 4 节保留原研究交接协议，不代表已逐项通过；本轮执行结果单独记录在 THEMES.md 与原生截图证据中。

已检查 D:/、D:/Projects、D:/Projects/Rust、仓库根、crates、crates/jadix 的 AGENTS.md，未发现磁盘文件；遵循任务提供的 CodeGraph 与资源管理指引。研究时先查询 CodeGraph，再读取未索引的 THEMES.md。

## 结论

- 四种设计语言预设：shadcn 保留默认；fluent 统一 Fluent/Windows 的设计体系，WinUI 3 仅作为其 Windows 组件实现参考（旧配置名作为兼容别名）；material3 明确 baseline 子集；liquid-glass 明确跨平台自绘光学近似。均不因外观接近而成为官方原生组件。
- 主题不只是主色：字体、形状、表面、层级和状态应共同变化；保留用户覆盖，不把所有组件统一高度、圆角、阴影和透明度。
- Windows Reactor 是声明式 Rust/WinUI 3 原生控件库，当前官方文档标记为 preview，**不是设计系统**；设计依据仍应来自 Fluent/Windows 指南。[R1、W1]

## 1. Fluent Web 与 WinUI 3

| 维度 | Fluent 2 Web | Windows / WinUI 3 桌面 |
| --- | --- | --- |
| 字体 | Segoe UI；跨平台可用系统字体 | Segoe UI Variable；简体中文回退 Microsoft YaHei UI |
| 字号/绝对行高 | Body 1 14/20；Caption 1 12/16；Subtitle 2 16/22；Subtitle 1 20/26；Title 3 24/32；Title 2 28/36 | Caption 12/16；Body 14/20；Body large 18/24；Subtitle 20/28；Title 28/36；Large title 40/52；Display 68/92 |
| 字重 | 正文 Regular、强调/副标题 Semibold | 正文 400、强调/标题 600，不全局改成 500/Bold |
| 圆角 | 常见矩形 4；小于 32 的形状通常 2；尺度含 0/2/4/8/12/circle | 常驻控件 4；窗口、flyout、dialog 8；Tooltip 4；相接/贴边 0，最大化/贴靠窗口不强制圆角 |
| 阴影 | key + ambient 双层，shadow 2/4/8/16/28/64 按层级使用 | Windows 用描边代替 key shadow 勾边，不直接复制 Web 配方并称为 WinUI 阴影 |
| 状态方向 | 颜色指南概括为交互趋深，focus 主要通过加粗轮廓 | 同页明确 Windows 交互色方向相反、趋亮；具体控件和明暗模式仍需独立状态角色 |

依据：[F2–F5、W2、W3]。字号映射逻辑/有效像素后做 DPI 缩放，不混同物理像素、Apple pt、Android sp；不要求下载或分发 Segoe/SF 字体。

**颜色与材质：** 中性色组织画布、正文、次要文字和边界；品牌色用于主要操作/选中，语义色用于反馈，不大面积铺蓝。[F4]

- 建议角色：canvas、surface、surface-raised、foreground、foreground-muted、stroke、accent、accent-foreground，并配置 rest/hover/pressed/selected/disabled。这些是 Jadix 映射名，不是微软 token 原名。
- Solid 是常用实色内容；Mica 是桌面背景着色的**不透明**窗口基底，非实时透视；Acrylic 用于菜单/Popover 等临时轻关闭表面；Smoke 为模态下方半透明黑遮罩，不随明暗反转。[F6]
- 统一 fluent 的 Windows 参考：正文 14/20/400、强调标签 14/20/600、控件圆角 4、弹层 8、Tooltip 4、细描边。32 控件高度属于项目紧凑桌面取值，**不是全部 WinUI 控件统一官方高度**。
- Web 与 Windows 在状态色、字号 ramp、阴影上存在平台差异；这张表解释同一设计语言的不同实现，不再据此拆分两个预设。12 不是所有弹层的强制圆角，应按组件选取。
- focus 独立于 hover/selected；disabled 停止 hover/pressed，不把整个子树统一乘 0.5。选中/错误同时提供标记或文字，不能只靠颜色。普通文字对比度至少 4.5:1，大字至少 3:1。[F2、F4；状态组合为实现建议]

## 2. Material 3：当前规范与版本差异

当前官网已含 Expressive：30 种文字样式（15 baseline + 15 emphasized），十档圆角。Roboto 是默认基线；Roboto Flex 是可选表达手段，不是全部控件默认字体。2026 年文档还加入按语言脚本高度调整行高；中文回退/裁切需实测。[M1、M2]

以下 baseline 大小/行高/字重由 Google 官方 Material Web v0.192 离散 token 交叉核对，rem 按 16px 换算，不是当前完整 30 样式表。[M6]

| 角色 | large；medium；small（大小/行高/字重） | tracking（逻辑 px） |
| --- | --- | --- |
| Body | 16/24/400；14/20/400；12/16/400 | 0.5；0.25；0.4 |
| Label | 14/20/500；12/16/500；11/16/500 | 0.1；0.5；0.5 |
| Title | 22/28/400；16/24/500；14/20/500 | 0；0.15；0.1 |
| Headline | 32/40/400；28/36/400；24/32/400 | 0 |
| Display | 57/64/400；45/52/400；36/44/400 | -0.25；0；0 |

tracking 是拉丁字体起点，不盲目强加给 CJK。

- 当前圆角：none 0、extra-small 4、small 8、medium 12、large 16、large-increased 20、extra-large 28、extra-large-increased 32、extra-extra-large 48、full。full 表示完全圆化；当前按钮还有 round/square、尺寸与按压形变，不能概括成全部胶囊。[M2、M5]
- 本次 baseline 建议：正文 16/24、按钮标签 14/20/500、可见高度 40/full、Card 12、Dialog 28；后两者是项目映射建议，不宣称完整 Expressive 实现。可见盒与触摸命中区分离，小型按钮/图标按钮至少 48×48dp。[M5、M8]
- 分开 primary/onPrimary、primaryContainer/onPrimaryContainer，以及 secondary、tertiary、error 对应角色；紫色只是可选种子。正文 surface/onSurface，次要文字 onSurfaceVariant，outline/outlineVariant 区分强弱，scrim/shadow 各司其职。[M3]
- 保留 surfaceContainerLowest/Low/Container/High/Highest。官方示例：正文 surface、导航 surfaceContainer、elevated card/button surfaceContainerLow、basic dialog surfaceContainerHigh。明暗分别配色，不反转 RGB。[M3]
- 六级 elevation 0–5 是空间层级，传统实现对应 0/1/3/6/8/12dp，**不是 blur 半径**。优先表面色阶，必要时加阴影，避免全部 Card/按钮同一大阴影。[M4、M9]
- 当前状态层 hover 8%、focus 10%、press 10%、drag 16%；顺序 container → state layer → content，状态层用内容的 on-color。同一时刻仅一个状态层，不叠加 hover+press。selected/activated 改容器/内容角色，focus ring 独立，disabled 不继承交互层。[M7]
- **版本陷阱：** 官方 Material Web v0.192 focus/press 仍为 12%；其 filled button disabled 容器 onSurface×12%，文字/图标 onSurface×38%。标为旧版组件 token，不能冒充当前网站规范或全局 disabled-opacity。[M6、M8]

## 3. Liquid Glass：光学与适应性

**官方特征：** lensing 使背景光弯曲、塑形和聚集，产生透镜式折射与层次分离，不只是大半径模糊；材质外观与流动响应共同设计。底层内容变化时，阴影、染色和动态范围适应，必要时材质可独立切换明暗；展开成更大表面时，折射、阴影、光散射也随之变化。[A1、A4]

- **有限功能层：** 用于悬浮工具栏、导航、侧栏和关键操作；正文、表格、普通内容 Card、整窗背景保留内容层/标准材质。避免 glass-on-glass：玻璃上的按钮可用轻量填充、透明度和 vibrancy 区分，不每个再捕获折射背景。内容层 Slider/Toggle 的临时交互元素是官方说明的例外。[A2、A4]
- **regular 默认：** 模糊及亮度调整维护文字可读性，适合文字较多的 sidebar、popover、alert；滚动边缘效果弱化经过操作层下方的内容。固定 alpha 不能完整复现其适应能力。[A2、A3]
- **clear 限用：** 用于媒体等视觉丰富背景上的有限控件，不作为一般面板默认。当前 HIG 建议亮背景时考虑在后方加 35% 黑色 dimming；已足够暗或 AVKit 自带遮罩则不重复添加。35% 是这个场景的建议，不是所有玻璃的不透明度。[A2]
- **字体与形状：** 语义标签色、亮度保护、同心嵌套圆角优先。没有跨平台统一的“Liquid Glass 字号/圆角”；桌面系统字体 14/20、面板圆角 20–24 只能是 Jadix 工程起点，不称 Apple 官方 token。[A2、A3]
- **交互与无障碍：** hover/press 可改变局部高光/填充，focus 保持独立可见轮廓；选中用 tint/标记，禁用停止反馈。自绘路径主动支持减少透明度、增强对比、减少动态效果，回退到不透明语义 surface 并保留操作。系统标准组件的自动适配不等于自绘 shader 已具备适配。[A2、A3；具体状态映射为工程建议]

### 自绘光学建议，不是 Apple 算法

1. 玻璃前捕获已绘制背景，通过圆角形状/边缘法线产生折射采样；材质之后再画前景文字和图标，不能把文字一起扭曲。
2. 背景变化时边缘采样位移/明暗也应变化。仅透明白色、大 blur、静态描边或固定渐变不能证明光学效果。
3. Fresnel、镜面/边缘高光、色散和聚光可作自定义艺术控制，不宣称物理准确或 Apple 等效；所核实官方文档未提供相应的折射率、厚度、色散数值规范。现有 1.46/12/0.025/0.6 是项目参数，非 Apple 官方值。
4. 先验证动态背景可读性与不透明降级，再加视觉强度。普通正文至少 4.5:1 作为工程基线，对比度针对最终合成背景测量，不能只比较 tint 色值。[对比度采用 F2]

## 4. 原生窗口视觉验收交接

**本研究未启动应用、未截图、未执行视觉验收或构建。** 以下是给主代理的验收协议；源码/token 测试、浏览器效果图和渲染器静态图均不能单独替代正在运行的 Jadix 原生窗口证据。由于本任务仅允许写本文件，本研究不额外生成截图或测试文件。

| 验收项 | 操作与通过条件 |
| --- | --- |
| 五预设与明暗 | 在原生窗口逐一切换五预设及 light/dark；字体、角色层级、控件/面板曲率同步改变，内容/布局稳定，用户覆盖不丢失；不能只是品牌色换色 |
| 字体/DPI/CJK | 100%、150%、200% 缩放检查中英文、标签、标题、输入框与长文本；字号/行高保持逻辑尺度，无裁切、错基线、图标字体被正文覆盖；系统放大后仍可操作 |
| 瞬时/持久状态 | 鼠标 rest/hover/press/release；键盘 Tab/Shift+Tab 与 Enter/Space；selected 与 focus 可同时识别，disabled 不触发交互反馈，焦点轮廓不被裁切、不只依靠颜色 |
| 弹层 slot | 分别实际打开 Popover、HoverCard、Tooltip、Sheet、Drawer、Dialog、下拉/上下文菜单和 Select；检查内容面、标题、文字、描边、圆角、阴影都随预设变化，不只触发器改变 |
| slot 应用时序 | 主代理已发现缺失标记及 SlotChrome.apply 早于 dialog/menu slot 标记的问题：先完成 slot 标记再应用主题。此项是工程修复依据，不是官方设计规定；应以打开弹层后的最终像素验证 |
| 层级/模态 | 内容 → 临时浮层 → 模态遮罩/对话框层级正确；Popover/Menu/Tooltip 不被父级裁切，模态背景变暗且不再接受交互；阴影/表面色不跨主题混用 |
| 光学真实性 | 在棋盘格、细线、照片和滚动正文之上观察玻璃边缘；底图改变能驱动可见边缘折射，前景文字不被折射。固定高光/模糊只能算装饰，不能声称完成 lensing |
| 光学可读性 | 分别放在亮、暗、高频纹理背景上；regular 风格维护可读性，clear 风格仅限媒体小面积控件并有必要的 dimming；不把所有内容 Card 玻璃化，不玻璃叠玻璃 |
| 无障碍降级 | 减少动态效果时停止非必要形变；减少透明度/增强对比时使用可读实色与明确边界；若 Jadix 没有接通系统设置，记录缺口，不宣称原生自动适配 |

记录原生窗口版本/平台、缩放比例、预设/模式、具体控件/状态、观察结果；只在这些证据齐备后报告“通过”。Windows 自绘窗口通过该协议，仍不等于通过 Apple 原生 Liquid Glass 或原生 WinUI 控件一致性认证。

## 5. 已核实的一手来源

Material 官网为 SPA，已读取当前发布版 `2026-10-13_06-10-19` 的官方内容 JSON；旧版源码独立标记。GitHub main/master 后续可能变化。F/W/M/A/R 标识对应正文中的来源分组。

- [F1 Fluent UI 入口](https://developer.microsoft.com/en-us/fluentui#/)：已访问，具体决策以 Fluent 2 专题和 Windows 指南为准。
- [F2 Fluent 2 Typography](https://fluent2.microsoft.design/typography)：各平台 ramp、字体回退、文字对比度。
- [F3 Fluent 2 Shapes](https://fluent2.microsoft.design/shapes)：形状、圆角、stroke。
- [F4 Fluent 2 Color](https://fluent2.microsoft.design/color)：中性/品牌/语义色、交互状态及 Windows 方向差异。
- [F5 Fluent 2 Elevation](https://fluent2.microsoft.design/elevation)：双层阴影、六档 ramp、Windows 描边差异。
- [F6 Fluent 2 Material](https://fluent2.microsoft.design/material)：Solid、Mica、Acrylic、Smoke 的用途及透明性。
- [W1 WinUI 3 中文入口](https://learn.microsoft.com/zh-cn/windows/apps/winui/winui3/)：Windows App SDK 原生桌面 UI 框架。
- [W2 Windows Typography](https://learn.microsoft.com/en-us/windows/apps/design/style/typography)：Segoe UI Variable、有效像素、字重和中文字体建议。
- [W3 Windows Geometry](https://learn.microsoft.com/en-us/windows/apps/design/style/rounded-corner)：4/8/0 圆角及 Tooltip 等例外。
- [W4 WinUI 官方 Button 模板](https://github.com/microsoft/microsoft-ui-xaml/blob/main/controls/dev/CommonStyles/Button_themeresources.xaml)：2026-10-21 复核 Normal 字重、AccentFill 的 0.9/0.8 hover/pressed 透明度与独立禁用角色；[Common resources](https://github.com/microsoft/microsoft-ui-xaml/blob/main/controls/dev/CommonStyles/Common_themeresources_any.xaml) 的 XAML 颜色为 ARGB，Jadix hex 颜色为 RGBA，映射时已转换。
- [F7 Fluent UI light alias tokens](https://github.com/microsoft/fluentui/blob/master/packages/tokens/src/alias/lightColor.ts)、[dark alias tokens](https://github.com/microsoft/fluentui/blob/master/packages/tokens/src/alias/darkColor.ts)：2026-10-21 复核中性背景、品牌角色及 disabled 颜色。源码中 brand 参数的旧颜色注释不等于当前 Fluent Web 的完整品牌 ramp。
- [M0 Material 3 官网](https://m3.material.io/)；[M1 Typography](https://m3.material.io/styles/typography/overview)：30 样式、baseline/emphasized、默认字体与语言行高；[已读取的当前正文 JSON](https://m3.material.io/_dsm/content/m3/2026-10-13_06-10-19/f6a1d89a-1bbf-46d1-a177-f13b074897a5.json)。
- [M2 Shape](https://m3.material.io/styles/shape/corner-radius-scale)：十档圆角和 Expressive；[已读取的当前正文 JSON](https://m3.material.io/_dsm/content/m3/2026-10-13_06-10-19/5e04e8f6-7da6-475e-a1b0-3818021c0cea.json)。
- [M3 Color roles](https://m3.material.io/styles/color/roles)：角色配对及 containers；[已读取的当前正文 JSON](https://m3.material.io/_dsm/content/m3/2026-10-13_06-10-19/ced25aa3-a367-44c6-a47e-439bc9e59d68.json)。
- [M4 Elevation](https://m3.material.io/styles/elevation/overview)：六级空间、色阶优先；[已读取的当前正文 JSON](https://m3.material.io/_dsm/content/m3/2026-10-13_06-10-19/38354be9-206b-4988-96a8-0f2a88074511.json)。
- [M5 Buttons](https://m3.material.io/components/buttons/overview)：round/square、状态形变、40dp 基线与命中区；[已读取的当前正文 JSON](https://m3.material.io/_dsm/content/m3/2026-10-13_06-10-19/e31df68a-59d4-41dc-8743-8c48b476d4f8.json)。
- [M6 Google 官方 Material Web v0.192 Typography](https://github.com/material-components/material-web/blob/main/tokens/versions/v0_192/_md-sys-typescale.scss)、[Typeface](https://github.com/material-components/material-web/blob/main/tokens/versions/v0_192/_md-ref-typeface.scss)、[State](https://github.com/material-components/material-web/blob/main/tokens/versions/v0_192/_md-sys-state.scss)：基线离散值及旧版 12% 状态层。
- [M7 States](https://m3.material.io/foundations/interaction/states/overview)：单一状态层及 8/10/10/16%；[已读取的当前正文 JSON](https://m3.material.io/_dsm/content/m3/2026-10-13_06-10-19/b3339b30-4a6d-49b6-9a8b-8e581b2b1204.json)。
- [M8 Google 官方 v0.192 Filled button](https://github.com/material-components/material-web/blob/main/tokens/versions/v0_192/_md-comp-filled-button.scss)：40px/full、禁用容器 12%/内容 38%。
- [M9 Google 官方 v0.192 Elevation](https://github.com/material-components/material-web/blob/main/tokens/versions/v0_192/_md-sys-elevation.scss)：0/1/3/6/8/12dp。
- [A1 Liquid Glass 总览](https://developer.apple.com/documentation/technologyoverviews/liquid-glass)；[官方 JSON](https://developer.apple.com/tutorials/data/documentation/technologyoverviews/liquid-glass.json)。
- [A2 Apple HIG Materials](https://developer.apple.com/design/human-interface-guidelines/materials)；[官方 JSON](https://developer.apple.com/tutorials/data/design/human-interface-guidelines/materials.json)：有限功能层、regular/clear、亮背景 35% dimming、标准材质区别。
- [A3 Adopting Liquid Glass](https://developer.apple.com/documentation/technologyoverviews/adopting-liquid-glass)；[官方 JSON](https://developer.apple.com/tutorials/data/documentation/technologyoverviews/adopting-liquid-glass.json)：系统组件适配、滚动边缘、同心曲率、无障碍设置测试。
- [A4 Apple WWDC25 Meet Liquid Glass（文字稿）](https://developer.apple.com/videos/play/wwdc2025/219/)：lensing、折射/散射、动态阴影/染色、避免 glass-on-glass；clear 的使用细节优先采用当前 HIG。
- [A5 当前 macOS 产品页](https://www.apple.com/macos/)：2026-10-21 读取页面标题为 macOS 27 Golden Gate；本轮以当前 Materials HIG 和 Liquid Glass 技术总览复核功能层/内容层分离，而非把旧版毛玻璃或蓝色半透明卡当作 macOS 27 的等价实现。没有原生 macOS 27 设备截图的逐像素对照，不能宣称完全复刻系统光学算法。
- [R1 Microsoft windows-rs Windows Reactor](https://github.com/microsoft/windows-rs/blob/master/docs/crates/windows-reactor.md)：声明式 Rust/WinUI 3 原生控件库、preview 状态，不是设计规范。
- [A6 NSGlassEffectView](https://developer.apple.com/documentation/appkit/nsglasseffectview)；[已读取的官方 JSON](https://developer.apple.com/tutorials/data/documentation/appkit/nsglasseffectview.json)：公开类从 macOS 26.0 提供，包含 `contentView`、`cornerRadius`、`style`、`tintColor`、`effectIsInteractive`。当前通过运行时类检测选择原生路径，不把特定系统版本硬编码为可用性证明；Windows 主机只完成模块类型检查。
