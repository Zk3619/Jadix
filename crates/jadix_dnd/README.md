# jadix_dnd

独立产品 crate，默认不启用 `macros`。普通函数、Builder、Props 共用运行时实现；RSX 显式启用 `macros` 并使用本包的 `ui!`。

组件使用 `jadix_components::prelude::*`；DnD 使用 `jadix_dnd::utils::drag`。基础指针、路由和宿主协议仍在 `jadix::utils::drag`。

源码分发须复制 `src`、LICENSE 及 manifest 的依赖和 feature 到本地模块包/path 包；不复制 target 或根 workspace 配置。安装 crate 为推荐方式，源码定制者负责同步升级。


## 验证归属

原产品内部测试包含框架私有绘制/布局夹具，仍由 `cargo test -p jadix --features product-tests --lib`
在宿主测试桥中编译同一份产品源码，不为迁包公开 renderer 私有字段。
独立包的 `tests` 验证公开消费者契约；基础路由测试仍在 jadix。
`dock` 是 `jadix_components/dnd` 下的高层工作区配方，组合本包与组件库自己的 resizable；DnD 产品不反向依赖组件库。
验证 crate 与源码双分发、无宏与可选宏：`scripts/check-product-distributions.ps1 -Offline`。
