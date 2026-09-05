# Remote asset（`remote-asset`）

对照 Freya `remote-asset`：让 `<Image src>` / `<AvatarImage src>` / `<Icon src>` **能接受** `http://` 与 `https://`。

框架 **不内置 HTTP 客户端**（无 `ureq` / `reqwest` / TLS）。网络栈、超时、代理、鉴权、缓存全部由应用安装 [`RemoteAssetFetcher`](../../crates/rui/src/media/remote.rs)（与 [`set_icon_provider`](../../crates/rui/src/utils/icon_provider.rs) 同一形态）。

同步 [`load_src`](../../crates/rui/src/media/decode.rs) / CSS `cursor: url()` / `decode_image_src` **仍然拒绝** 远程 URL。

## 两层

| 层 | 谁提供 | 职责 |
|----|--------|------|
| 入口 | 框架（本 feature） | 识别 `http(s)`；`spawn_blocking` → 解码 → GPU；[`SrcLoadState`](../../crates/rui/src/app/pending_images.rs) |
| 传输 | 应用 | `GET` / 缓存 / CDN / 企业代理……任意实现 [`RemoteAssetFetcher::fetch`](../../crates/rui/src/media/remote.rs) |

## Feature

```toml
rui = { version = "0.1", features = ["remote-asset"] }
```

空 feature：不拉依赖。未开启时远程 `src` 立即失败。开启后若未 `set_remote_asset_fetcher`，后台拉取会失败（空白 / Avatar fallback）。

```text
cargo run -p gallery --features remote-asset
```

Gallery 演示用 **进程内闭包** 把固定 URL 映射成 PNG，不访问外网。

## 安装 fetcher

闭包（最常见）：

```rust,ignore
rui::set_remote_asset_fetcher(|url: &str| {
    let bytes = reqwest::blocking::get(url)
        .and_then(|r| r.error_for_status())?
        .bytes()?;
    Ok(bytes.to_vec())
});
```

或自有类型：

```rust,ignore
struct CachedCdn { /* client + disk cache */ }
impl rui::RemoteAssetFetcher for CachedCdn {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, String> { /* … */ }
}
rui::set_remote_asset_fetcher(CachedCdn { /* … */ });
```

`fetch` 在 **worker 线程** 上调用，可以阻塞。不要在里面碰 Signal / Element。

自定义管线（SVG 外链、自绘贴图）走同一入口，不必经过 `<Image>`：

```rust,ignore
let bytes = rui::fetch_remote_bytes(url)?;
rui::queue_image_from_bytes("tex", &bytes);
```

## 行为

| `src` | 默认 | `remote-asset` |
|-------|------|----------------|
| 本地 / `file:` / `assets/` | 同步读盘 | 同左 |
| `data:` | 同步解码 | 同左 |
| `http(s)` | 立即失败 | Pending → 调已安装 fetcher → 解码上 GPU |

## 不做

- 内置 `fetch` / TLS / Cookie
- CSS `cursor: url(https://…)`
- SVG 场景图自动跟 `href="https://…"`（[`drawing.md`](../drawing.md)）；需要时应用先 `fetch_remote_bytes` 再当本地/贴图键用
