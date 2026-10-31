# 共享远程资源缓存

`remote-asset-cache` 是默认关闭的 feature，会同时启用 `remote-asset` 与 Moka 的同步缓存。
缓存面向**下载后的编码字节**；Image、Avatar、自定义 SVG/图片管线可以共享同一次下载，
解码和遮罩仍使用各自选项，GPU 纹理池继续按原有帧/设备生命周期管理。
它不依赖 Tokio；实际阻塞读取继续在 worker 上执行。

## 安装已有网络适配器

```rust,ignore
use std::{num::NonZeroU32, time::Duration};
use jadix::{CachedRemoteAssetFetcher, RemoteAssetFetcher, set_remote_asset_fetcher};

fn install(fetcher: impl RemoteAssetFetcher + 'static) -> CachedRemoteAssetFetcher {
    let cached = CachedRemoteAssetFetcher::new(
        fetcher,
        NonZeroU32::new(64 * 1024 * 1024).unwrap(),
        Duration::from_secs(300),
    );
    set_remote_asset_fetcher(cached.clone());
    cached
}
```

将已有 Reqwest/ureq/平台适配器或闭包交给 install。feature 本身不会自动安装缓存、
HTTP 客户端或 runtime。容量是缓存保留的 payload 字节权重目标，空值计为 1；
单个响应超过配置容量会返回错误。Moka 采用尽力淘汰，不能把它当作整个进程的内存硬上限。
网络后端仍须在传输/解压期间落实响应体限制、超时、权限及 URL 策略。

相同 URL 的并发 miss 使用 Moka 的 `try_get_with_by_ref` 合并，成功返回同一个 `Arc<[u8]>`。
命中时直接借用 URL 查询，不再为每次读取分配 String 缓存键。
失败不会存入缓存。原有 `RemoteAssetFetcher::fetch` 及 `fetch_remote_bytes` 保持 Vec 返回类型；
新增 fetch_shared 的默认实现兼容旧适配器。框架的 Image/Avatar 已改用 fetch_shared，
缓存命中时不复制整份编码字节；老的 Vec 调用方仍会获得独立副本。

## 失效与生命周期

`cached.clear()` 原子替换所有 clone 共用的缓存实例。清空之前开始的读取可以为原调用者完成，
但它的迟到结果不会重新填入新缓存，也不会成为后续 fetch 的命中值。
TTL 控制后续 fetch 的命中资格，**不自动刷新已经加载的纹理或控件失败状态**。

缓存键是去除首尾空白后的完整 URL，不擅自删除 query、签名或大小写信息。
同一 URL 在不同权限下可能返回不同内容，因此每个认证/租户会话应使用独立缓存实例。
切换会话时还必须按应用原有资源策略重建/失效旧控件及已解码纹理；仅 clear 编码字节缓存
不会撤销其他地方持有的 Arc、已排队结果或已显示的图像。

一个控件离开不会取消其他控件仍需要的共享阻塞读取。当前缓存复用已有同步 fetcher 契约，
底层操作的中断由后端负责；它没有实现最后订阅者离开即取消的完整资源 registry。
高并发调用应使用有界后台执行器。既有 Image 便利加载路径仍使用原有 worker 桥接。

## 缓存实现

选择 Moka 复用同键初始化合并、TTL、字节权重与并发淘汰。该 feature 不会把 Tokio 或 Moka 放进默认 Jadix 依赖图。
