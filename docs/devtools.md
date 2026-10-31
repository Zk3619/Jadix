# DevTools 与热重载

Gallery 的 debug 构建默认启用 `devtools`、`hot-reload`。Release 构建通过 `debug_assertions` 编译门禁排除 DevTools 面板、调试快捷键、源码编辑和热重载客户端；即便传入同名 features 或环境变量也不会启用。运行 `cargo run -p gallery`，F12 打开/关闭，Ctrl+Shift+C 点选主窗元素。

## 停靠与独立窗口

- 拖动面板左边缘调整宽度；减号/加号也可调整。弹出窗口图标切换至独立原生次窗，再次点击停靠图标回到主窗右侧。
- 独立面板复用主窗树快照、选中节点、Styles、Sources、Changes，不检查 Inspector 自身。关闭独立窗口或在其中按 F12 会关闭 DevTools；在主窗按 F12 重新打开停靠面板。
- DevTools 次窗随主窗退出。停靠改变会同步布局节点样式并完整重绘，主窗立即收回被占用区域，不必调整原生窗口大小。
- 未启用设备预览时，主内容的响应式断点和 Sidebar 等组件读取的逻辑视口宽度扣除停靠面板占用的空间；调整窗口或面板宽度立即更新。Inspector 自身不继承主内容的断点作用域。关闭面板或切换独立窗口后恢复主窗可用宽度；设备预览开启时以设备的逻辑尺寸为准。
- 启动前设置 `JADIX_DEVTOOLS_OPEN=1` 可自动打开停靠面板。
- 元素树默认折叠，不再截断为“查看更多”；点击展开图标或双击节点文字可展开、收起分支。展开后保留完整滚动范围，大量节点仅挂载视口内及缓冲区的行。拾取和查找会展开目标的祖先，并按换行后的实际行高尽可能将目标居中；普通点选树行不改变树的滚动位置。查找使用最新树快照，页面切换后保留搜索框草稿，但不会继续匹配旧页面的节点。
- 停靠与独立窗口均支持元素树滚轮和滚动条拖拽；树列表可滚动时滚动条始终显示，不因鼠标离开而隐藏。左上角的元素拾取和设备工具栏按钮带中英提示，提示使用框架基础浮层，不依赖 `components`。

## 热更新与源码保存

在仓库根目录分别运行 `cargo run -p gallery` 与 `cargo run -p jadix_hot_reload --features cli --bin jadix-hot-reload -- .`。监听器和 Gallery 的 `JADIX_HOT_RELOAD_ADDR` 必须一致。

静态文字、受支持的属性增改删、class、动画参数及已登记有限结构可在 UI 安全点提交；Rust 执行逻辑、新捕获、未编译类型与未知结构要求重新构建。Changes 显示实际版本事务与提交结果，不把 TCP 发送成功作为 UI 已更新。



Inspector 已独立于 UI 宏：普通 Element／Builder 未登记热更描述也可检查树、拾取及样式/布局；缺来源明确不可用，只有实际登记才能授予模板更新能力。选择、hover、折叠和 CSS Apply 绑定运行时实例，提交 ACK 同步刷新路径，移除目标不会串到相似邻居。DevTools chrome 使用 Element／Builder，不依赖产品组件库，也不要求应用使用某个宏；未来 uix! 仅保留适配契约。


该方案的核心通过普通描述与登记接口接收更新，`ui!` 是当前优先改造的作者入口。宏不是热重载前提；无宏登记也不能让修改后的任意 Rust 函数体自动生效，Builder 源码热更范围未因此扩大。

Apply CSS 只编辑运行中的树，不写磁盘。Sources 默认只读，需显式启用源码写入后保存。真实保存仅限 debug + hot-reload，限制到工作区内不超过 512 KiB 的 Rust 文件，并校验磁盘内容未被外部修改。`JADIX_DEVTOOLS_ROOT` 可指定工作区根目录。启用监听器后，保存由同一热重载链路处理。

Styles 的 `element.style` 初始列表展示当前节点与框架基线不同的本地运行时属性，包含组件默认样式，但不是源码声明的逐字还原。继承的文字属性不会冒充本地声明；“继承的文字样式（只读）”单独列出祖先路径及属性值，Computed 展示最终解析值（含字体、字号、字重）。通过检查器显式提交的声明（包括默认值覆盖）及未提交草稿保持不变。

对齐属性的编辑框使用 CSS 值（如 `center`、`flex-start`、`space-between`、`safe center`），不会显示 `AlignContent { keyword: ..., safety: ... }` 等 Rust 调试结构。支持 `justify-content`、`align-content`、`align-items`、`align-self`、`justify-items`、`justify-self`；其中 `align-self` 和 `justify-self` 可用 `auto` 恢复父容器决定的对齐。单行对号与声明文本的 Apply CSS 使用同一解析和提交路径；不支持的对齐值会在 Styles 区域及 Changes 提示，拒绝本次提交并保留编辑草稿；改为受支持的值后再次提交，提示会清除。

当前布局后端未提供物理方向 `left` / `right` 对齐关键字；要沿当前轴靠起点或终点排列，请用 `flex-start` / `flex-end`。`justify-content` 控制容器内子项的主轴分布，`align-items` 控制交叉轴对齐；它们不会移动容器自身。内容恰好填满容器时，对齐变化可能没有可见位移：调整对齐时选择有空余空间的容器，或先增大其 `width` / `height`。

元素树中的 `<text>` 是含文字的底层 Element 的显示名称，不代表单独公开的 Text 组件，也不保证它是叶节点。`<Label>` 是带 14px / 500 字重及 foreground 颜色等默认值的产品组件；这些本地默认值可能覆盖父级对应的文字属性。未显式指定的文字属性从父级继承；组件内嵌文字也可能由组件直接设置字号、颜色等，因此不能仅凭 `<text>` 标签判断样式来源。布局、背景、边框等属性不因为容器嵌套而自动继承。

CSSOM 覆盖绑定框架分配的运行时实例。兼容协调且已登记节点类型、业务身份一致时保留实例；结构移动后按实例重新解析路径。匿名重建、类型替换、身份重复或目标消失使旧覆盖失效，保留编辑草稿，不选择邻近源码或相同尺寸的节点。提交按钮也校验实例；跨窗口相同路径不是同一目标。热更 ACK 的节点变更映射同时供树选择和 Changes 使用。

例如在 Styles 中应用 `color: #ff0000;` 只改变目标 Element 的运行时文字颜色，不会自动生成 `.text_color(...)`，也不会改写 `<Label color="...">`、组件默认值或 `styles.toml`。只有在 Sources 中显式修改并保存 Rust 文件，磁盘源码才会改变；随后由监听器判断能否热更，否则需重建。

Gallery「导航 / 窗口」第一张卡片验证启动参数、双向消息和关闭返回值，API 见 [desktop-windows.md](desktop-windows.md)。

## 设备预览与界面语言

标题左侧的屏幕图标切换设备预览。可选择 iPhone SE、iPhone 16、Pixel 9、平板、1440×900 桌面或 Full HD，输入宽高后点击应用；支持旋转及适应/25%/50%/75%/100%/150% 缩放。

元素树的长节点文字自动换行，续行与节点文字左侧对齐，展开图标保持在首行。树列表保留虚拟化，按换行后的行高定位、滚动和显示节点。双击节点文字可展开或收起节点。元素树没有横向滚动条；纵向滚动条位于视口内部，仅在内容可滚动时为其留出宽度，显示后保持可见。

预览直接改变原 UI 的逻辑布局尺寸，再使用框架的视觉缩放及命中测试，不是截图。右侧检查器检查同一棵真实元素树。关闭 DevTools（包括独立次窗关闭和 F12）同步关闭预览并恢复原布局；这不是操作系统、触摸能力或浏览器 UA 模拟。

普通应用和 release 构建可以直接使用组件 `scale_preview(width, height, scale)` 或 `<ScalePreview>`，无需启用 `devtools` feature。DevTools 与该组件复用 `core::scaled_viewport` 的逻辑视口、缩放及 Portal 坐标能力，示例见 [ScalePreview](../crates/jadix/SCALE_PREVIEW.md)。

预览右侧、底部和右下角的拖柄分别调整宽度、高度及两个维度；拖动使用指针捕获和逻辑像素，并限制在应用区剩余空间内。缩放比例超过可用空间时自动降低至适应尺寸。默认 Portal（下拉、提示、弹出框和模态层，包括性能面板）在 DevTools 打开时挂到应用区，预览时迁移到预览宿主，继承缩放和裁剪；已有显式 `mount` 保持原有局部挂载。关闭 DevTools 后恢复根叠层。

标题栏固定尺寸的 EN/中按钮切换语言。DevTools 使用 `jadix.devtools.*` 翻译键和框架 `tr_default_zh`，完整 i18n feature 下可由应用目录覆盖。未启用该 feature 时，`jadix::set_chrome_locale("zh-CN")` / `"en-US"` 和 `JADIX_LOCALE` 仍支持轻量中英组件文案。Gallery 默认中文。

匿名节点原来显示 `<node 0,0,…>`，是没有 data-slot、role、focus_id 或源码标记的布局/内部包装节点，不代表渲染错误。现在优先显示语义角色，并以 `hstack`、`vstack`、`box`、`text` 回退；定位路径仍保留在属性面板。

## 监听与重建托管

默认通过 Cargo 定位整个 workspace，不再限定 crates/examples。默认跳过 target、.git、.tmp、.codegraph、node_modules、reference。旧式 `ROOT ADDR` 参数继续支持。

`cargo run -p jadix_hot_reload --features cli --bin jadix-hot-reload -- --root . --watch examples/gallery --watch crates/jadix/src --exclude crates/jadix/src/platform`

`--watch` 和 `--exclude` 可重复，路径相对 root，监听路径必须在 root 内。也可用 `JADIX_HOT_RELOAD_ROOT`、`JADIX_HOT_RELOAD_WATCH`（平台路径列表分隔符）和 `JADIX_HOT_RELOAD_ADDR` 配置。

要自动退出、重新构建并启动 Gallery，只运行以下命令，不再另外启动 Gallery：

`cargo run -p jadix_hot_reload --features cli --bin jadix-hot-reload -- --run -- cargo run -p gallery`

托管器和 ui! 适配使用同一描述与能力签名。已登记的静态文本、支持的字面量属性增删、class 重算、animation/transition 参数和有限结构走版本化事务；属性删除恢复构造时声明基线。基础结构支持 Box、VStack、HStack、GStack、Text 及显式登记的无捕获工厂；不变 Rust 子树作为编译槽位移动，闭包不重复执行。新类型、未登记结构、改变闭包或捕获、Cargo.toml/Cargo.lock 仍要求重建。仅 `--run` 或 TOML 的 `run` 模式停止它启动的进程；非托管模式只报告需要手动重建。TCP 发送成功不代表已提交，Changes 展示 UI 安全点返回的 ACK。

`.keyframes.css` 提供数据化动画定义热更：相同定义保留时钟，改变定义重启，删除恢复基线；普通 CSS 文件走 stylesheet 路径。WGSL 和缓存中的本地图片按资源专用事务更新，解析或解码失败保留旧资源。当前一个事务只更新一个资源，资源、stylesheet 和模板混合批次明确拒绝。未挂载模板的 `Deferred` 仅表示缓存，挂载后仍须通过编译能力校验，不表示已呈现。Portal 内登记节点可按实例更新；改变 Portal 宿主、任意动态 child 列表或根容器类型仍要求重建。

纯 Builder 无来源／无登记也可使用 Inspector；来源栏显示不可用，不伪造 ui! 跳转。显式登记使用 `Element::hot_reload_description` 与 `jadix_hot_reload::TemplateDescription`，与 ui! 共用协议；声明基线和结构工厂需由作者明确提供。Debug 能力受编译门禁控制。验证需分别覆盖协议、真实文件保存到原生画面、状态保持及 Release 开销；单元或 TCP 成功不能替代原生窗口验证。


## TOML 监听配置

监听器自动读取 Cargo 工作区根目录的 `jadix.dev.toml`，也可显式指定：

`cargo run -p jadix_hot_reload --features cli --bin jadix-hot-reload -- --config path/to/dev.toml`

```toml
[hot-reload]
root = "."
addr = "127.0.0.1:5174"
watch = ["src", "ui/src"]
exclude = ["src/generated"]
# 可选：由监听器启动、重建并管理应用进程树。
run = ["cargo", "run", "-p", "my-app"]
```

`root` 相对配置文件所在目录（省略时也是该目录），`watch`/`exclude` 相对 root；拆分 crate 后将需要的路径放在同一 workspace 的 root 内。配置格式严格校验未知字段。优先级为 CLI > 环境变量 > TOML；排除项累加，CLI/环境变量 watch 列表替换 TOML 列表。命令以参数数组传入，不通过 shell 解析，运行目录为 root。不配置 run 时只监听，不启动或终止应用。

## 选中元素样式

Elements 以起始标签、结束标签和折叠省略形式展示树；图标显示语义名称，不将图标私用区字符交给代码字体。悬停只更新高亮，只有显式选择/点选才定位选中行。

查找按钮和搜索框 Enter 按文档顺序选择匹配标签、标识、文本或源码的节点；新关键词从首项开始，重复查找循环到下一项并展开祖先。点选或查找树节点时，若页面组件不在自身滚动视口内，将其竖直居中显示；设备预览将视觉坐标差转换为逻辑滚动距离，绘制和命中时再应用一次祖先缩放。ScrollArea 的滚动指标和偏移信号始终使用逻辑像素，MessageScroller 的末尾定位遵循同一约定。页面内容区的横向和纵向滚动条分别由对应方向的滚动或滚动条悬停触发显示，空闲后独立退场；局部重绘保留轨道在滑块下方的绘制顺序。

设备预览内 Bubble reaction、Avatar +N 等浮层在首次绘制前完成定位。定位预遍历沿用绘制阶段的祖先缩放、变换与滚动坐标；挂到预览内部宿主的浮层修改位置后，重算包含该宿主的布局树，并保留页面文字测量缓存。宽度折叠使用 Taffy 的百分比小数单位，避免 `max-width: 100%` 被再次除以 100。

输入法仅在实际绘制的光标叶完整位于裁剪区内时更新系统候选锚点。完整性判断使用文字缩放和变换后的视觉范围，避免缩小的输入框被误判为越界、候选窗停在先前的 DevTools 搜索框位置。

Styles 的 `element.style` 按属性列出名称、颜色预览和可编辑值，行尾确认按钮应用单个属性；Declarations 支持批量声明。CSSOM 在这里是选中 Element 的运行时样式接口，不是全局样式表修改。相同 class 的兄弟节点和后续创建节点不受影响。匹配规则只读显示来源，Computed 按属性名排序显示计算值；修改不写入源码，身份校验通过的覆盖仅在本次调试会话保留。
