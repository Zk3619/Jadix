# Jadix 微信小程序

这个示例在微信逻辑层的 WXWebAssembly 中运行 Rust，复用 jadix_core 的节点、受控事件与环境契约，以及 jadix_layout 的 Taffy 布局。MiniProgramHost 通过 protocol 2 发布到递归 WXML 原生组件，Canvas measureText 负责宿主文字测量。无需 GPU 或 QuickJS。

## 构建和运行

在仓库根目录执行，需要 Rust、Node.js 22+ 与微信开发者工具：

```powershell
rustup target add wasm32-unknown-unknown
npm ci --prefix examples/miniprogram
node scripts/build-miniprogram.js target/miniprogram-project
```

构建目录必须尚不存在，避免覆盖上次证据。构建脚本采用固定 Binaryen 132.0.0 降低指令集并验证 MVP，然后生成 app.wasm.br、应用文件和含 SHA-256／包大小／imports 的 build-receipt.json。只设置 Rust target-feature 不足以消除预编译标准库中的新指令：旧微信引擎实际拒绝过这种产物。

将生成目录导入微信开发者工具，项目配置默认 touristappid，适合本地模拟器。发布和真机权限需要自己的小程序应用配置。CLI 路径按本机安装位置调整：

```powershell
& 'D:/微信开发者工具/cli.bat' auto --project 'D:/Projects/Rust/rust-native-ui/target/miniprogram-project' --auto-port 9422 --trust-project
```

默认保留平台原生外观。需要共享主题、字号和控件尺寸时，可启用可选的一致性特性：

```powershell
node scripts/build-miniprogram.js target/miniprogram-consistent --consistent-ui
```

这会启用 `jadix_native/consistent-ui`，由共享主题在布局前生成样式。系统字体、键盘、选择菜单和控件内部绘制仍由微信及操作系统提供；该特性不保证逐像素一致。

## 当前实现

- 十二种共享节点：Box、Text／inline spans、Button、Input、Textarea、Checkbox、Switch、Select、Scroll、Image、Dialog、Popover；示例挂载 68 个节点。
- frame 来自同一个共享布局结果，WXML 使用父节点相对 px；不引入一套独立 CSS 布局算法。原生控件字体、内边距和辅助功能仍受微信与系统影响。
- protocol 2 原子校验 root／revision／树所有权，受控 edit／toggle／select／dismiss 版本使用十进制 u64 字符串；拒绝过期节点、错误根、分裂 UTF-16 代理对和失效浮层事件。
- 稳定叶更新缩小 setData path；共享树复制、完整投影、几何变化仍有 O(N) 成本。
- 平台名为 miniprogram，物理 os 单独报告；未知开发者工具 OS 为 null。主题、字体缩放、resize、前后台和卸载进入共享环境／生命周期，焦点和滚动位置保留；延迟到达的旧控件 blur 不会清除新焦点。
- 支持已接受根／修订绑定的程序滚动。卸载释放 Rust 根和 JS 树，丢弃晚到事件。

实际 WXWebAssembly 已验证最终压缩模块、Unicode 编辑、选择／切换、焦点、滚动、主题／字号、resize、Dialog／Popover 和卸载；受控 value 回显保留 Unicode cursor，并拒绝分裂 UTF-16 代理对的位置。Node 测试使用真实 Rust WASM，但 Canvas 测量为确定性 fixture，不能代替字体与视觉一致性验证。

目前没有小程序 TSX／QuickJS、页面栈、动画、长按／拖拽／缩放、刷新／sticky、复杂阴影／渐变／backdrop、媒体／WebView、自定义字体文件适配。真实 IME preedit、系统控件像素、读屏器和实体 iOS／Android／HarmonyOS 微信按设备门另验；当前 input 仅提交 value／cursor，composing 为 null。共享 jadix GPU 已有的高级样式不能直接记到这个 WXML 后端。
