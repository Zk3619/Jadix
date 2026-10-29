# 迁移与版本说明

当前清单是首个基线的待发布版本，项目保持 Alpha。版本号、归档和 dry-run 都不能代替真实发布记录。

## 框架与产品

- 基础节点、运行时、状态和窗口使用 `jadix::prelude::*`。
- 产品控件使用 `jadix_components::prelude::*`。
- 图标使用 `jadix_lucide`；高层拖拽使用 `jadix_dnd`。
- 不再依靠框架的 `components` 或 `dnd` 开关；`drag` 表示基础拖动能力。
- 仅用框架的应用不应因为 prelude 而反向安装完整产品库。

## 作者入口与 feature

Builder 是普通 Rust 接口。使用 `ui!` / `#[component]` 时启用对应包的 `macros`；产品默认不要求 UI 宏。
依赖重命名后，将宏的 `crate_path` 指向应用实际使用的别名。
热更描述符格式改变时需实际重编译一次，源码更新不会自动改变旧二进制的描述符。

裁剪依赖时使用 `default-features = false`，显式选择平台支持的 `backend-*` 和所需能力。
代码编辑、语法高亮、LSP、Markdown、SVG、动画和无障碍分别启用；默认值和关闭行为见 [Feature 清单](FEATURES.md)。

## 维护叉与升级

窗口、GPU、布局和文字栈使用明确的 `jadix_*` 包名及锁定版本。消费者从同一 registry 解析闭包，不复制本仓 path／git／patch。
维护叉保留 README、GUIDE、许可证及来源说明；Jadix 的发布不表示上游发布或背书。

升级时核对名称、版本、checksum、feature 和锁文件。分别构建框架单用与框架加产品的原生应用，检查启动／退出、控件、输入和实际后端。
本次测试目录迁移保留 Rust 测试模块身份及生产 API，不要求应用修改 import。

## 字体与 SVG 资源

大型字体不进入 registry 包；默认字体发现与 `JADIX_FONT_DIR` 运行时入口保持可用。启用 `bundled-*` 前，按 [Jadix 包说明](../crates/jadix/README.zh.md) 准备字体并设置构建期 `JADIX_BUNDLED_FONT_DIR`。源码工作区仍可用已有 assets，registry 消费者应显式准备字体；仅改变默认 feature 不能缩减 Cargo 上传归档。

`jadix_lucide` 增加 [独立 SVG 源文件](../crates/jadix_lucide/assets/svg)，可直接复制，或启用纯数据 `paths` 后调用 `svg_document`。原字体与动画入口保留。SVG 可设置 fill，但描边图标不保证具备正确实心轮廓；字体改色不等于填满图标内部。
