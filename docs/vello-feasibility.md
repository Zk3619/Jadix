# 使用 Vello 替代 Jadix 2D 的可行性

> 可复用子场景、嵌套实例及组合能力更新见[第十二轮](2d-round12.md)。

> 最新复杂文本排版、彩色字形、字体 atlas 与 source-region 绘图见[第十一轮](2d-round11.md)。公平性能计时的更正仍见[第十轮](2d-round10.md)。以下保留前期记录。


评估日期：2026-11-05。结论：路径/Canvas 后端可替换，当前不能将整个 Jadix
渲染器直接改成一个 Vello 调用。本次完成独立 `jadix_2d` 提取，没有引入 Vello
依赖或宣称 Vello 性能已验证。

后续已参考绘制模型增强现有实现，见 [2D 增强记录](2d-enhancements.md)，包含
路径裁剪、隔离图层、锥形/重复 GPU 渐变与局部性能实测；尚未与 Vello 同机比较。

## 当前边界

`jadix_2d` 拥有路径、渐变、CPU 光栅、Canvas 状态/变换/裁剪、GPU 命令与
stencil-cover + MSAA/SSAA 渲染。Jadix 保留 Element、布局、缓存、GPU 贴图注册、
上传和 readback、渲染图及窗口呈现。文本排版、字形缓存、圆角 UI 实例、
背景模糊、原生材质、3D/WebView 合成也不属于本次移动的路径管线。

## 官方实现的区别

原始 `vello` 需要 compute shaders，可将 Scene 渲染到纹理；路径、图片、渐变和
定位后的字形属于其绘制模型。当前可见发布文档为 0.11.0。
来源：[Vello crate 文档](https://docs.rs/vello/latest/vello/)。

官方 main 已将原始 compute 实现定位为实验路线；`vello_gpu` 采用 CPU 预处理加
GPU 光栅/合成，不要求 compute，并提供 WebGL2 和 wgpu 后端；`vello_cpu` 提供
软件路线。main 与 crates.io 发布状态必须分别核对。
来源：[官方 README](https://github.com/linebender/vello)。

`vello_gpu` 文档目前列出的限制包括 mask layers、复杂 filter graphs、部分
非隔离混合模式会 panic，部分资源和字形缓存 API 仍在演进。不能由“支持渐变/图片”
推导出 CSS 效果或所有合成语义完全等价。
来源：[Vello GPU README](https://raw.githubusercontent.com/linebender/vello/main/vello_gpu/README.md)。

## 对本仓库的判断

| 范围 | 可行性与工作 |
| --- | --- |
| Path、Chart、SVG 路径、Canvas fill/stroke | 高；转换 PathCommand 到 kurbo/Scene，保留填充规则、圆弧、线帽、连接与变换语义 |
| 替换现有 GPU 路径后端 | 可行；在当前离屏纹理边界接入，再由 Jadix 贴图池合成，先保持其它 UI 管线 |
| CPU 回退 | 可保留当前 RasterCanvas；如采用 vello_cpu，另验像素与颜色空间语义 |
| GLES/WebGL 与弱 GPU | 原始 compute 型 vello 无法覆盖；vello_gpu 更贴近现有 fragment-only 默认策略，仍须实际平台验收 |
| 全框架 2D（文字、UI、背景效果） | 中长期重构；需要改绘制列表、遮罩与层合成，保留文字 shaping、输入法与布局职责 |
| Live2D | 与向量路径不是相同问题；已移到 Animet，网格变形/纹理遮罩渲染不应混入 Vello 替代结论 |

主要硬障碍是 GPU 类型身份：本 workspace 的 `wgpu` 实际为自有包 `jadix_wgpu`
0.1.0（基线 wgpu 30）。Vello 上游依赖名为 `wgpu` 的包；即使版本相近，也不能
将两个包的 Device、Queue、Texture 互传。普通新增 `vello = ...` 不会自动复用
Jadix 的设备。接入前必须固定 Vello 发布版/commit，核对其 wgpu 主版本，再统一依赖
到同一个包（回归上游 wgpu，或维护 Vello 的依赖适配）。仅在本 workspace 改一个
依赖别名不足以改变 Vello 的传递依赖。

颜色与能力也需逐项核对：本管线在 RGBA8 sRGB 纹理上进行 coverage resolve，
CPU API 对外返回 straight-alpha RGBA；替代后必须确认预乘 alpha、线性/显示色、
渐变插值、透明边缘与裁剪一致。Canvas 的命中测试和同步 get/put_image_data
仍需要 CPU 几何或像素路径；GPU 渲染器自身不能消除这些职责。

## 建议的验证顺序

1. 固定一个 Vello 版本和统一的 wgpu 包，验证共用 Device/Queue/离屏目标；先在
   DX12 运行，记录依赖体积与初始化成本。
2. 通过 `jadix_2d` 的路径/Canvas 边界接入实验后端，复用现有曲线、渐变、裁剪、
   亚像素抗锯齿与 readback 像素回归；保留现有后端以便比较。
3. 测 Gallery/图表/动态 Canvas 的冷启动、静态缓存、连续绘制 CPU/GPU 时间、
   内存、纹理上传与提交次数，再验证 GLES/WebGL、Metal、移动端。
4. 只有这些结果支持收益后，再考虑统一文字、UI 实例与效果管线；没有测量数据前，
   不推断吞吐量、帧耗时或包体必然改善。

当前建议：保持新 `jadix_2d` 边界，优先评估 `vello_gpu` 的兼容性；若目标明确
限定 compute-capable 桌面 GPU 且动态复杂向量占主导，再同时比较原始 `vello`。
以上是源码与官方文档推导的可行性判断，尚未实现 Vello 原型或性能基准。

## 本次拆分的验证

- 已有 GPU 回归：16 个单元测试和 1 个独立 Canvas 测试通过；1 个计时测试按设计忽略。
- 已有 GPU 回归：65 项通过。
- 同配置显式运行 `renderer::path_pipeline::tests::antialias -- --ignored --test-threads=1`：5 项真实 GPU 像素测试通过。
- Jadix 默认配置及禁用默认特性的库编译通过。
- Animet CPU 配置下 2 项 View 测试通过；`--features gpu-tests --test render` 的 GPU 遮罩、混合、变形和输入验证通过。真实授权模型测试仍需外部模型，未运行。
