# jadix_lucide

统一的 Lucide 静态目录、字体、SVG、lucide-animated 播放与动画配方；不再分发 data 包。

```toml
jadix_lucide = "0.1"
```

默认启用 `ui,paths`，在 UI 线程、创建窗口前调用 `jadix_lucide::install()?` 安装目录与字体。
静态图标使用框架的 `icon_named("check")` / `<Icon name="check" />`。
`animated` 增加动画配方与播放（启用框架 animation-base）；`svg` 使用真实 SVG 帧绘制。
`animation-path` 显式启用路径插值与 MorphIcon 协作（包含 SVG 和框架增强动画）；仅启用 `svg` 不会隐式启用路径动画。
`macros` 可选，不强制依赖 UI 解析包；动画 RSX 使用 `jadix_lucide::ui!`。
纯数据工具可关闭默认 feature，按需开启 `paths,font`，不会依赖 Jadix/GPU。

推荐安装 crate。源码定制复制本包 src/assets/LICENSE/NOTICE 并保留实际 Cargo 依赖与 feature。
框架仅通过通用 IconProvider/字体/播放契约使用目录，不反向依赖本包。
验证入口：`scripts/checks/check-product-distributions.ps1 -Offline`。
