# jadix

[English](README.md) | [简体中文](README.zh.md)

Rust 原生 UI 框架，提供运行时、元素、布局、绘制、窗口与可选开发工具。框架、产品、宏和运行时适配分别依赖。

本包属于 [Jadix](https://github.com/Zk3619/Jadix)。**Alpha；版本 `0.1.0`。** 本地归档和清单不代表 registry 已可安装。

## 安装

使用目标 registry 中可用的对应版本：

```toml
[dependencies]
jadix = { version = "0.1.0", default-features = false, features = ["backend-dx12"] }
```

发布前使用源码工作区。普通应用从同一 registry 解析依赖图，不复制本仓私有 path／git／patch；平台专用包仍需对应 SDK。

## Feature 与边界

默认开关：`macros`, `backend-dx12`, `backend-gles`, `backend-metal`, `window-materials`, `theme-shadcn`, `animation-base`, `native-clipboard`, `images`, `file-persistence`。完整 feature 与依赖关系以 [Cargo.toml](Cargo.toml) 为准；存在开关不代表平台行为已完成验证。普通应用通过门面选择内部 GPU／窗口后端，不直接依赖平台内部实现。

## 可选内嵌字体

registry 归档分发源码与来源声明，不携带大型 Noto 字体集。默认构建继续使用系统／运行时字体发现；运行时字体目录通过 `JADIX_FONT_DIR` 提供。

启用 `bundled-cjk`、`bundled-cjk-serif`、`bundled-emoji` 或 `bundled-symbols` 时，在**构建前**设置 `JADIX_BUNDLED_FONT_DIR` 为已取得许可的字体目录。各开关分别需要：`NotoSansCJK.ttc`；`NotoSerifCJK.ttc`；`NotoColorEmoji_WindowsCompatible.ttf` 与 `NotoColorEmoji.ttf` 两个文件；或 `NotoSansSymbols-wght.ttf`、`NotoSansSymbols2-Regular.ttf`、`NotoSansMath-Regular.ttf`、`NotoMusic-Regular.ttf` 四个文件。`bundled-fonts` 需要全部文件。源码工作区默认使用包内已有的 `assets` 目录。缺失所需文件时构建会给出明确错误，不会在构建期间联网下载。应用分发时保留字体许可证与 [NOTICE](NOTICE)。

## 详细用法

原有[包使用指南](GUIDE.md)保留 API、平台及来源细节，可能使用其他语言。

## 文档与贡献

[项目文档](https://github.com/Zk3619/Jadix/tree/main/docs)说明用法、迁移、测试和发布。保留测试发现入口，私有模块测试与公开契约分层；包配置变化同步两种 README。

## 许可证

Apache-2.0。保留 [LICENSE](LICENSE), [NOTICE](NOTICE) 及依赖的来源声明。
