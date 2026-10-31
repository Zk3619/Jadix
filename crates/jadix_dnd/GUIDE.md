# jadix_dnd

独立产品 crate，默认不启用 `macros`。普通函数、Builder、Props 共用运行时实现；RSX 显式启用 `macros` 并使用本包的 `ui!`。

组件使用 `jadix_components::prelude::*`；DnD 使用 `jadix_dnd::utils::drag`。基础指针、路由和宿主协议仍在 `jadix::utils::drag`。

源码分发须复制 `src`、LICENSE 及 manifest 的依赖和 feature 到本地模块包/path 包；不复制 target 或根 workspace 配置。安装 crate 为推荐方式，源码定制者负责同步升级。


