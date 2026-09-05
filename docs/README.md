# RUI 文档

**从这里开始**：[AUTHORITY.md](AUTHORITY.md) — 巩固期唯一入口与执行计划。

根目录只放**权威规范**与 **changelog**。专题手册、对照审计、冻结计划已按域收进子目录。

## 根目录（权威）

| 角色 | 文档 |
|------|------|
| 执行源 | [AUTHORITY.md](AUTHORITY.md) |
| 变更记录 | [CHANGELOG.md](CHANGELOG.md) |
| 工程门槛 / ADR | [ENGINEERING_GRADE_FRAMEWORK.md](ENGINEERING_GRADE_FRAMEWORK.md) |
| 组件规范 | [COMPONENT_LIBRARY.md](COMPONENT_LIBRARY.md) |
| 响应式规范 | [REACTIVITY.md](REACTIVITY.md) |
| 身份契约 | [element_identity.md](element_identity.md) |
| 公开自绘 | [drawing.md](drawing.md)（Svg / Canvas / 2D·3D；WP-3.11–3.13） |

## 子目录

| 目录 | 内容 |
|------|------|
| [components/](components/component_inventory.md) | 分桶表、variant 词汇、分层对照、自定义组件 / headless / 浮层壳 |
| [engine/](engine/box_model.md) | 盒模型、布局、字体、颜色、动效、damage、资源 |
| [ops/](ops/desktop_smoke.md) | 冒烟、IME、键盘、发布、托盘、多窗口、DevTools、热重载 |
| [comparison/](comparison/gap_analysis.md) | 对照审计（非执行源） |
| [archive/](archive/README.md) | 冻结计划、愿景稿、早期随笔 |
| [plans/](plans/global-stylesheet-loader.md) | 进行中的专题计划 |
| [site/](site/README.md) | 文档站 stub（WP-4.4） |

### 组件

- [component_inventory.md](components/component_inventory.md) — 分桶 + id 策略（**Gallery 验证 2026-09-05**）
- [component_variants.md](components/component_variants.md) — variant / size / 受控命名
- [COMPONENT_LAYERING.md](components/COMPONENT_LAYERING.md) — 体量 × gpui-component（非执行源）
- [custom_components.md](components/custom_components.md) · [custom_components_suggestions.md](components/custom_components_suggestions.md)
- [headless.md](components/headless.md) · [overlay_shells.md](components/overlay_shells.md)

### 引擎

- [box_model.md](engine/box_model.md)（WP-2.9 ☑ **已冻结**）· [layout_subtree.md](engine/layout_subtree.md) · [stack_and_element_model.md](engine/stack_and_element_model.md)（ADR-10 ☑ 冻结）· [platform.md](engine/platform.md)
- [color.md](engine/color.md) · [font.md](engine/font.md) · [font_strategy.md](engine/font_strategy.md) · [text_align.md](engine/text_align.md)
- [animation_and_motion.md](engine/animation_and_motion.md) · [css_animation_max_alignment.md](engine/css_animation_max_alignment.md)（**已冻结**）
- [damage.md](engine/damage.md) · [glyphon_audit.md](engine/glyphon_audit.md) · [assets_and_icons_architecture.md](engine/assets_and_icons_architecture.md)

### 运维 / 验收

- [desktop_smoke.md](ops/desktop_smoke.md) · [a11y_smoke.md](ops/a11y_smoke.md) · [ime.md](ops/ime.md) · [keyboard_nav.md](ops/keyboard_nav.md)
- [PUBLISH.md](ops/PUBLISH.md) · [W4_PRELAUNCH_CHECKLIST.md](ops/W4_PRELAUNCH_CHECKLIST.md)
- [multi_window.md](ops/multi_window.md) · [tray.md](ops/tray.md) · [remote_asset.md](ops/remote_asset.md) · [devtools.md](ops/devtools.md)
- [reactivity_and_hot_reload.md](ops/reactivity_and_hot_reload.md) · [size_and_memory.md](ops/size_and_memory.md) · [crate_deps.md](ops/crate_deps.md)

### 非执行源

- 对照：[REFERENCE_COMPARISON.md](comparison/REFERENCE_COMPARISON.md) · [GPUI_ECOSYSTEM_COMPARISON.md](comparison/GPUI_ECOSYSTEM_COMPARISON.md) · [FREYA_COMPARISON.md](comparison/FREYA_COMPARISON.md) · [gap_analysis.md](comparison/gap_analysis.md)
- 冻结计划：[ENGINEERING_IMPLEMENTATION_PLAN.md](archive/ENGINEERING_IMPLEMENTATION_PLAN.md) · [COMPONENT_LIBRARY_PLAN.md](archive/COMPONENT_LIBRARY_PLAN.md) · [REACTIVITY_PLAN.md](archive/REACTIVITY_PLAN.md)
- 历史：[PROJECT.md](archive/PROJECT.md) · [STAGE7_ROADMAP.md](archive/STAGE7_ROADMAP.md) · [NAMING_DECISION.md](archive/NAMING_DECISION.md)（已锁定 `jadix` / 琢玉）
