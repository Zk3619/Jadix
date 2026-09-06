# RUI × EUI-NEO：视觉效果与 2D/3D

> **文档地位**：对照审计（非巩固期执行源）。排期与勾选以 [AUTHORITY.md](../AUTHORITY.md) 为准；底座禁区见 [ENGINEERING_GRADE_FRAMEWORK.md](../ENGINEERING_GRADE_FRAMEWORK.md) §2–§3；公开自绘面见 [drawing.md](../drawing.md)；脏区见 [damage.md](../engine/damage.md)；六参考总表见 [REFERENCE_COMPARISON.md](REFERENCE_COMPARISON.md)。  
> **对照源**：[`reference/EUI-NEO-main`](../../reference/EUI-NEO-main)（快照 · C++17 · Apache-2.0 · **0.5.x**）。GPUI 轴见 [`reference/gpui`](../../reference/gpui) · [`reference/gpui-component-main`](../../reference/gpui-component-main)（磁盘路径；文档树里写作 `Reference/`）。RUI 以 `crates/` 与工程文档为准。  
> **日期**：2026-09-06。此前对照把 EUI **压成「组件密度 + 脏矩形」**，漏掉它真正拉开观感的一层：图元级视觉、2.5D UI 平面、Shadertoy 叶、动态纹理。同日补 **§8 动画对照**（目标声明 / hover 平滑 / 属性 mask / 全局速率）。同日再补 **§9 与 GPUI / gpui-component 三角对照**（视觉 / 2.5D / 动画；生态密度仍见 [GPUI_ECOSYSTEM_COMPARISON.md](GPUI_ECOSYSTEM_COMPARISON.md)）。  
> **禁区不变**：不搬命令式 compose DSL、不换 GLFW/GL/Vulkan 双后端、不弃 wgpu、不把 workshop 件塞进 shadcn 稳定面；**不**为对齐 EUI 拆掉 RUI 已冻结的 `@keyframes` / presence / 句柄；**不**为对齐 GPUI 换 Entity / `Render`。

---

## 0. 一句话结论

EUI-NEO 的「绝佳视觉」**不是**另一套 3D 引擎，而是把 **高质量 2D 图元 + 真 2.5D 平面投影 + 用户 Shader 叶 + 动态纹理** 做成与 `rect`/`text` **同一套布局 / 裁剪 / 圆角 / 透明度 / 命中 / dirty rect**。动画「看起来强」也不是时间轴全家桶，而是 **只声明目标、Runtime 从当前值插值、hover/press 永不 compose**。RUI 的 wgpu 叶模型与 CSS 动效栈方向对；缺的是 **产品级 2.5D、用户 shader graph、指针驱动不 compose、创意目录**，以及若干 **产品配方**（全局速率、阴影藏 alpha、Gallery 同一 actor 多属性）。

| 层 | EUI-NEO 实际做的 | RUI 今日 | 该不该跟 |
|----|------------------|----------|----------|
| **2D 图元** | 圆角矩形：填充/渐变/描边/外阴影/内阴影/blur + 多边形 + SVG 光栅 | wgpu SDF 矩形/阴影/渐变；`backdrop-blur`；`<svg>` / Canvas2D | 学组合纪律，不换核 |
| **2.5D UI 平面** | `rotateX/Y` + `translateZ` + `perspective`；无 depth buffer；绘制序仍是树 + `zIndex` | CSS `transform` **解析了 3D 函数，resolve 时压成 2D**（`rotateX`→`scaleY(cos)`，`perspective` 空操作） | **该学**：这是卡片翻转 / tilt 的正确层 |
| **真 3D 场景** | **没有** mesh/camera/light 世界 | `<scene3d>` 实验 feature：透视立方 + camera/light/material | **RUI 已超前**；勿用 EUI 否定 scene3d |
| **用户 Shader** | 一等图元 `ui.shadertoy()`：`mainImage()` 多 Pass、Image/Buffer/Self、JSON 预设、双后端同一契约 | `effect_wrap("blur"\|"glass"\|…)` **具名** fx；无用户 shader graph | **该学契约**；实现走 WGSL/naga，opt-in |
| **动态纹理 / 视频** | `ImageStream`（NV12/I420/P010）；FFmpeg **只在示例**，核不链解码器 | Canvas `put_image_data` / Image 位图；无 YUV 流 | **已收口** [`engine/media.md`](../engine/media.md)：核只收帧；解码在应用；播放器后置 crate |
| **创意组件** | `components/workshop/`：tilt_card / 新拟态 / heart_switch / card_slider | B16/B17 **底座☑、目录 backlog**；inventory 曾标 workshop「不跟」 | **改口**：跟心智与分层，不跟 C++ API |

**路线**：用 EUI 当 **视觉产品形状** 与 **叶节点契约** 的标尺；实现仍走 RUI 的 Signal + wgpu + taffy。canvas-ui / react-bits 仍是创意目录对照；EUI 证明同一套事可以 **不嵌浏览器、不搬 HTML-in-canvas** 就做出来。

---

## 1. 现有对照漏了什么

[REFERENCE_COMPARISON.md](REFERENCE_COMPARISON.md) / [gap_analysis.md](gap_analysis.md) / inventory §3 把 EUI 的角色写成：

> 组件密度标尺 + 脏矩形 / 静态层缓存 + gallery 驱动

这在 **控件表** 上仍然对。视觉上漏了这些一等能力（均在 `reference/EUI-NEO-main`）：

| 源 | 能力 |
|----|------|
| [`docs/Shadertoy.md`](../../reference/EUI-NEO-main/docs/Shadertoy.md) | Shadertoy 图元契约：Pass graph、通道、uniform、热更新、与普通 UI 同语义 |
| [`docs/动画.md`](../../reference/EUI-NEO-main/docs/动画.md) · [`core/animation.h`](../../reference/EUI-NEO-main/core/animation.h) | `AnimatedValue` / `SmoothedValue`；属性 mask；全局时长缩放；2.5D lerp；视觉脏区 = `union(old, new)`。对照见 **§8** |
| [`docs/retained_layer_cache.md`](../../reference/EUI-NEO-main/docs/retained_layer_cache.md) | 静态子树 → 离屏层；backdrop blur / 动画 / 滚动 / 图 / SVG **切断缓存** |
| [`docs/渲染后端架构.md`](../../reference/EUI-NEO-main/docs/渲染后端架构.md) | Runtime 不知 GL/Vulkan；图元 facade 下发后端无关 draw |
| [`components/workshop/`](../../reference/EUI-NEO-main/components/workshop) | 创意件与基础控件分桶；CSS→DSL 分层移植 skill |
| [`examples/shadertoy.cpp`](../../reference/EUI-NEO-main/examples/shadertoy.cpp) · `animated_card.cpp` · `magic_wand.cpp` · `dynamic_texture.cpp` | 可运行的视觉样本，不是「有 API 没 Gallery」 |

控件密度 RUI 已经赶上或超过；**观感差距主要在这一层**，不是再抄一套 DatePicker。

---

## 2. EUI 视觉栈（三层，勿混）

EUI 自己写得很清楚：**2.5D ≠ 3D 场景**。对照时必须分三层，否则会把 `<scene3d>` 和卡片翻转做成同一件 API。

```text
┌─────────────────────────────────────────────────────────────┐
│ A. UI 2D 图元                                                │
│    rect / text / image / svg / polygon                       │
│    + shadow / insetShadow / blur / gradient / radius         │
├─────────────────────────────────────────────────────────────┤
│ B. UI 2.5D 平面投影（仍是 UI 树）                              │
│    rotateX/Y/Z · translate3d · perspective · transformOrigin │
│    绘制顺序 = DSL 树 + zIndex；无 depth buffer                │
│    默认 hit = 未变换 layout frame；可选 transformedHitTest    │
├─────────────────────────────────────────────────────────────┤
│ C. 叶内 GPU 内容（仍走普通 image 合成）                         │
│    Shadertoy graph → RGBA 目标 → 圆角/透明度/clip/transform   │
│    ImageStream 动态帧（YUV → 纹理）                            │
└─────────────────────────────────────────────────────────────┘
```

RUI 对应：

| EUI 层 | RUI 落点 | 缺口 |
|--------|----------|------|
| **A** | SDF 矩形、`filter` / `backdrop-blur`、`<svg>`、Canvas2D、Chart path 核 | 内阴影 / 多阴影层主要靠叠 Element；矩形 blur 不如 EUI 图元一等 |
| **B** | `style/transform.rs` 存了 `RotateX/Y`、`Perspective`、`Translate3d` | **resolve 压成 Mat2d**：`RotateX`→Y 向 cos 缩放，`Perspective` 恒等，`TranslateZ` 丢弃。翻牌看起来是压扁，不是透视 |
| **C-shader** | `effect_wrap` 捕获子树 + 具名 pass（`rui_fx:`） | 无用户 `mainImage` / 多 Pass / Buffer feedback / JSON 预设 |
| **C-3D** | `<scene3d>` 真 mesh 世界（opt-in feature） | EUI 没有这一层；RUI 勿为对齐而删 |
| **C-stream** | Canvas 像素回写 | 无 NV12/P010 上传契约 |

---

## 3. 该学（按优先级）

下列都是 **契约 / 产品形状 / 纪律**，实现必须长在 wgpu + Signal 上。

### 3.1 Shadertoy 式用户 Shader 叶（P1 视觉 · 不挡 Beta）

EUI 把用户 fragment 做成 **普通 shape**：`.size` / `.radius` / `.opacity` / `.transform` / 祖先 clip / 命中 / dirty rect 全套可用。最终 Pass 走 **普通 UI image 合成**，不是另开全屏 overlay。

可抄的契约（见其 `docs/Shadertoy.md`）：

| 项 | EUI | RUI 该怎么接 |
|----|-----|----------------|
| 入口 | `void mainImage(out vec4, in vec2)` | **WGSL** 入口 + naga；禁止默认依赖运行时 GLSL 编译器 |
| Graph | 顺序多 Pass；每 Pass 四路 `sampler2D` | 复用已有 `RenderGraph` / `TexturePool` |
| 通道 | `Image` / `Buffer` / `Self` / `None` | 同语义即可；图片走现有解码缓存，不新开媒体栈 |
| 时钟 | `iTime` / `iFrame` + `timeScale` / `paused` / `resetKey` | 与 `tick_animations` / `request_frame` 对齐；改圆角不得重置 feedback |
| 指针 | 仅图元局部 `iMouse`；按下在内则捕获到释放 | 与 pointer capture 一致，勿订根 effect |
| 失败 | 候选 program/pipeline；失败保留上一有效帧 + 结构化错误 | 同：热更失败不黑屏 |
| 体积 | 固定 RGBA32F ping-pong；明确 **不做** cubemap/3D/audio/video channel | 默认 feature **关闭**；JSON 预设可后补 |

**不该抄**：OpenGL 驱动编译 GLSL 当默认；Vulkan 预编译 SPIR-V 双轨；把 Shadertoy 做成第二窗口系统。

映射：B16 全幅背景 / 粒子叶的实现路径；公开标签倾向 `<shadertoy>` / `shadertoy()` **实验 feature**，与 `<scene3d>` 同级 opt-in。规范仍归 [drawing.md](../drawing.md)，不新开 WP。

### 3.2 真 2.5D UI 平面（P1 观感 · WP-3.13 加深）

EUI Gallery Animation 页用三个按钮验收：`Flip X`（`rotateX`）、`Flip Y`（`rotateY`）、`Depth`（`translateZ + perspective`）。文档原话：

> 当前 2.5D 是 UI 平面投影，不是完整 3D 场景：绘制顺序仍由 DSL 树顺序和 `zIndex` 决定，不启用真实 depth buffer。

这正是 RUI 该补的层：

- **要**：控件树上的透视投影矩阵（至少 `rotateX/Y` + `perspective` + `transform-origin`），子树继承，脏区用投影后 AABB。
- **不要**：为翻一张卡片去走 `<scene3d>` mesh；也不要 `preserve-3d` 真 3D 层叠当默认。
- **命中**：默认未变换 layout box（EUI 如此）；需要跟手时再开 `transformedHitTest` 等价物。
- **文字**：已有 Bake / MSDF / `scene3d().label` 路径（drawing.md §5）；2.5D 卡片上的字应走 Bake/MSDF，而不是再压一次 2D cos 缩放。

RUI `func_to_mat2d` 把 `RotateX` 做成 `scaleY(cos)`、`Perspective` 空操作——API 面看起来「有 3D」，像素上没有透视。加深时改 **resolve + 绘制**，不要再加一套平行 API。

### 3.3 指针倾斜不 compose（P1 · 与 Solid 同向）

Workshop skill 写明：

> For pure visual pointer-follow transforms, prefer Runtime binding (`.runtimePointerTiltFrom` / `.runtimePointerTransformFrom`) over `mouseArea.onMove` state maps so hover movement does not recompose.

`tilt_card.h` 把 perspective + `runtimePointerTiltFrom(hitId, maxTilt, scale)` 绑在稳定 id 上，**hover 移动不改 compose 树**。

RUI 已有 `bind_*` / `untrack` / paint 内 `peek`。缺的是一条 **运行时指针→transform 的专用通道**（不经过 `set` 整卡状态）。否则 tilt 会变成每帧 Signal 风暴，和细粒度目标相反。

映射：B16 tilt / B17 hover 倾斜件的 **引擎原语**；实现落 `bind_style` 或 renderer 侧 pointer 采样，禁止根 `create_effect` 每帧写树。

### 3.4 Workshop 分桶与 CSS→层移植（P1 产品目录）

EUI 把基础控件放 `components/`，创意件放 `components/workshop/`，并规定：

1. 调用方像 QML 短 Builder；内部仍是普通 DSL 层叠。
2. CSS `background/shadow/::before` → 多层 `rect`；多阴影 = 多层透明 rect，藏阴影时 **保 RGB、只把 alpha 置 0**。
3. 稳定内部 id：`id + ".part"`；为动画不要按 hover 增删层。
4. SVG 保真时内联 path + tint，不拿 polygon 糊近似。

这与 [drawing.md](../drawing.md) §6 B16/B17、ADR-6（默认面仍是 shadcn）一致。inventory §3 旧行「heart_switch / neumorphic / tilt_card / 不跟」应读成：**不进 SemVer 稳定面**，不是「视觉上禁止参考」。

可跟的样本（只跟形状，不跟 C++）：

| EUI | 学什么 | RUI 落点 |
|-----|--------|----------|
| `tilt_card` | 2.5D + 指针绑定 + 渐变面 + 独立阴影层 | B16 目录 |
| `neumorphic_button` | 双阴影（外亮/外暗或 inset）叠层 | 创意件；稳定 Button 不改默认美学 |
| `animated_card` 示例 | 一卡同时 anim Frame/Color/Radius/Shadow/Transform | 已有 CSS transition；Gallery 补一页验收 |
| `magic_wand` 示例 | 弹簧跟手 + 图 blur 揭示（应用层 spring） | B17；spring 已在 animation 引擎，勿再抄一份 Euler |

### 3.5 视觉脏区与保留层（P2 · 已有 B8）

EUI 动画推进：

```text
dirty = union(oldVisualRect, newVisualRect)
```

visual rect 含 **frame + shadow + blur + transform**（含父 2.5D 继承）。然后 framebuffer cache + scissor。

保留层缓存（`docs/retained_layer_cache.md`）的「谁不能进缓存」可直接当 B8 验收表：

- backdrop blur / dependent visual → 切断兄弟段
- 动画、交互、滚动、timer、frame callback、image、SVG → 不缓存
- 面积/成本阈值 + **连续稳定两帧** 才建层
- 新建层 **当帧不采样**，下一帧再 blit
- 标题统计 `Layer H/M/D/Re`（命中/未命中/绘制/重建）

RUI [damage.md](../engine/damage.md) 四层模型已经对：逻辑 PartialLoad 正式、GPU retain = B8。EUI 补的是 **视觉外接盒** 与 **层缓存排除表**，不是换脏区哲学。

### 3.6 动态纹理：核收帧、应用解码（P2 观察）

`ImageStream` + `examples/dynamic_texture.cpp`（NV12 / I420 / P010 轮换）+ `ffmpeg_video_player`（**可选示例，核不链 FFmpeg**）。

学：YUV/动态帧是 **image 叶的一种 source**，不是第三套渲染器。RUI 若做视频/摄像头，应是 `Image` / Canvas 的 source 变体 + opt-in feature，解码器留在应用或独立 crate。分层契约已收进 [`engine/media.md`](../engine/media.md)（B18）。

### 3.7 Gallery 视觉验收页（P2 DX）

EUI Animation 页把 Move / Rotate / Fade / Scale / Radius / Glow / FlipX / FlipY / Depth 做成 **同一 actor 的按钮切换**。这比文档更能锁住 2.5D 回归。

RUI Gallery「绘制」已有 Svg/Canvas/Scene3d。加深 3.2 后应加 **Flip/Perspective actor**，不要只靠单位测 `mat2d`。

---

## 4. 不该搬

| 禁 | 为什么 |
|----|--------|
| 命令式 `compose(Ui&, Screen)` 整页重建 | ADR-1；EUI 动画文档自己说 hover 尽量不 compose，主模型仍是整页 |
| GLFW/SDL2 + OpenGL/Vulkan 双后端当 RUI 主线 | ADR-3 wgpu；Windows 默认 GL 已是 wgpu backend 选择 |
| FreeType 独占文本 / 弃 glyphon | 文本栈已锁定 |
| 把 workshop 当默认组件面 | ADR-6 shadcn；创意库 `experimental` |
| 用 `<scene3d>` 模拟每张 tilt 卡 | 成本与命中都错层；2.5D 才是 UI 平面 |
| 默认 feature 塞 glTF / Shadertoy / FFmpeg | 与 drawing.md 禁区、体积路径一致 |
| 运行时 GLSL 编译器进默认依赖 | wgpu 的优势是 naga/WGSL；用户 shader 必须 opt-in |
| 为对齐 EUI 砍 AccessKit / 细粒度 | EUI 明确非 a11y 一等；这是 RUI 护城河 |
| 无 timeline/keyframe 的动画模型 | EUI **缺** `@keyframes` / 完成回调；RUI 已领先，禁止倒退。动画该学什么见 **§8** |

EUI 动画系统明确限制：无 timeline、无 repeat/yoyo、无完成回调、无句柄。RUI 的 `keyframes` / `motion` / `Animation` 句柄 / `Timeline` **不要为「更像 EUI」而拆掉**。该抄的是目标声明、hover 不重组树、阴影藏 alpha、Gallery 同一 actor，不是变窄引擎。

---

## 5. 对照表（代码事实 · 2026-09-06）

| 能力 | EUI-NEO | RUI | 结论 |
|------|---------|-----|------|
| 圆角矩形 + 渐变 + 边框 + 阴影 | 图元一等，可动画 | SDF 主路径；shadow 有；inset 靠叠层 | 齐骨架；组合密度 EUI 更顺 |
| 图元 blur / 多阴影 | `.blur` / `.insetShadow` | `filter` / `backdrop-blur` 管线 | 毛玻璃 RUI 不弱；卡片 glow 跟 EUI 学叠层 |
| SVG | nanosvg 光栅 + `ui.svg` 内联 | resvg 场景图 + Lucide | RUI 更完整 |
| Canvas 2D | 无独立 WHATWG 叶（自绘靠 shader/图元） | `<canvas>` + Canvas2D MVP | RUI 更完整 |
| 2.5D 透视 | 真投影 | 解析有、resolve 压扁 | **EUI 领先** |
| mesh 3D | 无 | `<scene3d>` opt-in | **RUI 领先** |
| 用户多 Pass shader | Shadertoy 一等叶 | 具名 `effect_wrap` | **EUI 领先**（契约可移植） |
| 子树特效 wrap | 无独立 canvas-ui 包 | `effect_wrap` 底座☑ | **RUI 领先底座**；缺目录 |
| 指针 tilt 不重组树 | Runtime binding | 需自写 Signal | **EUI 领先 DX** |
| CSS `@keyframes` / WAAPI 子集 | 无 | 已冻结子集 | **RUI 领先** |
| 目标声明 + 中断续插 | `AnimatedValue` 从 current 改道 | `motion` + transition retarget / spring `vel` | **模型齐**；EUI Gallery 用得更满 |
| hover/press 平滑 | `SmoothedValue` + `states`，不 compose | `hover:` + `transition` | 学指数平滑与 `visualStateFrom` |
| 全局动画速率 | 连续时长缩放 0.05–8× | `prefers-reduced-motion` 跳终态 | 学速度条；勿替代 reduce |
| 2.5D 分量 lerp | rotateX/Y/Z + perspective 独立插 | packed transform；resolve 压 2D | 透视像素仍缺（§3.2 / §8.3） |
| 脏矩形 | 默认路径 + 视觉外接盒 | 逻辑策略正式；GPU FullClear 默认 | 学视觉盒；GPU retain = B8 |
| 静态层缓存 | 自动、有排除表 | B8 规划 | 抄排除表 |
| GIF / 远程图 / 取色 | 内置 | Image + `remote-asset` | 齐思路 |
| 视频 | ImageStream；解码在示例 | 无 | **B18** [`media.md`](../engine/media.md)；不进默认 |
| 创意目录 | workshop 4 件 + calgary_gallery 产品 demo | B16/B17 目录 backlog | 跟分桶 |

---

## 6. 落到现有计划（不新开 WP）

本文 **不** 增加 AUTHORITY 工作包。加深全部挂已有项：

| **学到的东西** | **挂靠** | **何时** |
|------------|------|------|
| 真 2.5D resolve + Gallery Flip/Depth | [drawing.md](../drawing.md) §5（WP-3.13 能力层加深） | 不挡 Beta；B16 tilt 的前置 |
| 用户 shader 叶（WGSL graph） | B16 背景/全幅叶；drawing 公开自绘面实验 feature | 不挡 Beta；默认关 |
| 指针→transform 运行时通道 | B16/B17 + `bind_*` / peek | 与 tilt 件一起 |
| workshop 分桶 / CSS 叠层 | B16/B17 产品目录；inventory 标明 experimental | 目录仍 backlog |
| 视觉脏区 AABB（shadow/blur/transform） | [damage.md](../engine/damage.md) B8 输入 | 与 B8 同期 |
| 层缓存排除表 | B8；backdrop 切断 = 已有 B3 依赖 | ≥ B3 |
| ImageStream / YUV | **B18** [`media.md`](../engine/media.md) / [`plans/media-av.md`](../plans/media-av.md) | 不挡 Beta；核不链 FFmpeg |
| 目标声明 / 中断续插（`AnimatedValue`） | 已有 [`motion.rs`](../../crates/rui/src/utils/motion.rs)；产品上把 Gallery 做成「一 actor 多属性」 | 配方 / Gallery，不改冻结面 |
| hover/press `SmoothedValue` + `visualStateFrom` | `hover:` + `transition` 已能走；缺指数平滑与跨节点共享 press | 组件配方；禁止每帧 `set` 整卡 |
| 阴影「藏 alpha、保 RGB」 | `box-shadow` 已在 `PropertyId`；写件时勿把 none 写成黑透明 | 作者纪律；workshop 移植 |
| 全局时长缩放（Gallery 速度条） | 已有 `prefers-reduced-motion` 跳终态；连续倍率是产品设置 | Gallery / settings；不挡 Beta |
| `OutExpo` ≈0.92s 抽屉手感 | 参考值，不是新 easing 函数 | Drawer 产品打磨 |

---

## 7. 交叉文档

| 文档 | 关系 |
|------|------|
| [drawing.md](../drawing.md) | Svg / Canvas / 2D·3D / B16 / B17 领域规范；本文是 EUI 视觉对照 |
| [animation_and_motion.md](../engine/animation_and_motion.md) | RUI 动效**冻结手册**；引擎/keyframes/presence **领先** EUI 与 GPUI 核。EUI 产品模型见本文 **§8**；GPUI 三角见 **§9**；手册附录 §20 / §21 只作指针 |
| [REFERENCE_COMPARISON.md](REFERENCE_COMPARISON.md) | 六参考总表；EUI 角色从「仅密度」扩到「密度 + 视觉契约」 |
| [gap_analysis.md](gap_analysis.md) | 历史密度审计；视觉层以本文为准 |
| [component_inventory.md](../components/component_inventory.md) §3 | 控件密度标尺；workshop = 创意目录参考，非稳定面 |
| [damage.md](../engine/damage.md) | 脏区四层；EUI 视觉盒 / 层排除表 |
| [AUTHORITY.md](../AUTHORITY.md) | 唯一勾选；B16 / B17 / B8 / WP-3.13 |
| [size_and_memory.md](../ops/size_and_memory.md) | 禁止为体积弃 wgpu 去追 EUI OpenGL 量级 |
| [GPUI_ECOSYSTEM_COMPARISON.md](GPUI_ECOSYSTEM_COMPARISON.md) | GPUI 生态专章（原生 / 密度 / Entity）；**视觉·动画轴**见该文 **§6.5** 与本文 **§9** |

---

## 8. 动画对照（2026-09-06）

> **地位**：对照审计，**不改** [`animation_and_motion.md`](../engine/animation_and_motion.md) 冻结面（`PropertyId` 64 槽、模块边界、presence 用 `animation_run_active`）。  
> **源**：[`docs/动画.md`](../../reference/EUI-NEO-main/docs/动画.md) · [`core/animation.h`](../../reference/EUI-NEO-main/core/animation.h) · [`examples/animated_card.cpp`](../../reference/EUI-NEO-main/examples/animated_card.cpp) · Gallery `gallery_animation.h`。

### 8.1 为什么「看起来强」

EUI 动画引擎比 RUI **窄**：没有 `@keyframes`、repeat/yoyo、完成回调、句柄、`steps()`、CSS `spring()`。强在 **产品模型**：

```text
app 改一个 bool
  → compose 只写新的目标属性（x / color / radius / shadow / rotateY…）
  → Runtime 发现目标变了
  → AnimatedValue 从 *当前值* 插到新目标（可中断、可改道）
  → 每帧只标 visual dirty
hover / press 根本不走 compose：SmoothedValue 指数靠近
```

这和 RUI [`utils/motion.rs`](../../crates/rui/src/utils/motion.rs)「只写目标、保 mid」是同一句话。EUI Gallery / `animated_card` 把这句话用满了；RUI 引擎更完整，**缺的是同一 actor 上同时 morph 多属性的演示与组件配方**。

### 8.2 两套时钟（该学）

| 类型 | 用途 | 曲线 | 会不会 compose |
|------|------|------|----------------|
| `AnimatedValue<T>` | 状态切换：展开卡、翻牌、改颜色 | duration + `Ease` + delay | 只在 **目标** 变时 compose 一次 |
| `SmoothedValue<T>` | hover / press 持续跟目标 | `1 - exp(-speed * dt)` | **永不** compose |

`setTarget` 从 `current_` 起跳（不是从旧 `start_`），所以连点 / 中途反向不会跳。RUI transition retarget + spring 保 `vel` **已经强于** 这条；该抄的是 **DX**：页面不手写插值。

Hover 路径：

```text
.states(normal, hover, pressed)
Runtime 维护 hoverBlend / pressBlend
颜色：normal → hover → pressed
.visualStateFrom("button.bg", 0.965)  // 外壳 scale 共享命中 rect 的 press
```

RUI 对应：`hover:` / `active:` 变体 + `transition-*` 改 specified，tick 插值。Solid 下 hover **不必**重建组件函数。缺口是：

1. **指数平滑**（duration-less）—— CSS transition 总有固定时长；按住/松开的「跟着走」更像 `SmoothedValue`。
2. **跨节点共享交互态**（`visualStateFrom`）—— 背景 rect 的 press 驱动外壳 scale，不必再包一层 Signal。
3. **指针 tilt**（§3.3）—— 与 SmoothedValue 同类：Runtime 绑定，不 compose。

禁止：为对齐 EUI 在 `on_pointer_move` 里 `set` 整棵卡的 transform。

### 8.3 可动画属性 vs RUI `PropertyId`

EUI `AnimProperty` 是 **位掩码**，节点上 `.animate(Color | Radius | Shadow)` 声明「哪些属性吃这份 `Transition`」：

| EUI | 插值内容 | RUI | 结论 |
|-----|----------|-----|------|
| `Frame` | x/y/w/h（绝对定位 actor） | `width`/`height`/`left`/`top`… **C 层 Layout** | **勿把 Frame 当 taffy 几何的廉价 tween**；卡片 morph 优先 `transform` / `max-height` / `translate` |
| `Color` / `TextColor` | RGBA | `background-color` / `color` | 齐 |
| `Opacity` | 标量 | `opacity` | 齐 |
| `Radius` | 圆角 | `border-radius` + 四角 | 齐 |
| `Border` | 宽 + 色 | `border-width` + `border-color` | 齐（分槽） |
| `Shadow` | offset / blur / spread / color / inset | `box-shadow` 列表 lerp；inset 不对齐则 discrete | 齐骨架；见 §8.4 |
| `Blur` | 图元 blur | `filter` | 齐思路 |
| `Transform` | translate / scale / rotate / **rotateX/Y / translateZ / perspective / origin** 分量 lerp | packed `transform` + 2D longhand；3D 函数 **resolve 压扁** | **插值表有了，像素透视没有**（§3.2） |

`.animate(mask)` 的产品价值：同一 `Transition` 时长，子节点各自声明参与项（背景走 Color+Radius+Shadow，正文只走 Frame+Opacity）。RUI 用 `transition-property` 已经能做；Gallery「ANIM 对齐」偏 CSS 子集验收，**缺 EUI 那种一张卡同时 Move/Radius/Glow/Flip 的产品页**。

### 8.4 阴影与「关掉」的插值（该学纪律）

EUI `lerpValue(Shadow)`：

- `enabled == false` 时 **只把 `color.a` 置 0**，RGB / offset / blur 仍参与 lerp。
- `inset` 在 `amount < 0.5` 取 from，否则取 to（离散）。

Workshop skill 同一条：藏阴影时 **保 RGB、只改 alpha**，不要切成「透明黑」，否则中间帧会脏一下。

RUI `box-shadow` 已在冻结矩阵（§13.2）；作者写件时对齐这条纪律即可，**不必扩 `PropertyId`**。

EUI 自己警告：shadow / blur 动画成本高，慎用。RUI 同样：`LayoutImpact::Paint`，脏区必须含旧+新视觉盒（§3.5）。

### 8.5 全局速率 vs 减弱动效

| | EUI | RUI |
|--|-----|-----|
| 减弱 | 无系统级 `prefers-reduced-motion` | `prefers_reduced_motion()` / `RUI_REDUCED_MOTION`：**时长跳到区间末端** |
| 连续倍率 | `setGlobalTransitionDurationScale(0.05–8)`；只影响 **新** `setTarget`，不改进行中的动画；`Transition::none()` 不受影响 | 无产品级速度条 |
| Gallery | Settings「Animation speed」显示倍率，底层写 `1/speed` | — |

该学：Gallery / 应用设置里一条 **连续时长缩放**（调试手感、演示放慢），与 a11y **reduce = 跳终态** 是两件事，不要用速度条冒充 reduced-motion。

不改冻结子集；实现可挂现有 `Timing` 输入或一层 wrapper，须保证进行中的 run 不被追溯改写（EUI 已写明）。

### 8.6 手感配方（可抄数字，不换引擎）

| 场景 | EUI | RUI 现状 | 建议 |
|------|-----|----------|------|
| 抽屉 / 侧栏 / 底栏 | `OutExpo` + 约 **0.92s**：「先快出现，再慢慢收住」，无回弹 | Drawer 走 `@keyframes` + `translate_pct` | 打磨时长/easing 时用这个当参照；不要为此新增 `Ease` enum |
| 卡片展开 | 0.34s `OutCubic`；Frame+Color+Radius+Shadow+Transform 一份 `Transition` | 有 transition / motion | Gallery 补一页 `animated_card` 对位 |
| 翻牌 | 0.42s `OutBack` + `rotateY`/`perspective` | 2.5D resolve 仍压 2D | 先 §3.2，再抄时长 |
| 跟手弹簧 | `magic_wand` **应用层** Euler（stiffness 200, damping 25） | CSS `spring()` 已是引擎能力 | 不要再抄一份手写弹簧 |

### 8.7 明确不跟（动画）

| 禁 | 为什么 |
|----|--------|
| 用 EUI 的「无 keyframe」当先进 | RUI `@keyframes` / presence / 句柄 / `steps()` **已领先**；拆掉等于倒退 |
| 把 `Frame` 插值搬到 taffy 节点当默认 | EUI 布局弱、绝对定位 actor 便宜；RUI 宽高动画已走 C 层 + `used_layout` |
| hover 每帧改 compose 树 | 连 EUI 都禁止；与 Solid 细粒度冲突 |
| 为阴影动画扩 `PropertyId` 槽 | 64 槽已满；现有 `box-shadow` 够 |
| 全局速度条替代 `prefers-reduced-motion` | reduce 必须是跳终态，不是放慢 |

引擎对照一句话：**RUI 时间轴更完整；EUI 目标声明 + 交互平滑 + 2.5D 分量 lerp 的产品形状更满。** 学后者，留前者。

---

## 9. 三角对照：EUI × GPUI / gpui-component × RUI（2026-09-06）

> **地位**：对照审计，不改冻结面、不新开 WP。生态密度 / IME / Entity 仍以 [GPUI_ECOSYSTEM_COMPARISON.md](GPUI_ECOSYSTEM_COMPARISON.md) 为准；本节只补 **视觉 / 2.5D / 动画** 轴（2026-09-02 专章几乎没写这一层）。  
> **源**：GPUI [`gpui/src/elements/animation.rs`](../../reference/gpui/gpui/src/elements/animation.rs) · [`svg.rs` `Transformation`](../../reference/gpui/gpui/src/elements/svg.rs) · [`canvas.rs`](../../reference/gpui/gpui/src/elements/canvas.rs)；gpui-component [`crates/base/src/motion.rs`](../../reference/gpui-component-main/crates/base/src/motion.rs) · [`crates/base/src/animation.rs`](../../reference/gpui-component-main/crates/base/src/animation.rs)。

### 9.1 一句话

GPUI 生态**不是** EUI 那种「一张卡同时 morph + 真透视 + Shadertoy」的视觉竞品。它强在 **原生深度 + 商业控件密度**；动画是 **时间轴闭包包装器**（GPUI）叠 **目标值 `transition` / `spring`**（gpui-base）。创意观感跟 EUI 学；时间轴全家桶留 RUI；表格 / 编辑器 / Dock 跟 gpui-component。

### 9.2 视觉三层：GPUI 落在哪

§2 的 A / B / C 放到 GPUI 上会立刻分清「看起来也是 GPU UI」和「产品形状」：

| 层 | EUI | GPUI / gpui-component | RUI |
|----|-----|----------------------|-----|
| **A. 2D 图元** | 圆角矩形 + 渐变 + 内外阴影 + blur 一等 | Scene 批绘：quad / shadow / path（lyon）/ sprite / surface；样式有 `box-shadow` | wgpu SDF + `backdrop-blur` + `<svg>` |
| **B. 2.5D UI 平面** | **真透视** `rotateX/Y` + `perspective` | **无**。`Transformation` 只有 2D scale + translate + rotate；注释写明 **不影响 hit / layout，只影响绘制** | 解析了 3D 函数，**resolve 压成 2D** |
| **C-shader** | Shadertoy 一等叶 | **无**用户 shader graph | `effect_wrap` 具名；无用户 graph |
| **C-3D mesh** | 无 | 无 | `<scene3d>` opt-in **领先** |
| **C-canvas** | 无独立 WHATWG 叶 | `canvas(prepaint, paint)`：**立即**走 Scene API，不是 WHATWG Canvas2D | Canvas2D MVP **更完整** |
| **C-stream** | `ImageStream` YUV 跨平台；解码在示例 | macOS `CVPixelBuffer` `surface`，**不是**跨平台 YUV | 无 YUV |
| **图表** | 自绘图元 | `chart` + `plot`（area/bar/line/pie/radar/sankey/K 线）走 path | `chart` grammar experimental |
| **创意目录** | `workshop/` 与基础控件分桶 | **不是**产品叙事；Skeleton / Spinner 是控件动效 | B16/B17 目录 backlog |

GPUI 的 `canvas()` 文档自己写：*short term custom drawing*。它是「临时画一笔」，不是 EUI Shadertoy / RUI Canvas2D 那种叶契约。Spinner 用 SVG `Transformation::rotate` 转圈——这是 **2D 旋转**，不要当成卡片翻转。

### 9.3 动画两层（勿混）

GPUI 生态把动画拆成两层，和 EUI / RUI 的对应关系不同：

```text
GPUI 核
  with_animation(id, Animation, |el, delta| …)
    duration · oneshot / repeat / repeat_synced
    easing 闭包 · request_animation_frame 直到 done
    reduce_motion：oneshot → 终态；repeat → 始态；不再排帧

gpui-base（组件库才有）
  transition(id, target, policy)   从采样点改道；位置连续、速度不连续
  spring(id, target, Spring)       改道保速度
  Spring::with_travel(false)       拖拽跟手，不滞后指针
  EffectTransition                 fade / slide / width / height → 包成 with_animation
                                   （旧名 Transition 已 deprecated）
```

| | EUI | GPUI 核 | gpui-base | RUI |
|--|-----|---------|-----------|-----|
| **作者写什么** | 只写目标；Runtime 从 current 插 | 每帧闭包改元素（opacity / transform…） | render 时采样目标值 | CSS `transition` / `@keyframes` / `motion` / `spring()` |
| **keyframes** | 无 | 无 | 无 | **已冻结领先** |
| **repeat** | 无 | `repeat`（各自 `start`）/ **`repeat_synced`（全 App **一个**纪元）** | 借 GPUI | CSS `animation-iteration-count`；错落用 `animation-delay` |
| **中断改道** | `AnimatedValue` 从 current | 闭包自己算；无属性表 | `transition` 从采样点；`spring` 保 `vel` | `motion` + transition retarget + spring `vel` |
| **hover 平滑** | `SmoothedValue` **永不 compose** | Entity `render()` 每帧重建元素；动画靠 element state | 同左 | `hover:` + CSS transition；Solid **不必**重建组件函数 |
| **reduce motion** | 无系统级 | **一等**：绑动画帧泵 | 跳终态、不 `request_animation_frame` | `prefers-reduced-motion` **跳终态** |
| **产品用法** | 一张卡 Color+Radius+Shadow+Flip | SVG 旋转示例；Skeleton 呼吸；Spinner 转圈 | Dialog / Sheet / Accordion **入场**，不是 morph 卡 | ShadCN 件 + ANIM Gallery；创意目录 backlog |
| **cubic-bezier** | `Ease` 枚举 | 核只有 linear / quadratic / ease_in_out / ease_out_quint / bounce / pulsating | **CSS 忠实求解**（对照 Chromium；先解 x 再采 y） | CSS `cubic-bezier()` 已在冻结子集 |

gpui-base 自己写得很清楚：**duration 改道连续位置、不连续速度；spring 才保动量。** 这和 RUI `motion` + CSS `spring()` **同构**。该对齐的是叙事与 `with_travel(false)` 拖拽配方，不是再抄一套引擎。

`repeat_synced` **不是**「一组灯的命名时钟」。相位公式是 `(now - App.synced_animation_epoch) % duration / duration`：全 App **一个**纪元。同一 `duration` 的元素锁相（后挂上的也跳到当前相位，测试里 250ms 后才 mount 的第二个灯直接是 `0.5`）。API **没有** `delay` / 相位偏移 / 分组 epoch。

交叉错落因此要另做：

| 要的效果 | 做法 |
|----------|------|
| 齐刷刷同起同落（列表 Skeleton、后插入的行也合拍） | `repeat_synced`，同 duration |
| 波浪 / 交错呼吸 | **仍用** synced 当共同钟，闭包里 `(delta + offset) % 1`；或不用 synced，各自 `.repeat()` 靠 mount 时间自然错开 |
| 几组各自合拍、组间错开 | GPUI **没有**命名组；自己备第二条时间源，或同 epoch 不同 offset |

RUI 对齐：齐刷刷用一条共享时间源（或相同 `animation-delay` 起点）；波浪用 **不同** `animation-delay`，不要误以为「共享相位」= 不能错落。

### 9.4 产品观感切割

| 你要的观感 | 向谁学 | 不要向谁学 |
|------------|--------|------------|
| 卡片翻转 / tilt / 新拟态 / Shader 背景 | **EUI** workshop + Shadertoy | GPUI（没有 2.5D / shader 叶） |
| 抽屉 / Dialog 入场手感 | EUI `OutExpo` 数字 **或** gpui-component Sheet 时长 | 不要为此换 easing 枚举 |
| 呼吸灯齐刷刷同相位 | **GPUI `repeat_synced`**（同 duration） | EUI（无 repeat） |
| 呼吸灯交叉错落 | synced + 闭包 offset，或 `.repeat()` 各走各的；RUI 用 `animation-delay` | 以为「共享相位」就做不了波浪 |
| 拖拽跟手、中途反向不跳 | EUI `SmoothedValue` **或** gpui-base `spring` + `with_travel(false)` | GPUI 纯闭包自己积分 |
| 表格 / 编辑器 / Dock / 块级 MD | **gpui-component** | EUI 密度表（控件形状不同） |
| `@keyframes` / presence / 句柄 / `steps()` | **RUI 已有** | 不要拆掉去「更像 GPUI 闭包」 |

### 9.5 该学 / 不学（GPUI 轴）

挂已有 B16 / B17 / B8 / WP-3.13 / 冻结动效手册，**不新开编号**。

| 学 | 为什么 | 不学 |
|----|--------|------|
| `repeat_synced`：同 duration 锁相；后插入也合拍 | 列表 Skeleton 不会各走各的钟。错落是闭包 `(delta+off)%1`，API 无分组 epoch | 用 Entity `render()` 换 Signal；把「共享相位」理解成不能做波浪 |
| `reduce_motion` 与帧泵绑定 | oneshot 跳终态、repeat 停在始态、不再排帧；RUI 已有类似，文档对齐即可 | 用 Metal/DX 双栈换 wgpu |
| gpui-base `transition` / `spring` 目标值 API | 与 RUI `motion` 对齐叙事；**改道纪律写进注释** | 把 `EffectTransition`（fade/slide）当唯一动效模型——会倒退掉 keyframes |
| `Spring::with_travel(false)` | 拖拽 / resize **不得滞后指针** | 用 2D SVG `rotate` 冒充 2.5D 翻牌 |
| Scene 原语批绘 | 已在生态专章；B8 | 把 GPUI `canvas()` 当 WHATWG Canvas2D 缺口（RUI 已更完整） |
| `cubic_bezier` 先解 x 再采 y | CSS 手感；RUI 已有 `cubic-bezier()`，对照测可抄纪律 | 为对齐 GPUI 在 `on_pointer_move` 里 `set` 整棵卡 |

### 9.6 落到现有计划

| 学到的东西 | 挂靠 | 何时 |
|------------|------|------|
| 真 2.5D / Shadertoy / workshop | 仍是 **EUI §3 / §6**，不是 GPUI | 不挡 Beta |
| `repeat_synced` 心智 | 齐刷刷：一条共享时间源。波浪：`animation-delay` 错开，或采样后加 offset | 配方；不扩 `PropertyId` |
| spring 拖拽跟手 | 已有 CSS `spring()`；组件里拖拽路径设「即时吸附 / 不 travel」 | 作者纪律 |
| Scene 批绘 | B8 | ≥ B3 |
| 图表密度 | gpui-component `chart`/`plot` 当 **能力清单**；实现走 RUI path / Canvas | W3 产品面 |

三角一句话：**视觉产品形状跟 EUI；商业控件密度跟 gpui-component；时间轴与 Canvas2D / scene3d 留 RUI。GPUI 核贡献的是 `repeat_synced`、reduce-motion 帧泵、Scene 批绘，不是 2.5D。**
