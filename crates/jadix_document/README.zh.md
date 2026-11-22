# jadix_document

[English](README.md) | [简体中文](README.zh.md)

无 GUI 依赖的文档快照、文本数据、高度索引与可选差异算法，不包含应用绘制控件。

本包属于 [Jadix](https://github.com/Zk3619/Jadix)。**Alpha；版本 `0.1.0`。**

## 安装

在应用中添加依赖：

```toml
[dependencies]
jadix_document = "=0.1.0"
```

平台专用功能需要对应平台的 SDK。

## Feature 与边界

默认开关：无。完整 feature 与依赖关系以 [Cargo.toml](Cargo.toml) 为准；存在开关不代表平台行为已完成验证。普通应用通过门面选择内部 GPU／窗口后端，不直接依赖平台内部实现。

## 详细用法

原有[包使用指南](GUIDE.md)保留 API、平台及来源细节，可能使用其他语言。

可选 `checkpoint` feature 提供无 GUI 的 `checkpoint::{Checkpoint, CheckpointStore}`，支持大小限制、校验和、代次冲突检测与同目录替换；`jadix_native` 和 `jadix_tui` 共用同一实现，不引入渲染器或语法引擎。锁与代次冲突显式失败，不承诺断电可靠。

## 文档

`syntax::SyntaxWorker` 在后台线程处理不可变快照，最多保留一个可替换的待处理任务。启用 `syntax` 时，从修改位置之前的行首检查点续算；仅当未变后缀的解析状态和高亮状态均一致时复用后缀。内容完全相同的新快照直接复用颜色，同时发布新修订身份。语言／主题变化从头解析。每个 worker 保留一份高亮快照、间隔至少 4 KiB 的稀疏检查点，以及供流式追加使用的末行检查点；在 2 MiB 文档限制下最多 514 个检查点。单行超过 64 KiB 或文档超过 2 MiB 时返回无颜色文本并清空复用状态。取消会丢弃过期结果，无需等待当前解析结束。源文本比较与颜色注释复制仍有线性成本，解析状态大小受语法嵌套深度影响，不提供解析时间上限。

[项目文档](https://github.com/Zk3619/Jadix/tree/main/docs)介绍应用配置、平台接入、功能选择与 API 用法。

## 许可证

Apache-2.0。保留 [LICENSE](LICENSE) 及依赖的来源声明。
