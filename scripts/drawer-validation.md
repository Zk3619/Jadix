# Drawer 本机回归验证

日期：2026-10-17。Windows 11；未使用 Computer Use。树莓派离线，尚未测量 Ubuntu 24.04 上的实际帧率。

## 修复内容

- 逐档吸附使用相邻吸附点的中点决定切换，并限制每次至多跨一档；低于 28% 的合法吸附点可以正常停靠。
- 上下抽屉松手后从当前高度平滑归位；拖动关闭保留当前高度进入退场，避免突然恢复全高。
- 拖动轴采用实际配置的面板尺寸：上下尊重自定义高度，左右尊重宽度及 `24rem` 上限。
- 面板不抢占嵌套输入、按钮和正文滚动区域的指针操作；右键不启动拖动；静止指针不反复触发布局。
- 独立 `DrawerSwipeHandle` 继承 Content 的方向、吸附点及逐档设置，并使用拖动光标。
- 遮罩颜色强度按开度变化，拖动、吸附过程及静止使用同一规则；背景模糊独立于颜色强度，避免拖动时模糊减弱。
- 吸附改变颜色时刷新整个遮罩；模态模糊存在时先合成缓存背景再绘制浮层，避免局部重绘把新露出的条带画成清晰背景。关闭完成后清除拖动状态，重新打开恢复完整尺寸。
- Tooltip / HoverCard 单独计算 Portal 布局时，显式使用视口尺寸，修复容器坍缩为 0×0、性能按钮及 HUD 被移至负坐标的问题。Gallery 性能显示使用独立的响应式宿主。

## 自动验证

上一轮结果：24 项 Drawer 测试、27 项指针事件回归、36 项模态动画回归均通过；启动到内容绘制为 387 / 417 / 417 ms（中位数 417 ms），最大化保存及恢复通过。本轮进一步增加了每个吸附帧与强制重新捕获背景的像素比较、颜色强度关系，以及 Tooltip / HoverCard 打开后的 HUD 实际坐标检查；本轮 release 身份及复验结果见 JSON。

本轮结果：175 项相关组件 / 布局回归通过，四向 Drawer 与模态背景缓存的原生 GPU 回归分别在 GL、DX12 上通过。实际 release Gallery 的 Tooltip、HoverCard 和 Builder HoverCard 打开时 HUD 均保留；修复前按钮位于 `[-100, -130, 88, 32]`，修复后回到视口内。最终启动及最大化恢复复验通过。

本轮可执行文件为 `target/release/gallery-fixed.exe`。原有 `gallery.exe` 被资源管理器启动的旧进程占用，Cargo 完成链接后无法替换它，因此将已链接产物另存于同一 release 目录；旧文件保持不变。新文件 SHA-256：`4cfbf7e8aebfdadafbfda3c891dae7eef6f003bdc427f3910a4e403af33e6c2b`。

运行组件和事件回归：

```powershell
cargo test -p jadix --lib --features svg,bundled-lucide --config 'profile.test.package.jadix.opt-level=0' --config 'profile.test.package.jadix.debug=false' components::drawer::tests -- --test-threads=1
```

原生窗口/GPU 回归使用同一测试二进制，分别设置 `JADIX_GPU_BACKEND=gl` 和 `dx12`：

```powershell
$env:JADIX_GPU_BACKEND='gl' # 再以 dx12 重复
cargo test -p jadix --lib --features svg,bundled-lucide --config 'profile.test.package.jadix.opt-level=0' --config 'profile.test.package.jadix.debug=false' drawer_native_drag_resize_scroll_and_style -- --ignored --nocapture --test-threads=1
```

该测试创建隐藏的原生 Windows 窗口，经过真实命中测试、指针分发、布局和 GPU 渲染。四个方向均检查拖动距离、半开停靠、过渡中间高度、Footer 可见、Body 实际滚动、程序设定吸附点、拖动关闭和重新打开。拖动、每个吸附帧及滚动后的保留帧，均与同一动画姿态下强制重新捕获背景及完整重绘逐像素比较，通道容差为 3；要求超出容差的像素数为 0。CSS 使用虚拟时钟固定姿态，250 ms 高度吸附使用真实帧间隔推进。HUD 同时使用真实 Button / Show / Label，并检查 Tooltip / HoverCard 打开及数据刷新后仍在视口内。

960×640 逻辑视口的预期几何：

| 方向 | 完全打开 | 拖动关闭 30% | 半开吸附 |
| --- | --- | --- | --- |
| Down | 高 544，顶 96 | 高 380.8 | 高 272，顶 368 |
| Up（自定义 400px） | 高 400，顶 0 | 高 280 | 高 200，顶 0 |
| Left | 宽 384，左 0 | 左 -115.2 | 左 -192 |
| Right（自定义 280px） | 宽 280，左 680 | 左 764 | 左 820 |

与本次问题对应的失败记录位于 `.tmp/drawer-red.log`（4 项原始回归失败）、`.tmp/drawer-settle-red.log`（释放时高度跳变）。最终结果及 release 身份记录在 `scripts/drawer-validation.json`。


## 最大化后的 HUD 回归（2026-10-17）

浮层独立布局曾把 Portal 根节点写成当前视口的固定像素尺寸。随后还原或最大化，应用根虽然重新布局，Portal 仍使用旧尺寸，HUD 因而漂移或落在可见区域外。改为百分比尺寸，同时保留独立计算时的明确可用空间。

新增 `persistent_hud_keeps_viewport_edges_after_floating_follow_and_maximize`，旧实现确定失败：1706.67×978.67 视口内 HUD 仍在 `[668,510,280,118]`。修复后右、下边距均保持 12px；同时加强原有响应式刷新回归，检查真实坐标。Portal、Popup、Floating Motion 共 119 项通过。

原生 GPU 回归新增实际窗口尺寸变化（960×640 ↔ 约 1707×979），检查 HUD 固定边距。release Gallery 通过 Win32 事件回放验证最大化下 Tooltip、HoverCard、Builder HoverCard 打开，以及后续还原、重新最大化。未使用 Computer Use。

本轮生成标准路径 `target/release/gallery.exe`；可执行文件身份与原始结果见 JSON 的 `release` 和 `hud_maximize`。重启最大化恢复检查三次通过；三次本地启动到内容绘制中位数为 419ms，显示后的空白约 34–37ms。
