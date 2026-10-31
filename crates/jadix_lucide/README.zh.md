# jadix_lucide

[English](README.md) | [简体中文](README.zh.md)

Lucide 静态图标与可选动画配方、路径运算，静态使用不要求完整动画栈。

本包属于 [Jadix](https://github.com/Zk3619/Jadix)。**Alpha；版本 `0.1.0`。**

## 安装

在应用中添加依赖：

```toml
[dependencies]
jadix_lucide = "=0.1.0"
```

平台专用功能需要对应平台的 SDK。

## Feature 与边界

默认开关：`ui`, `paths`。完整 feature 与依赖关系以 [Cargo.toml](Cargo.toml) 为准；存在开关不代表平台行为已完成验证。普通应用通过门面选择内部 GPU／窗口后端，不直接依赖平台内部实现。

## 详细用法

原有[包使用指南](GUIDE.md)保留 API、平台及来源细节，可能使用其他语言。

## 三种使用方式

1. 静态 UI 图标：创建窗口前调用 `install()`，再使用 `icon_named("check")` 或 `<Icon name="check" />`。
2. 动态图标：按需开启 `animated`；`svg` 用于 SVG 帧绘制，路径插值另需 `animation-path`。
3. 独立 SVG 源文件：直接复制 [`assets/svg`](assets/svg) 中的单个文件；也可用 `default-features = false, features = ["paths"]` 调用 `svg_document("check")`，纯数据方式不依赖 Jadix/GPU。1,703 个标准 SVG 来自本包固定的路径目录；旧字体别名不代表新增 SVG 几何。查询 API 将 `unlock` 映射到 `lock-open`。

SVG 默认 24×24、`fill="none"`、`stroke="currentColor"`。对于合适的闭合轮廓，可以修改 SVG 的 `fill` 与 `stroke`。Lucide 按描边图标设计，开放路径、孔洞和交叠线条不会自动变成正确的实心图标。字体字形改色只改变已有描边，不能填满内部；需要可靠的实心轮廓时，应提供专门绘制的 SVG。复制时保留 [NOTICE](NOTICE)。

## 文档

[项目文档](https://github.com/Zk3619/Jadix/tree/main/docs)介绍应用配置、平台接入、功能选择与 API 用法。

## 许可证

Apache-2.0。保留 [LICENSE](LICENSE), [NOTICE](NOTICE) 及依赖的来源声明。
