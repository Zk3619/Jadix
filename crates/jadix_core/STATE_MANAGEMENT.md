# 状态管理与作用域

`jadix_core` 是独立的响应式与状态内核；直接使用它不需要 GUI、Tokio 或 Moka。
Jadix 的 prelude 导出 `create_store`、`Store`、`ReadStore`、`StoreField`、
`store_field!`、`batch`、Memo 与 Context 的常用入口。

## 选择状态与作用域

| 需求 | 推荐入口 | 生命周期 |
| --- | --- | --- |
| 单个局部值 | `create_signal` / `Signal::new` | 当前 owner |
| 重建时需要按本地名称复用 | `use_scoped_signal` | 当前 owner，名称不与其他 owner 冲突 |
| 结构化状态 | `create_store` + `store_field!` | Store 创建时的 owner |
| 派生值 | `create_memo`，避免复制一份派生状态 | 当前 owner |
| 多个字段一起更新 | `batch` / `Store::update` | 外层批次结束后统一通知 |
| 子树共享 | 稳定父 owner 下创建 Store，再 `provide_context` | 提供者 owner |
| 多窗口独立状态 | 每个窗口根分别创建并提供 | 各窗口 owner |
| 全应用共享状态 | 在明确的长寿命 owner 下创建并显式注入 | 应用自行持有与释放 |

`create_root` 创建**独立根**，即使写在另一个 create_root 中，也不会继承它的 Context。
组件子树使用 `component_root` / `with_child_owner`，会继承外层 Context，子树随父 owner 清理。
持有或复制 Store/ReadStore/字段句柄不会延长 owner 的生命周期。

不要在每次渲染、每次事件或反复运行的 effect 中重新创建应共享的 Store。
不要把 `use_signal` 的进程级命名注册表当作默认业务状态容器。

## 字段读写

```rust
use jadix_core::{create_root, create_store, store_field};

#[derive(Clone, PartialEq)]
struct User { name: String, age: u32 }
#[derive(Clone, PartialEq)]
struct Settings { user: User, dark: bool }

let (_owner, ()) = create_root(|| {
    let settings = create_store(Settings {
        user: User { name: "Ada".into(), age: 25 }, dark: false,
    });
    let name = store_field!(settings, user.name);
    name.set("Grace".into());
    settings.update(|value| value.dark = true);
    assert_eq!(settings.read_only().get().user.name, "Grace");
});
```

宏从同一个 Rust 字段表达式生成路径、getter 和 setter；不存在的字段或不匹配的赋值类型会编译失败，
Store 表达式只求值一次。嵌套命名字段受支持。动态路径仍可用 `Store::field`，
但调用者必须确保同名路径始终对应同一字段、类型及纯 getter。

字段读取只订阅对应路径。字段写入比较自身及重叠的祖先/后代路径；无关兄弟字段不重算。
路径使用有序索引，字段更新直接查找祖先与后代区间，无须遍历全部已注册路径；
路径边界以 `.` 区分，`user.name` 不会误匹配 `user.nameplate`。
整根 `set/update/produce` 会比较已注册字段的投影值，仅通知改变的路径。
大 Store 的高频更新优先使用字段句柄；根读取仍订阅整个 Store。

## 只读共享与写入入口

`Store::read_only()` 返回 `ReadStore<T>`，支持 `get/try_get/with/peek/try_peek/is_alive`，
不提供 setter、不暴露底层可写 Signal。可通过 Context 把只读视图交给展示子树，
写入由父层的事件处理器执行；需要编辑的子树也可显式接收 Store 或 StoreField。

`Store::signal()` 已弃用，返回类型改为 ReadStore。原有 get/peek/with/is_alive 读取可继续使用；
原来通过该 Signal 写入或显式标注为 Signal 的代码需要迁移到 Store/StoreField，
避免跳过字段路径通知。只读句柄在 owner 销毁后可用 try_get/try_peek 检查失效。

响应式存储属于创建它的线程。后台任务返回普通数据，在 TaskScope 的 UI 回调内更新 Store；
无需将整个状态内核改成 Arc/Mutex，也不在 worker 上访问 Signal/Store。

## 列表与异步数据

列表项使用稳定业务 ID 作为 keyed UI 的身份。排序和插入不要让位置索引充当身份。
服务端整批快照可先通过 Store::set/update 应用；本轮不引入通用 reconcile 或另一套 query 状态。
现有 Resource 继续管理 Loading/Ready/Err、请求代次和过期完成屏蔽；缓存保存共享数据值。

验证：`cargo test -p jadix_core`。
