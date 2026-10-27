# jadix_components

原生控件库。默认无过程宏依赖；普通函数、Builder、Props 和 RSX 调用同一实现。
需要 RSX 时启用 `macros`，使用 `jadix_components::prelude::*` / `jadix_components::ui!`。
Props 声明由无依赖声明式契约维护，避免有宏/无宏各复制一份组件逻辑。

推荐 crate 安装；源码定制需复制 src、LICENSE 和实际使用的 Cargo 依赖与 feature。
私有绘制/布局夹具回归仍在 jadix 的宿主测试桥中编译同一产品源码；独立产品 tests
验证公开 API 消费。不会为迁包公开 renderer 私有字段。
