# Jadix 框架能力与 Cargo feature 清单

本清单覆盖 `crates/jadix/Cargo.toml` 中所有已声明的门面 feature，区分**始终保留的核心**、
**默认启用但可以关闭的 feature**、**默认关闭的可选能力**与**兼容别名**。
“默认开启”以单独构建 `jadix` 为准，不包括示例、Gallery 或工作区 feature 合并后的结果。
平台支持与编译开关是两个条件；开启 feature 不代表当前操作系统、GPU 或驱动支持该能力。

异步与资源集成（默认关闭）：`runtime-tokio` 提供官方 TokioExecutor 适配，runtime 由应用持有；
`remote-asset-cache` 启用 remote-asset 与 Moka 字节缓存，需要显式安装 CachedRemoteAssetFetcher。
详见[异步架构](async-architecture.md)、[共享远程资源缓存](remote-assets.md)与[状态管理](state-management.md)。

## 包与能力边界

本页以当前 Cargo 清单为准，区分核心、默认能力、可选 feature 和兼容别名。独立包不等于可选依赖；安装和产品拆分见[迁移说明](migration.md)。

高层拖放的宿主输入、唤醒、布局和 collect 接入现通过 `utils::drag::runtime` 登记，内置 DnD 在产品侧实现适配。`drag::pointer`／`drag::routing`／`drag::runtime` 是实际接口；`drag` 控制基础拖动，高层产品已迁到独立 `jadix_dnd`；框架不再提供 `dnd` 开关，用法和限制见[组件作者接口](component-authoring.md#高层拖放的宿主接入)。

通用组件作者契约现位于 `jadix::authoring`；`jadix_components::components::base` 保留产品兼容重导出与条目协作状态。框架不再提供 `components` 开关；过程宏由 `macros` 可选开启，产品默认不依赖 UI 过程宏；用法与显式运行时路径见 [自定义组件作者接口](component-authoring.md)。

动画图标 API（含 `AnimatedIcon`、`animated_icon` 与配方查询）需显式启用 `jadix_lucide/animated`，该开关启用框架 `animation-base`；需要 SVG 动画绘制时同时启用 `svg`，路径形变另外启用 `jadix_lucide/animation-path`；普通 SVG 不隐式启用增强层。关闭配方时，普通 `Icon` 的 `hover_animation="animated"` 保留静态图标；CSS 悬停播放使用基础层，通用 IconProvider 不依赖 Lucide 配方。

`jadix_animation` 承载无 UI 依赖的共享 CSS 时钟、timing、spring/inertia、scalar player、timeline、stagger 与路径形变算法。`animation` 聚合 `animation-base` 与 `animation-ex`；`animation-path` 单独启用路径形变并带入 Jadix SVG 适配。默认显式保留 `animation-base`，延续普通 CSS/组件反馈；完全静态配置使用 `--no-default-features`，并核对其他依赖是否传递开启基础层。

未启用基础层时，命令式动画立即应用目标，CSS keyframes 使用结束姿态，presence 立即卸载；`prefers_reduced_motion()` 固定为真，不能通过运行时 false 恢复动画。静态参数/样式接口仍可用，手动 Timeline seek 不会启动动画帧。

`MorphIcon` / `morph_to` 由 `animation-path` 控制；Jadix 保留 Lucide 名称解析、SVG 输出与光栅宿主适配，路径解析与几何位于 `jadix_core::path`，采样/插值位于 `jadix_animation::path` 与 `jadix_animation::icon_morph`。CSS Style 的写回与 Element invalidation 仍由 Jadix 宿主驱动。

迁移时，原先只启用 `svg` 并依赖自动 morph 的应用需追加 `animation-path`；默认配置需要自动布局投影时也须追加增强层。原先使用 `animation` 的应用可继续使用聚合入口。

| 已 feature 化的能力组 | 已有开关（代表项，完整列表见后文） | 当前默认状态 | 仍保留在核心的相邻能力 |
| --- | --- | --- | --- |
| 主题预设提供者 | `theme-shadcn`、`theme-fluent`、`theme-material3`、`theme-liquid-glass` | 仅 Shadcn 预设默认启用 | 基础令牌、语义样式、slot 适配、自定义主题契约 |
| 代码编辑、语法高亮、语言服务 | `code`、`syntax-highlighting`、`code-lsp` | 默认关闭，可分别选择 | 普通文本编辑、输入、IME、多光标、文本模型 |
| Markdown 与文档 | `markdown`、`document`、`document-editing`、`document-diff`、`document-math`、`document-science` | 默认关闭，按依赖关系组合 | 普通文本显示与编辑；不要求 Markdown 解析器 |
| 基础/增强动画与预览 | `animation-base`、`animation-ex`、`animation`、`animation-path`、`scale-preview` | 仅基础层默认启用 | 共享时钟、普通变换与布局、静态图标及无动画完成契约 |
| GPU 后端与扩展绘制 | `backend-*`、`compute`、`scene3d` | DX12/GLES/Metal 默认启用；其余默认关闭 | wgpu 渲染管线、基础绘制与内部效果实现 |
| 系统窗口材质 | `window-materials`、`windows-dwm`、`windows-composition`、`windows-dcomp` | 系统材质聚合默认启用；自定义 DComp 默认关闭 | 窗口数据契约、事件循环、DPI、普通透明路径 |
| 平台集成 | `a11y`、`native-menu`、`system-tray`、`file-dialog`、`webview` | 默认关闭 | 系统字体与剪贴板接口、平台窗口基础 |
| 资源与服务 | 字体包、SVG/额外图片格式、`remote-asset`、`net`、`net-json`、`i18n` | 默认关闭；部分旧图片开关仅为兼容空开关 | 基础图片格式、资源/文件工具、媒体与任务契约 |
| 开发与诊断 | `devtools`、`hot-reload`、`performance`、`runtime-logging`、`tracing`、`gallery-tests` | 默认关闭；示例可自行启用 | 响应式运行时及正常呈现流程 |

“默认启用但可关闭”不是“始终属于不可裁剪核心”。例如默认窗口材质与 Shadcn 预设
已经 feature 化，而基础文本编辑和内部 GPU 效果尚未 feature 化。

## 始终保留的框架核心（不受 feature 裁剪）

| 能力 | 默认状态 | 保留范围／原因 | 不包含的可选能力 |
| --- | --- | --- | --- |
| 响应式与组件运行时 | 始终保留 | Signal、effect、owner、任务桥接、RSX/组件宏和 retained 元素树 | LSP 进程、网络客户端、热重载 |
| 布局与基础交互 | 始终保留 | 共享几何 `core::geometry::{Axis, Rect}`、Taffy、布局树、滚动、焦点、键盘、指针、基础虚拟列表 | 缩放预览组件及其局部 portal 作用域遍历 |
| 字体与基础文本 | 始终保留 | 字形测量与绘制、IME、选择、换行、普通输入协议、多光标和文本模型；产品 TextView／TextEditor 由 `components` 控制 | `CodeEditor`、Markdown、文档编辑器、语法高亮、LSP |
| 样式与基础组件 | 始终保留 | CSS/utility、语义令牌、authoring slot 适配、基础节点、Shadcn 基础 light/dark 令牌与自定义主题契约 | 可选主题提供者的配色与配置 |
| 动画兼容数据与静态完成 | 始终保留 | 样式声明、参数类型、静态目标、通用时钟/事件基础设施 | 基础层控制 UI 动画推进；增强层控制布局投影与 morph 播放；不等于全部动画类型/解析代码已从编译输入排除 |
| GPU/UI 渲染核心 | 始终保留 | wgpu/glyphon 桥接、绘制与命中、普通变换、裁剪、GPU 内部阴影/模糊/玻璃着色 | 桌面 DWM/DComp/macOS 原生窗口材质；内部 GPU 玻璃不等于系统窗口玻璃 |
| 窗口与平台基础 | 始终保留 | 多窗口、窗口选项数据、事件循环、DPI、普通透明/alpha 路径、系统字体与剪贴板接口 | 原生窗口材质实现、菜单、托盘、原生文件对话框、原生剪贴板实现 |
| 基础图片 | 始终保留 | PNG 解码、RGBA 图片原语与光标图片机制 | JPEG/GIF/WebP/ICO 及额外图片格式、SVG/resvg、网络下载实现 |
| 媒体与文件工具契约 | 始终保留 | 后端无关媒体播放接口、文本文件/资源工具、自动保存配置 | 内置音视频解码器或播放器后端（当前没有强制引入）、原生文件选择器 |

关闭所有 feature **不是无依赖或无 GPU 的 crate**：上述核心仍存在。后续拆 crate 时，
先拆模型、算法与系统桥接，再通过门面转发，避免以空开关冒充已经移出的核心能力。

## 默认配置与主题

| Feature | 默认开启 | 内容／依赖关系 | 关闭后的行为 |
| --- | --- | --- | --- |
| `product-tests` | 否 | 同源宿主测试桥，启用 drag 与宏以保留历史回归身份 | 不参与生产产品分发 |
| `file-persistence` | 否 | 窗口状态及编辑器等落盘能力 | 不写入持久化文件；内存状态仍保留 |
| `native-clipboard` | 是 | 可选 `jadix_clipboard` 及系统文本读写；快捷键和输入协议不随之裁剪 | 读取返回 None，写入不访问系统剪贴板；不维护隐式替代剪贴板 |
| `macros` | 是（兼容现有默认作者面） | 可选 `jadix_macros`、`ui!`、`#[component]`、`#[main]` 及原语的宏专用 Props | 纯框架关闭默认 features 后可用普通函数／Builder；独立产品通过各自的 `macros` feature 显式选择宏作者入口 |
| `drag` | 是（由组件间接开启） | 基础指针会话及 routing/runtime 宿主桥；不包含高层 DnD 实现 | 关闭后指针会话／注册不启动；选择与原生窗口拖动仍由核心输入处理 |
| `default` | 是 | `macros`、`backend-dx12`、`backend-gles`、`backend-metal`、`window-materials`、`theme-shadcn`、`animation-base`、`native-clipboard`、`images` | `default-features = false` 后由应用自行选择后端、剪贴板及图片格式 |
| `theme-shadcn` | 是 | 将 Shadcn 加入可选预设列表 | 基础令牌仍在核心；不再加入预设列表 |
| `theme-fluent` | 否 | Fluent 调色板、字体、尺寸与滚动条配置 | 不编译提供者，不允许选择该预设 |
| `theme-material3` | 否 | Material 3 调色板、字体、尺寸与焦点环配置 | 同上 |
| `theme-liquid-glass` | 否 | Liquid Glass 配色、组件 GPU 玻璃配置；不强制开启 Motion 或原生窗口材质 | 同上；与系统窗口材质是独立选择 |
| `themes` | 否 | 聚合以上四种主题 | 只保留调用方实际选择的主题 |

`ThemePreset::ALL` 只列出已启用预设，默认选列表中的第一个；完全未选择主题时使用
Shadcn 基础令牌。禁用的非 Shadcn 名称在 TOML 解析/安装时返回错误，直接构造或切换则 panic，
不先修改当前主题状态。枚举标识及共享 slot 适配仍留在核心。
Cargo 本地 feature 不能叫 `theme/shadcn`；本项目使用 `theme-shadcn`，将来可转发到主题依赖的 `shadcn`。

## 代码、Markdown、文档与语言服务

| Feature | 默认开启 | 内容／依赖关系 | 关闭后的行为 |
| --- | --- | --- | --- |
| `code` | 否 | `components::code_editor`、`CodeEditor`、代码编辑配方；复用核心 `TextEditor` | 不导出代码编辑器；普通文本编辑、行号与输入机制仍可用 |
| `syntax-highlighting` | 否 | 转发 `jadix_document/syntax`，框架适配为 TextSpan 并保留 UI 行缓存；独立于 `code` 和 `markdown` | 不引入 syntect；代码/文档块显示原文 |
| `code-lsp` | 否 | 可选独立 `jadix_lsp`：stdio JSON-RPC、子进程、UTF-16 位置与文本编辑转换；原 `text::lsp` 路径转发同一实现 | 不引入 LSP 服务包或配置字段；`code` 与高亮不会启动服务器 |
| `markdown` | 否 | pulldown-cmark、CommonMark `Document` 模型/富文本导入导出、轻量 block/inline helper、`Element::markdown_inline` | 不编译 Markdown 解析器及相关 API；普通 `TextDocument` 不受影响 |
| `document` | 否 | 启用 `markdown`；`DocumentView`、文档布局/高度索引/滚动锚点 | 仅开 `markdown` 可使用解析模型，不强制带入文档视图/编辑器 |
| `document-editing` | 否 | 启用 `document`；`DocumentEditor`、编辑状态与选择 | 不编译结构化文档编辑器 |
| `document-diff` | 否 | 转发 `jadix_document/diff`：similar、DiffDocument、DiffView；不依赖 Markdown | 不编译 diff 算法及视图 |
| `document-math` | 否 | `document` + `svg` + latex-rust | 公式使用诊断占位；基础 MathView 壳及化学文本排版仍在核心 |
| `document-science` | 否 | `document`；分子/蛋白模型与视图，不自动开启数学和 SVG | 文档分子内容使用诊断占位 |
| `text-undo` | 否 | 文本输入 undo/redo 历史 | 保留输入编辑，不保留该撤销历史能力 |

LSP 从 IDE 示例提取为可选传输模块；调用方提供消息回调，模块不依赖 UI Signal 或窗口宿主。
`jadix_ide` 显式启用 `code` 和 `code-lsp` 并负责 UI 线程分发和工作区交互，语言服务器可执行文件仍由用户提供。
代码编辑、语法高亮、Markdown 解析和 LSP 四者不是捆绑安装；只读文档无需代码编辑或语言服务器。
迁移时，原先直接调用 `text::syntax` 的程序需开启 `syntax-highlighting`；
原先使用 `CodeEditor`、`markdown_inline` 或 ScalePreview 的程序也需分别选择对应开关。

## 动画、预览与开发工具

| Feature | 默认开启 | 内容／依赖关系 | 关闭后的行为 |
| --- | --- | --- | --- |
| `animation-base` | 是 | CSS transition/keyframes、Spring/tween/inertia、数值 driver、Timeline 的基础播放与帧请求 | 基础播放直接结束到目标，不保留持续动画帧；兼容类型、解析和静态样式保留 |
| `animation-ex` | 否 | 启用基础层；Motion/MotionTarget、布局投影及 SVG morph 的自动播放 | 高层 Motion API 不编译；布局与图标切换直接应用目标 |
| `animation` | 否 | 基础层与增强层的兼容聚合入口 | 不单独决定是否存在基础动画，须核对其他直接/传递 feature |
| `animation-path` | 否 | 启用增强层；路径跟随/揭示/morph 预处理几何，启用 SVG 宿主适配 | 不编译高层路径动画模块 |
| `scale-preview` | 否 | ScalePreview 组件、逻辑视口配置、嵌套预览局部 portal 路由；release 也可使用 | 不导出组件/配置函数，不执行预览 portal 遍历；普通 scale/transform/响应式布局仍保留 |
| `devtools` | 否 | 启用 `scale-preview`；运行时 Inspector/设备预览，图标通过通用 provider 解析，无目录时使用 Unicode；UI 入口受 debug 构建限制 | 不加入 Inspector；独立 ScalePreview 不需要它 |
| `performance` | 否 | 性能采样及 Gallery 诊断所需采样 | 不加入可选遥测采样 |
| `hot-reload` | 否 | 开发期 TCP 热重载，还受 debug 构建限制 | 不加入该开发桥接 |
| `runtime-logging` | 否 | env_logger 控制台/可选文件 logger | 不安装该 logger，log 调用点仍可由应用接管 |
| `tracing` | 否 | 热路径结构化 tracing spans | 不加入该依赖与可选 spans |
| `gallery-tests` | 否 | 开发测试聚合：product-tests、text-undo、SVG、全部主题、Motion 路径、code、预览、DComp、文档编辑/diff/科学、动画图标、文件选择器 | 普通 SVG 或主题不会隐式编译 Gallery 场景；不建议产品应用开启 |

## GPU 后端与窗口材质

| Feature | 默认开启 | 内容／依赖关系 | 关闭后的行为 |
| --- | --- | --- | --- |
| `backend-dx12` | 是 | 仅选择 wgpu DX12 渲染后端 | 不启用 DX12；**不再隐式携带 DComp 材质配方** |
| `backend-gles` | 是 | 选择 wgpu GLES | 不启用 GLES |
| `backend-metal` | 是 | 选择 wgpu Metal | 不启用 Metal |
| `backend-vulkan` | 否 | 选择 wgpu Vulkan | 不启用 Vulkan |
| `backend-dx11-native` | 否 | 定制 wgpu-hal 原生 SM5/DX11，Windows 实验能力 | 不编译该 HAL 路径 |
| `backend-dx11-angle` | 否 | `backend-gles` + HAL gles-angle，Windows 实验能力 | 不启用 ANGLE 路径 |
| `backend-dx11` | 否，兼容别名 | 转发到 `backend-dx11-native` | 新代码使用规范名称 |
| `window-materials` | 是 | 跨平台原生窗口材质聚合：`windows-dwm`、macOS objc2/AppKit 桥接，其他平台按宿主支持处理 | 不启用该聚合；仍可单选 `windows-dwm` |
| `windows-dwm` | 是，经聚合 | Windows 系统 backdrop、DWM 帧扩展/穿透、AccentPolicy 回退 | 不编译框架 DWM/AccentPolicy 材质调用；普通窗口、IME、DPI、装饰和透明契约不受影响 |
| `windows-composition` | 否 | DX12 + Windows/WinRT/Composition 依赖边界，供 WebView 和 DComp 共享 | DX12 渲染本身不再因框架材质而引入这些 Composition API |
| `windows-dcomp` | 否 | `window-materials` + `windows-composition`；自定义 Acrylic/Mica/Aero 配方、DesktopWindowTarget/SurfaceHandle | 不编译自定义合成宿主和配方，不请求 DComp 交换链 |
| `native-materials` | 否，兼容别名 | 转发到 `window-materials` | 新代码使用规范名称 |
| `webview` | 否 | `windows-composition` + 平台 WebView 依赖 + HTML/资源桥接 | 不编译嵌入式 WebView；不是普通文档视图的前置条件 |
| `native-menu` | 否 | muda，Windows/macOS 原生菜单 | 不安装原生菜单桥接 |
| `system-tray` | 否 | tray-icon + 菜单依赖 | 不加入系统托盘 |
| `file-dialog` | 否 | 桌面 rfd | 保留路径信号，选择函数返回空结果并记录警告 |
| `a11y` | 否 | AccessKit 与平台无障碍桥接 | 基础语义属性仍可声明，不安装平台桥接 |
| `i18n` | 否 | ICU4X、目录与 locale overlay | 基础文案回退仍在核心，不加入 ICU4X |

DX11 两种实现可以单独或同时启用，统一名称不改变其实验性状态。
`jadix-frame-compare` 同时支持新名称与旧 `dx11`/`dx11-angle` 别名。
窗口材质选项数据仍可传递；关闭平台实现时不发出原生模糊请求。原生系统 API 也可能由 winit 等
传递依赖内部使用，不能把本框架开关解读成整个依赖图绝不出现 DWM。
**迁移注意**：默认系统窗口材质保留，但自定义 DComp 不再随 DX12 默认启用；需要原有自定义配方的应用加上 `windows-dcomp`。
AccessKit 的 Windows 依赖由 `a11y` 接管，不再把隐式依赖名 `accesskit_windows` 暴露为公共 feature。

## 资源、图片与扩展服务

| Feature | 默认开启 | 内容／依赖关系 | 关闭后的行为 |
| --- | --- | --- | --- |
| `bundled-cjk` | 否 | 内嵌 CJK 字体 | 可从磁盘加载，不内嵌该字体 |
| `bundled-cjk-serif` | 否 | 内嵌 CJK 衬线字体 | 同上 |
| `bundled-emoji` | 否 | 内嵌 emoji 字体 | 同上 |
| `bundled-symbols` | 否 | 内嵌符号字体 | 同上 |
| `bundled-fonts` | 否 | 聚合以上五项 | 仅保留所选内嵌字体 |
| `fonts-cjk` | 否 | 可选 CJK 字体磁盘加载 | 不启动该可选字体加载 |
| `fonts-cjk-serif` | 否 | 可选 CJK 衬线字体磁盘加载 | 同上 |
| `fonts-emoji` | 否 | 可选 emoji 字体磁盘加载 | 同上 |
| `svg` | 否 | resvg、SVG/SMIL；MorphIcon 另需 `animation-path`，基础路径几何在 `jadix_core` | 不加入 SVG 光栅与对应公开组件 |
| `emoji-svg` | 否 | 启用 `svg`；Twemoji SVG | 保留 PNG 回退 |
| `image-bmp` | 否 | image/bmp | 不启用额外 BMP 解码 |
| `image-tiff` | 否 | image/tiff | 不启用额外 TIFF 解码 |
| `image-avif` | 否 | image/avif | 不启用额外 AVIF 解码 |
| `image-exr` | 否 | image/exr | 不启用额外 EXR 解码 |
| `image-hdr` | 否 | image/hdr | 不启用额外 HDR 解码 |
| `image-pnm` | 否 | image/pnm | 不启用额外 PNM 解码 |
| `image-qoi` | 否 | image/qoi | 不启用额外 QOI 解码 |
| `image-tga` | 否 | image/tga | 不启用额外 TGA 解码 |
| `image-ff` | 否 | image/ff | 不启用额外 Farbfeld 解码 |
| `image-jpeg` | 是（由 images 开启） | image/jpeg 解码及质量可选编码 | 关闭时 encode_jpeg 返回 None |
| `image-gif` | 是（由 images 开启） | image/gif 全帧解码与动画 | 关闭时 GIF 动画解码返回 None |
| `image-webp` | 是（由 images 开启） | image/webp 解码与无损编码 | 不强制开启 GIF 或 JPEG |
| `image-ico` | 是（由 images 开启） | image/ico 解码 | CUR 热点解析与 PNG 路径仍保留 |
| `image-full` | 否 | 聚合 `images` 和九种额外图片格式 | 不影响 PNG；其他格式只按实际启用的 codec 依赖编译 |
| `jpeg` | 否，兼容别名 | 转发 `image-jpeg` | 不再由基础图片隐式开启 |
| `gif` | 否，兼容别名 | 转发 `image-gif` | 不再由基础图片隐式开启 |
| `webp` | 否，兼容别名 | 转发 `image-webp` | 不再由基础图片隐式开启 |
| `images` | 是 | 聚合 `image-jpeg`、`image-gif`、`image-webp`、`image-ico`，保留默认多格式兼容体验 | 精简框架仅保留 PNG/RGBA 路径；此项已不是空开关 |
| `cursor-images` | 否，兼容空开关 | 无额外能力 | 光标图片核心始终保留；不是裁剪开关 |
| `compute` | 否 | 可选 wgpu compute 效果，还需设备能力 | 使用已有非 compute 路径 |
| `scene3d` | 否 | 实验性 scene3d | 不编译该扩展 |
| `image-stream` | 否 | YUV→RGBA 帧流接口，无解码器依赖 | 不加入该帧流扩展 |
| `remote-asset` | 否 | 应用注入 RemoteAssetFetcher，框架不强制引入 HTTP 客户端 | 不启用远程图片加载桥接 |
| `remote-asset-cache` | 否 | `remote-asset` + Moka sync；显式安装 CachedRemoteAssetFetcher 后共享编码字节、合并同 URL 下载 | 不编译该缓存，也不安装 fetcher |
| `runtime-tokio` | 否 | 可选 jadix_tokio，适配应用持有的 Tokio Handle，异步/阻塞任务共享容量上限 | 不加入 Tokio 运行时适配 |
| `net` | 否 | 可选 jadix_net | 不加入网络客户端能力 |
| `net-json` | 否 | `net` + jadix_net/json | 不加入 JSON 网络便利 API |


## 配置组合与依赖边界

```toml
[dependencies]
jadix = { version = "0.1", default-features = false, features = [
    "backend-dx12", "theme-shadcn",
] }
```

| 用途 | 在所选后端/主题之外增加的 feature |
| --- | --- |
| 纯文本输入或只读文本（Builder） | 无 |
| RSX／组件属性宏作者层 | `macros`；关闭时普通 Builder 不受影响 |
| 代码编辑但不高亮 | `code` |
| 代码编辑和高亮 | `code`、`syntax-highlighting` |
| 带语言服务的 IDE | `code`、`code-lsp`，需要着色时再加 `syntax-highlighting` |
| 只解析 Markdown/保留源范围 | `markdown` |
| 只读文档 | `document`，代码块着色另加 `syntax-highlighting` |
| 可编辑文档 | `document-editing`，公式/科学另选 |
| 发布版布局预览 | `scale-preview`，无需 `devtools` |
| Windows 系统 Mica/Acrylic | `windows-dwm`（默认聚合已包含） |
| Windows 自定义 Aero/模糊配方 | `windows-dcomp` |

工作区依赖不替消费者选择后端；每个示例显式声明自身能力。
Cargo 同次构建会合并同一个包的 feature，因此精简依赖检查要使用 `-p jadix` 或独立应用，
不能拿 Gallery/整个工作区的依赖图代表最小配置。实际窗口仍需选择平台支持的 GPU 后端。

## Lucide 图标产品


`jadix_lucide` 是一个包，包含静态目录、字体、SVG 路径、UI 适配及动画配方；不需额外数据 crate。
默认 `ui,paths` 提供可直接安装的静态图标产品，不启用 UI 宏；调用 `jadix_lucide::install()` 注册目录与字体。
`animated`（兼容名称 `lucide-animated`）启用配方播放并依赖 `jadix/animation-base`；`svg` 另行启用光栅绘制；`macros` 仅增加 RSX 写法。
纯数据消费者可关闭默认 feature，按需选择 `font,paths`，不引入 UI 宿主。
这些是产品自己的 feature，不再是 `jadix` 的 feature。

`product-tests` 仅为框架内部历史产品回归提供同源测试桥，依赖 `drag,macros`；不恢复生产组件模块，也不是应用安装产品的入口。
