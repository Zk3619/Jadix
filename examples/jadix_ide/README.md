# Jadix IDE

运行：`cargo run -p jadix_ide`。独立代码编辑与差分应用。

- 多文件标签、原生 UTF-8 编辑、IME、选择、无软换行与撤销/重做。
- 默认行号、可见行号节点复用，后台语法高亮（Rust/JavaScript/Python/HTML/Markdown 等；TypeScript 使用 JavaScript 词法回退）。
- 打开、新建、保存和另存为；保存保留对照基线并拒绝覆盖已在外部变化的文件。
- 并排/统一 Diff；载入任意对照文件；示例内置一对 Rust 版本。
- Diff 使用共用虚拟列表保持两侧对齐；编辑与比较模式分离，点击 Diff 时计算快照，避免每次按键计算整份差分。

代码编辑复用框架的 `CodeEditor` 和文档注册表，差分复用 `DiffView`。

## 配置

将 `examples/jadix_ide/jadix-editor.example.json` 复制为启动目录的 `jadix-editor.json`，或设置 `JADIX_EDITOR_CONFIG` 指向自己的 JSON 文件。两个应用共用自动保存配置：

```json
{
  "autosave": { "enabled": true, "delay_ms": 1500 },
  "language_servers": { "rust": { "command": "rust-analyzer", "args": [] } }
}
```

`JADIX_AUTOSAVE_MS=0` 覆盖为关闭，正数开启并设置延迟（250–3600000 ms）。工具栏开关仅作用于本次会话，不覆盖用户配置文件。未命名文件不自动保存；外部修改不会被自动覆盖。

## LSP

语言服务器由用户自行安装，仅启动受信任的用户配置中指定的程序，不自动下载。启动目录中的 `jadix-editor.json` 也能配置可执行程序，打开不受信任的项目时应使用 `JADIX_EDITOR_CONFIG` 指向自己的可信配置。打开文件后通过 stdio Content-Length JSON-RPC 执行 initialize/initialized、didOpen/didChange/didSave；关闭/改路径时退出或关闭文档。支持 full/incremental 服务端同步和 UTF-16 坐标。

工具栏提供补全、悬浮信息、定义位置和格式化；诊断显示在底部面板。补全候选点击应用，格式化/补全作为可撤销修改，过期响应不覆盖新内容。未实现 snippets、completionItem/resolve、语义 token、重命名、跨文件 workspace edit 或服务器重连；LSP 不是本应用已实现的完整 IDE 协议覆盖承诺。

当前没有折叠、断点、终端、Git 操作、多方合并或关闭窗口的未保存提示。着色任务合并只保留最新待办；单文件超过 2 MiB 或单行超过 64 KiB 时使用纯文本，保持原生虚拟滚动，不让正则阻塞输入。Markdown 预览代码块着色缓存支持跨行词法状态，超过 256 KiB 的代码块降级纯文本。
