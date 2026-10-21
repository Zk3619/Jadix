# DevTools 与热重载

Gallery 的 debug 构建默认启用 `devtools`、`hot-reload`。Release 构建通过 `debug_assertions` 编译门禁排除 DevTools 面板、调试快捷键、源码编辑和热重载客户端；即便传入同名 features 或环境变量也不会启用。运行 `cargo run -p gallery`，F12 打开/关闭，Ctrl+Shift+C 点选主窗元素。

## 停靠与独立窗口

- 拖动面板左边缘调整宽度；减号/加号也可调整。弹出窗口图标切换至独立原生次窗，再次点击停靠图标回到主窗右侧。
- 独立面板复用主窗树快照、选中节点、Styles、Sources、Changes，不检查 Inspector 自身。关闭独立窗口或在其中按 F12 会关闭 DevTools；在主窗按 F12 重新打开停靠面板。
- DevTools 次窗随主窗退出。停靠改变会同步布局节点样式并完整重绘，主窗立即收回被占用区域，不必调整原生窗口大小。
- 启动前设置 `JADIX_DEVTOOLS_OPEN=1` 可自动打开停靠面板。

## 热更新与源码保存

在仓库根目录分别运行 `cargo run -p gallery` 与 `cargo run -p jadix_hot_reload_cli -- .`。监听器和 Gallery 的 `JADIX_HOT_RELOAD_ADDR` 必须一致。

静态 UI 文本及受支持的静态样式变更可即时注入；Rust 逻辑、结构性或不支持的模板修改仍需重新构建，不承诺任意 Rust 代码热替换。Changes 显示更新和注入结果。

Apply CSS 只编辑运行中的树，不写磁盘。Sources 默认只读，需显式启用源码写入后保存。真实保存仅限 debug + hot-reload，限制到工作区内不超过 512 KiB 的 Rust 文件，并校验磁盘内容未被外部修改。`JADIX_DEVTOOLS_ROOT` 可指定工作区根目录。启用监听器后，保存由同一热重载链路处理。

Gallery「导航 / 窗口」第一张卡片验证启动参数、双向消息和关闭返回值，API 见 [desktop-windows.md](desktop-windows.md)。

## 设备预览与界面语言

标题左侧的屏幕图标切换设备预览。可选择 iPhone SE、iPhone 16、Pixel 9、平板、1440×900 桌面或 Full HD，输入宽高后点击应用；支持旋转及适应/25%/50%/75%/100%/150% 缩放。

预览直接改变原 UI 的逻辑布局尺寸，再使用框架的视觉缩放及命中测试，不是截图。右侧检查器检查同一棵真实元素树。关闭 DevTools（包括独立次窗关闭和 F12）同步关闭预览并恢复原布局；这不是操作系统、触摸能力或浏览器 UA 模拟。

预览右侧、底部和右下角的拖柄分别调整宽度、高度及两个维度；拖动使用指针捕获和逻辑像素，超出拖柄也可继续调整。默认 Portal（下拉、提示、弹出框和模态层）挂到预览内的独立宿主，继承缩放和裁剪；已有显式 `mount` 保持原有局部挂载。预览开关会迁移已打开的默认浮层，关闭后恢复根叠层。

标题栏固定尺寸的 EN/中按钮切换语言。DevTools 使用 `jadix.devtools.*` 翻译键和框架 `tr_default_zh`，完整 i18n feature 下可由应用目录覆盖。未启用该 feature 时，`jadix::set_chrome_locale("zh-CN")` / `"en-US"` 和 `JADIX_LOCALE` 仍支持轻量中英组件文案。Gallery 默认中文。

匿名节点原来显示 `<node 0,0,…>`，是没有 data-slot、role、focus_id 或源码标记的布局/内部包装节点，不代表渲染错误。现在优先显示语义角色，并以 `hstack`、`vstack`、`box`、`text` 回退；定位路径仍保留在属性面板。

## 监听与重建托管

默认通过 Cargo 定位整个 workspace，不再限定 crates/examples。默认跳过 target、.git、.tmp、.codegraph、node_modules、reference。旧式 `ROOT ADDR` 参数继续支持。

`cargo run -p jadix_hot_reload_cli -- --root . --watch examples/gallery --watch crates/jadix/src --exclude crates/jadix/src/platform`

`--watch` 和 `--exclude` 可重复，路径相对 root，监听路径必须在 root 内。也可用 `JADIX_HOT_RELOAD_ROOT`、`JADIX_HOT_RELOAD_WATCH`（平台路径列表分隔符）和 `JADIX_HOT_RELOAD_ADDR` 配置。

要自动退出、重新构建并启动 Gallery，只运行以下命令，不再另外启动 Gallery：

`cargo run -p jadix_hot_reload_cli -- --run -- cargo run -p gallery`

托管器对 Rust token 结构分类：支持的静态 UI 文本、直接样式、animation/transition 属性走实时补丁；Rust 逻辑、动态表达式、组件结构/非样式属性、class 列表、Cargo.toml/Cargo.lock 变化停止它自己启动的进程树，再运行命令。class 列表按重建处理，避免删除类名后旧样式残留。编译失败保留错误输出，后续修改可重新触发。仅 `--run` 或 TOML 的 `run` 托管模式才拥有停止它所启动进程的能力。动画 keyframes/复杂程序化动画或其他未支持变更仍需 Rust 重建，不承诺任意 Rust 逻辑热替换。


## TOML 监听配置

监听器自动读取 Cargo 工作区根目录的 `jadix.dev.toml`，也可显式指定：

`cargo run -p jadix_hot_reload_cli -- --config path/to/dev.toml`

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

Styles 的 `element.style` 按属性列出名称、颜色预览和可编辑值，行尾确认按钮应用单个属性；Declarations 支持批量声明。CSSOM 在这里是选中 Element 的运行时样式接口，不是全局样式表修改。相同 class 的兄弟节点和后续创建节点不受影响。匹配规则只读显示来源，Computed 按属性名排序显示计算值；修改不写入源码，重建后的路径级覆盖仅在本次调试会话保留。
