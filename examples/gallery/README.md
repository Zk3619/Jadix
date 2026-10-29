# Jadix Gallery

Gallery 是框架与组件库的可交互展示应用。侧栏按用户要完成的工作分组，页面展示真实组件、状态和平台操作；测试位于 `src/mount_tests.rs`，不作为 Gallery 的内容目录。

## 启动与配置

```powershell
cargo run -p gallery
cargo run -p gallery --all-features
cargo run -p gallery --no-default-features
```

默认 `full` 启用 WebView、Inspector、热重载、性能面板、托盘、远程资源、3D、图像流、GPU compute、窗口材质、国际化、无障碍、图像格式、Tokio 与网络。文档编辑、Diff、科学文档、语法高亮、动画、主题、SVG、拖放、原生剪贴板和文件对话框由基础依赖启用。`--no-default-features` 关闭 Gallery 的可选集成，仍保留这些基础依赖。

`--all-features` 还编译 CPU benchmark 入口；通过环境变量 `JADIX_CPU_TELEMETRY` 显式启用。窗口材质、托盘、WebView 等依赖宿主平台支持。远程图片需要网络，页面也提供本地内容。

```powershell
$env:JADIX_GALLERY_SECTION = "documents" # 也可使用下表中的页面 key
$env:JADIX_DEVTOOLS_OPEN = "1"           # 启动时打开 Inspector
$env:JADIX_PERF_OVERLAY = "0"            # 隐藏性能浮层
cargo run -p gallery
```

Inspector 默认启用，F12 切换。排查宿主问题时可使用 `JADIX_DEVTOOLS=0`。环境变量只影响当前进程及其子进程，切换回默认配置时应移除对应变量。

## 展示目录

| 分组 | 页面 key | 展示内容 |
| --- | --- | --- |
| 组件库 | `basics`、`forms`、`feedback`、`data` | 基础控件、表单输入、消息反馈、表格图表与虚拟列表 |
| 交互与导航 | `navigation`、`overlays`、`events`、`drag` | 菜单路由、页面栈、浮层、事件传播、拖放排序 |
| 布局与样式 | `layout`、`responsive`、`style`、`transforms`、`text`、`theme` | 容器盒模型、断点、外观、变换、原生排版、主题设计系统 |
| 图形与媒体 | `draw`、`scene`、`media`、`motion` | 画布、矢量、3D、着色器、图像流、音视频与动画 |
| 文档与开发 | `documents`、`code` | Markdown 编辑阅读、公式、化学、Mermaid、Rust 编辑器、并排及统一 Diff |
| 桌面与运行时 | `desktop`、`workspace`、`reactivity`、`rendering` | 窗口会话、全屏、多实例、多窗口、托盘、Dock/WebView、状态作用域、异步边界、布局重绘 |
| Gallery | `about` | 当前构建配置、crate 目录和能力说明 |

侧栏与页面标题共享 `src/app.rs` 中的 `GALLERY_GROUPS` / `GALLERY_SECTIONS`。页面测试覆盖所有目录项在不同视口下的布局，分组扩充时应同步路由与测试中的页面构造。

各分组使用 Collapsible 构成可折叠的层级导航；点击或 Enter/Space 切换，分组聚焦后左右方向键收起/展开。切换到某分组内的页面会展开对应分支，选中页面与分组展开状态独立。

响应式页首次进入直接呈现最终布局；首帧成功呈现之后的断点和尺寸变化才触发布局过渡。进入动画需要单独显式设计，不借用首次布局测量的中间尺寸。

## Builder 与 `ui!`

页面使用两种写法共同构建同一棵 Element 树。`src/pages/documents.rs` 中，Builder 创建编辑器和动态模式按钮，`ui!` 组织卡片、操作栏和响应式分支；代码编辑器中的可编辑示例也展示这种组合。`src/app.rs` 中侧栏分组使用 `ui!` 生成，再交给 Builder 组合。

Markdown 支持实时预览、源码和阅读三种模式，并提供撤销、重做和重置。代码编辑器支持高亮、行号及选区编辑；Diff 按钮切换同一份文档的比较方式。代码文本不会执行。

`data` 页还展示可选的 Carousel 滚轮交互：`ui!` 使用 `wheel={true} wheelMode="discrete" wheelIntervalMs={220}` 逐项切换；Builder 使用 `.wheel(true).wheel_mode(CarouselWheelMode::Continuous).wheel_speed(1.25)` 按实际滚动位移连续推进。默认关闭滚轮接管；启用后仅在命中该 Carousel 且存在可滚动内容时消费事件。连续模式允许停在两项之间，方向键、导航按钮及拖拽仍可使用。

文档分层：`jadix_document` 管语义、源码位置和格式解析；`jadix` 管通用输入命中、IME、选区、事务和绘制失效；`jadix_components` 管实时预览策略、表格网格及编辑器外观；应用管主题、文件流程和偏好。表格配色、间距与类似 Typora 的视觉样式不进入 `jadix_document`，修饰键选字和旧选区擦除则应修在共享输入/绘制路径。

## crate 与实际实现边界

Gallery 直接依赖工作区运行库：`jadix`、`jadix_core`、`jadix_layout`、`jadix_renderer`、`jadix_components`、`jadix_dnd`、`jadix_lucide`、`jadix_animation`、`jadix_document`、`jadix_theme`、`jadix_style_tokens`、`jadix_media`、`jadix_net`、`jadix_tasks`、`jadix_tokio`、`jadix_window_state`、`jadix_window_materials`、`jadix_clipboard`、`jadix_hot_reload`、`jadix_lsp` 与 `jadix_macros`。宏在编译期使用，基础设施 crate 由对应组件及运行时路径使用，CLI 工具单独运行。

能力范围以 `development-docs/STATUS.md`、`development-docs/AUTHORITY.md`、各 crate 的代码和公开 feature 表为准。Jadix 仍为 Alpha；`jadix_lsp` 是实验性语言服务基础设施，Gallery 的代码编辑器不代表完整 IDE。`jadix_native` / `jadix_shell` 尚未实现，不列为可用后端。热重载支持静态内容、样式及受管重建；任意 Rust 逻辑修改仍需重新编译。

热重载监听单独启动：

```powershell
cargo run -p jadix_hot_reload_cli -- .
```

## 验证

```powershell
cargo test -p gallery
cargo check -p gallery --no-default-features
cargo check -p gallery --all-features
cargo test -p jadix --lib --features devtools app::navigator::tests
cargo test -p jadix --lib --features devtools primary_ui_rebuild_retains_inspector_and_mounted_navigation_page
```

原生呈现和系统集成需要在对应平台实际启动验证，不能由 headless 布局测试推导出所有平台均可用。
