# jadix

Rust 原生 UI 框架，提供 Element 树、响应式状态、布局、绘制、窗口及可选开发工具。
`jadix_components`、`jadix_lucide` 和 `jadix_dnd` 分别提供产品控件、图标和高层拖拽，按需单独依赖。
框架单用不依赖这三个产品库；Builder 模式不要求过程宏。

本包属于 0.1.0 发布候选闭包；版本示例不代表目标 registry 已发布或平台矩阵已经验收。
维护叉必须与本包所声明的版本一起可解析，消费者无需复制仓库的 patch 或 path 依赖。

## Windows 最小窗口

```toml
[dependencies]
jadix = { version = "0.1.0", default-features = false, features = ["backend-dx12"] }
```

```rust,no_run
use jadix::prelude::*;

fn main() {
    jadix::run(|| Element::new().child(text("Hello, Jadix")));
}
```

其他平台应选择对应后端及宿主 SDK；存在平台源码不代表该平台已经完成原生验收。
默认 feature 包含多个图形后端、主题、基础动画、剪贴板、图像和持久化支持。
需要裁剪时关闭默认 feature，再显式启用当前应用实际使用的能力。

## 可选作者入口和开发工具

| Feature | 用途 |
| --- | --- |
| `macros` | `ui!` 和 `component` 的静态作者入口；不改变 Release 的 Rust 执行模型 |
| `devtools` | Debug Inspector：未使用宏、未登记来源的普通 Element 也可检查；缺失来源明确显示 |
| `hot-reload` | Debug 热更接收端；另启动 `jadix_hot_reload` 文件监听器 |
| `svg` / `animation-base` / `animation-path` | 按需启用 SVG、基础动画或路径动画及其声明依赖 |
| `a11y` | 可访问性桥接；平台行为按实际设备验证 |

热更只应用可证明安全的静态声明或显式登记能力。未登记 Builder 只承诺运行时检查；
普通 Rust 业务逻辑、闭包、捕获、新表达式与未编译工厂仍需重新构建。
Sources 的文件保存、静态声明补丁、Styles 会话覆盖具有不同的持久化和撤销边界。
标准 Release 即使传入开发 feature，也不启动热更监听或包含 Debug 描述与诊断；
这项构建隔离不等于已测得所有场景零性能开销。

## 从产品伞形入口迁移

基础节点和运行时使用 `jadix::prelude::*`；按钮、输入等产品控件从
`jadix_components::prelude::*` 或对应模块导入。产品的宏入口需启用自身 `macros`。
图标使用 `jadix_lucide`；高层拖拽使用 `jadix_dnd`，避免把产品能力反向加入框架依赖。
使用别名依赖时，将宏的 `crate_path` 设置为应用实际使用的别名。

本归档附带 [动画说明](MOTION.md)、[缩放预览说明](SCALE_PREVIEW.md)、[主题](THEMES.md)、
[文档编辑](DOCUMENT_EDITING.md)、[文本视图](TEXT_DOCUMENT_VIEWS.md)、LICENSE 和 NOTICE。

