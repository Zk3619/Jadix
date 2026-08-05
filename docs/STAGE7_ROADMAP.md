# Stage 7+ 长期项排期（对照 Docs/PROJECT.md）

以下与 [PROJECT.md](PROJECT.md) 第七节对齐，作为里程碑占位，便于 issue/看板拆解：

1. **Headless 测试环境**：无窗口或离屏 surface 下跑布局 + 可选像素快照/ golden，服务 CI。
2. **桌面 Router**：路由状态机 + 历史栈（后退/前进），与 `Signal` 与当前 `template_id` 策略兼容。
3. **文档站**：中英双语文档站点 + 可运行示例索引（扩展现有 `examples/`）。
4. **跨平台**：在 Windows / macOS / Linux 三端固定示例冒烟；移动端与鸿蒙保留 `PlatformWindow` 式后端抽象，不阻塞桌面主线。

当前已并入主线的相关能力：`accesskit_winit` 运行时桥接、细粒度 `reactive!` / `create_memo`、热重载模板契约文档（见 [reactivity_and_hot_reload.md](reactivity_and_hot_reload.md)）。
