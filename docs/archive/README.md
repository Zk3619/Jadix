# 历史文档归档（已冻结）

> **地位**：选型过程、早期随笔与误名文档。**不是**权威规范或巩固期执行源。  
> **权威四件套**：[`../ENGINEERING_GRADE_FRAMEWORK.md`](../ENGINEERING_GRADE_FRAMEWORK.md) · [`../ENGINEERING_IMPLEMENTATION_PLAN.md`](../ENGINEERING_IMPLEMENTATION_PLAN.md) · [`../COMPONENT_LIBRARY.md`](../COMPONENT_LIBRARY.md) · [`../COMPONENT_LIBRARY_PLAN.md`](../COMPONENT_LIBRARY_PLAN.md)  
> **归档日期**：2026-08-09  

| 文件 | 原用途 | 为何归档 |
|------|--------|----------|
| [Tech-Select.md](Tech-Select.md) | 底层 crate 选型对比（winit/wgpu/taffy/glyphon…） | 决策已写入纲领 §2.2「锁定栈」 |
| [Design.md](Design.md) | 早期「自研 GUI 栈」蓝图聊天稿 | 实现前叙事；被纲领 / PROJECT 取代 |
| [framework.md](framework.md) | 早期 retained-mode / 混合架构随笔 | 原文 UTF-8 损坏，仅留 stub；结论已在 gap §1.1 / 纲领 |
| [rust-ui.md](rust-ui.md) | 早期借用 / IME / taffy 建议 | 启动期建议，非现行契约 |
| [test.md](test.md) | glyphon vs parley/vello 文本栈建议（**非测试计划**） | 中期结论见 [`../glyphon_audit.md`](../glyphon_audit.md)；勿当测试规范 |
| [implementation_plan.md](implementation_plan.md) | Dubai→Inter 字体统一提案 | **勿与** `ENGINEERING_IMPLEMENTATION_PLAN.md` 混淆；字体策略见 [`../font_strategy.md`](../font_strategy.md) |

修改本目录文件不会改变项目方向；若结论需回灌，应开 ADR 并改权威文档。
