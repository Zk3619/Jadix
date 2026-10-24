# Jadix Markdown

运行：`cargo run -p jadix_md`。与 Gallery 独立的原生应用。

- 打开、新建、保存、另存为 UTF-8 Markdown、纯文本或 HTML；文件在外部变化时拒绝覆盖。
- Markdown 实时源码切换、源码模式、阅读模式、大纲、查找、粗体/斜体和撤销重做。
- Markdown/HTML 源码模式带虚拟行号和后台语法着色；工具栏插入本地图片，相对路径按文档目录解析。
- 表格、代码、公式、化学方程式和分子数据以整个语义块切换源码。
- 公式按 DPI 超采样；分子显示球棍和推断键，拖动旋转、双击编辑源码。
- HTML 示例或 `.html` 文件走系统 WebView：原生 DOM/CSS 排版与富文本编辑，源码和阅读模式可切换。
- Markdown 中行内 HTML 与相邻 Markdown 一起交给 CommonMark HTML writer，再以脚本禁用的 DOM 片段排版；列表、表格保持整个编辑块，片段高度自动测量。混排数学公式输出 SVG。
- PDB 蛋白质使用 Cα 样条丝带，识别 HELIX/SHEET，按链、TER 和空间断裂分段；普通 XYZ/小分子保留球棍。
- 自动保存默认关闭，可在工具栏切换；配置文件与环境变量见 IDE README。保存使用去抖、原子写入和外部修改检查；未命名文件不会偷偷选择保存位置。

HTML 内置的格式按钮作用于浏览器选区；Markdown 显示对应的原生格式按钮。HTML 内也可使用浏览器的键盘快捷键。启动时可传入文档路径：`cargo run -p jadix_md -- path/to/document.md`。
导入脚本不会执行。HTML 编辑导出需要 Windows WebView2 或 macOS/Linux-X11 Wry 消息桥。

本地 HTML 图片仅加载文档目录内的文件，经过 canonicalize 检查，单张上限 16 MiB、合计 32 MiB；HTTPS 网络图片交给浏览器。浏览器 HTML 保留源图片路径，导出不会改写成庞大的 data URL。

这不是完整 Typora 复刻：没有跨块预览拖选、关闭窗口的未保存提示或拼写服务集成。HTML/CSS 的安全策略阻止脚本、表单和导航；多个片段不是一个共享 CSS 级联的浏览器文档。原生 Markdown 与浏览器 HTML 的长文档性能契约不同，详见 `crates/jadix/DOCUMENT_EDITING.md`。
