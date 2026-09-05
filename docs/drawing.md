# RUI 公开自绘面：Svg · Canvas · 2D/3D

> **文档地位**：绘制原语与公开标签的**领域规范**（「是什么 / 禁区」）。  
> **执行勾选**：[`AUTHORITY.md`](AUTHORITY.md) **WP-3.2 / WP-3.11 / WP-3.12 / WP-3.13**；未来创意库 **B16**（canvas-ui）· **B17**（react-bits）。  
> **日期**：2026-08-13（W2 退出后、进入 W3 前落地；同日补 B17）；**2026-08-18 WP-3.11 Svg MVP ☑**；**2026-08-18 WP-3.12 Canvas MVP ☑**；**2026-08-18 2D/3D 能力层 Phase 1–6 ☑**（RenderGraph / TexturePool / Canvas2D 正式面 / GPU 2D 可选 / 子树 effect / scene3d camera·light / Timeline·glyph_fx）。**B16/B17 产品目录仍不挡 Beta**。  
> **对照**：[canvas-ui](https://github.com/DavidHDev/canvas-ui)（包住子树的 shader/paint）· [react-bits](https://github.com/DavidHDev/react-bits)（文字动效 / 背景 / 动画 UI）；**禁止**搬 HTML-in-canvas / Chrome flag / DOM 纹理 / CSS-in-JS 运行时。

交叉：[`COMPONENT_LIBRARY.md`](COMPONENT_LIBRARY.md) §2 · [`component_inventory.md`](components/component_inventory.md) · [`damage.md`](engine/damage.md) · [`box_model.md`](engine/box_model.md) · Chart → WP-3.2

---

## 0. 一句话结论

RUI 已是 **wgpu 自绘 + 保留 Element 树**。缺的不是「再嵌一个浏览器 Canvas」，而是把 **公开自绘面** 做成与 `ui!` / Builder 同级的叶节点：

| 面 | 公开 API（目标） | 画什么 | 谁用 |
|----|------------------|--------|------|
| **Svg** | `<svg>` / `svg()` + 子标签 `path`/`circle`/`rect`/… | 矢量场景图（描边/填充/变换） | 图标、Chart、示意图 |
| **Canvas** | `<canvas>` / `canvas()` + **自定义 paint** | 像素/立即 2D 指令，或离屏纹理回写 | 图表算法、粒子、用户自绘 |
| **2D 场景** | Canvas 2D context **或** retained 2D 层（与 Svg 可共享 path 核） | 变换、层、合成 | 游戏 HUD / 创意效果底座 |
| **3D** | `<scene3d>` / `scene3d()`（实验） | mesh / camera / 简单光照，**仍走 wgpu** | 产品演示、B16 3D 件、B17 倾斜/背景 |

全部是 **taffy 叶 + `LayoutStatus` + damage**；不另开 Skia / Vello 主线（ADR-3）。

---

## 1. 现状（代码事实 · 2026-08-13）

| 能力 | 现状 | 缺口 |
|------|------|------|
| 矩形 / 圆角 / 阴影 / 渐变 SDF | wgpu 主路径 | — |
| `primitives/path` | `PathCommand` + `rui_path_d`（HTML/SVG `d`：M/L/H/V/C/S/Q/T/A/Z，曲线展平为折线）+ **CPU 覆盖率光栅 → RGBA 贴图**（tessellate 四边形仅回退）；`stroke-linecap` / `stroke-linejoin` | GPU 三角网（仍非主路径） |
| 用户 `<svg>` 场景图 | **WP-3.11 ☑** typed 形状（CPU path）+ `SvgNode::Elem` 透传（resvg）；默认色 / `currentColor` = `foreground`；默认 `pointer-events: bounding-box`（[CSS SVG-only](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/pointer-events)） | 类 Web DOM（script / foreignObject / CSS 选子 / 任意外链 image） |
| Lucide Animated | **配方 JSON + SVG 帧烘焙**（[`pqoqubbw/icons`](https://github.com/pqoqubbw/icons)）：`<AnimatedIcon>` / `animated_icon()`；默认色 `text-foreground`（经 `text_color` attr，防 cascade 盖成白）；默认 `bounding-box`；支持 `g` 嵌套 / `custom` 函数 variants / 单值 keyframe 插值 | React/Motion 运行时 |
| Chart | **grammar**：`define_chart` + cartesian / **polar 族** / `custom_mark` + `ChartTheme`；**compact scales**（linear / band / point / **log** / **time**）；tooltip · crosshair；预设薄封装；**同一 path 核** | JS D3 运行时（禁）；组合图可画在 Svg 叶上 |
| emoji SVG | `emoji-svg` feature：`resvg`/`usvg` **光栅成贴图** | **不是**公开 `<svg>` 场景图 |
| 用户 `<canvas>` / paint 回调 | **WP-3.12 ☑** `primitives/canvas`：正式 `Canvas2D`（曲线/图像/状态/离屏/命中）；CPU fallback + `RUI_GPU_2D=1` GPU 主路径 | 完整 WHATWG Canvas2D |
| 3D mesh / camera | **WP-3.13 ☑** `<scene3d>` 实验 feature；`Scene3dCamera`/`Light`/`Material` builder；透视立方 | glTF 全家桶 |
| 子树特效 wrap | **B16 底座 ☑** `effect_wrap()` + `EffectPassGpu`（capture → shader → `rui_fx:`） | 创意组件目录件 |
| 字形动效 | **B17 底座 ☑** `Timeline` + `text().glyph_fx()` + `GlyphFx` 前处理 | 165 件动效目录 |

---

## 2. 分层（实现 = 规范）

```text
  ui!  <svg> / <canvas> / <scene3d>     Builder: svg() / canvas() / scene3d()
                    │
                    ▼
  Element 叶（template_id · layout_bounds · damage）
                    │
     ┌──────────────┼──────────────┐
     ▼              ▼              ▼
  SvgScene       Canvas2D       Scene3d（实验）
  (矢量树)       (paint 回调     (wgpu pass /
                  或 2D context)   mesh 缓冲)
                    │
                    ▼
              RuiPipeline / wgpu
```

| 规则 | 含义 |
|------|------|
| **叶节点** | Svg/Canvas/3D 默认 `display` 块或按 props；子 SVG 形状**不**各占 taffy 节点（场景图在叶内） |
| **Layout ≠ Paint** | 改 path 数据 / paint → 优先 `StyleChanged`；改 `width`/`height`/`viewBox` → `LayoutChanged` |
| **Hit-test** | 默认用叶 `layout_bounds`；`pointer-events` 分通用 / SVG-only，path 场景上 fill/stroke 几何命中（见 §3）；Canvas 默认可点整叶 |
| **双书写** | 标签与 Builder 必须同语义（ADR-9）；稳定面进 prelude 前须双入口 |
| **响应式** | 属性走 `bind_*`；结构（增删 `<path>`）走结构岛 / 子 scene reconcile；禁止根裸 `get()` 整树重建画布 |

---

## 3. Svg（WP-3.11）

**目标心智**：接近 HTML `<svg viewBox>`，但是 **RUI Element 叶内的声明式矢量场景图 + resvg 光栅**，不是浏览器 SVG 运行时 / DOM。

| 项 | 口径 |
|----|------|
| 根 | `<svg viewBox="0 0 100 100" width="…" height="…">` / `svg().view_box(…).w(…).h(…)` |
| 子 | 见下方**支持矩阵**；Builder：`svg_el("…")` / 命名包装；`ui!` 标签表对齐 MDN（无 `script` / `foreignObject`） |
| 绘制 | 描边 / 填充 / opacity / transform；`stroke-width` / `stroke-linecap` / `stroke-linejoin` / `fill-rule`；`fill="url(#id)"` 渐变：resvg 全量 + CPU path 回退采样；长尾属性 `.attr` 原样 emit；根默认 `color`=主题 `foreground`（Tailwind `text-foreground` / `currentColor`） |
| 命中 | 对齐 [CSS `pointer-events`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/pointer-events)：**通用** `auto` / `none`；**布局扩展** `box-none`；**SVG 专用** `bounding-box`（叶默认）/ `visiblePainted` / `visibleFill` / `visibleStroke` / `visible` / `painted` / `fill` / `stroke` / `all` — 有 path 场景时按 fill/stroke **路径级**命中；`auto` 在 SVG 叶上 ≡ `visiblePainted` |
| 宏 | `ui!`：kebab（`stroke-width`）与 snake/camel 同义；未知 SVG 属性 → `.attr`；`<text>` 等字符串子 → `.text_content` |
| 实现倾向 | **`svg` feature**：Lucide + 公开 `<svg>` **统一 resvg/usvg 光栅**；SMIL bake（`animate` / `set` / `animateTransform`）+ 持续 `request_redraw`；`animateMotion` 等 Elem 透传；无 feature / Chart 动态数据走 `PathCommand` 核（含 `url(#)` 渐变） |
| 与 Chart | WP-3.2 grammar（`define_chart` + marks）与预设 **应消费同一 path 核**，禁止第二套折线格式 |

### 支持矩阵（对照 [MDN SVG 元素](https://developer.mozilla.org/en-US/docs/Web/SVG/Reference/Element)）

| 分类 | typed + CPU path | Elem + resvg 透传 | 不做 |
|------|------------------|-------------------|------|
| 基础形状 | `circle` / `ellipse` / `path` / `rect` / `line` / `polyline` / `polygon` / `g` | — | — |
| 结构 / 引用 | `defs` / `use` / `symbol` / `clipPath` | `a` / `switch` / `view` / `title` / `desc` / `metadata` | `script` / `foreignObject` |
| 渐变 | `linearGradient` / `radialGradient` / `stop` | （同左亦可走 Elem） | — |
| 文本 | — | `text` / `tspan` / `textPath` | — |
| 图像 / 图案 / 标记 / 遮罩 | — | `image`（贴图键 / data URI / 本地约定；**产品** `<Image src="https://…">` 见 feature `remote-asset` + [`RemoteAssetFetcher`](../ops/remote_asset.md)）/ `pattern` / `marker` / `mask` | SVG 场景内外链 `href="https://…"` 自动拉取 |
| 滤镜 | — | `filter` / `fe*` | 自研 CPU filter 光栅 |
| SMIL | bake：`animate` / `set` / `animateTransform` | 透传：`animateMotion` / `mpath`（本轮不 bake） | 完整时间线语义 / 脚本改树 |
| 呈现属性 | 高频 typed helpers | 长尾 `.attr`（`vector-effect` / `paint-order` / `marker-*` 等） | 浏览器 CSS 选中 SVG 子元素 / 样式表驱动 SVG |

**禁区（类 Web DOM）**：`<script>`、事件、SVG DOM API、脚本改树；CSS 选 SVG 子；宿主 HTML（`foreignObject`）；**SVG 场景图**内外链 image 自动拉取。产品面远程位图走可选 feature [`remote-asset`](ops/remote_asset.md)，不经 SVG `href`。

**验收（MVP）**：Gallery「绘制」：圆 + path 描边/填充；`ui!` 与 Builder 各一例；离屏或 layout 测证明叶尺寸正确。  
**落地（2026-08-18）**：`primitives/svg`；子形状 flatten 进叶；`rui_path_fit_leaf` 让 viewBox 跟叶；与 Chart 共用 `PathCommand` / `register_path_scene`。  
**加深（2026-08-27）**：`d` 曲线展平；ellipse/polygon；HTML kebab 宏；Lucide SVG path 表驱动 Icon spin；**defs/use/clipPath/linearGradient + SMIL animateTransform/animate/set**；公开 `<svg>` 统一 resvg；CPU path 回退解析 `fill="url(#…)"`。  
**加深（Elem 透传）**：`SvgNode::Elem` + `svg_el` / `text_content` / `.attr`；MDN 文本/滤镜/遮罩等经 emit → resvg；宏未知属性转发。

---

## 4. Canvas（WP-3.12）

**目标心智**：接近 HTML `<canvas>` 的 **「这块像素我自己画」**，但回调跑在 **UI 线程 + wgpu/CPU 后端**，不是 `CanvasRenderingContext2D` 百分百 API。

| 项 | 口径 |
|----|------|
| 根 | `<canvas width height>` / `canvas().w(…).h(…).paint(…)` |
| **自定义绘制** | `Fn(&mut dyn Canvas2D, Size) -> LayoutStatus`（或等价 trait）；每帧或 dirty 时调用 |
| 2D API（正式） | Path：begin_path / move_to / line_to / bezier / quadratic / arc / ellipse；状态：line_cap / line_join / global_alpha；图像：draw_image；像素：get/put_image_data；命中：is_point_in_path。CPU raster = fallback / golden；`RUI_GPU_2D=1` 走 tessellate |
| 缓冲 | 默认离屏纹理或 CPU pixmap → 上传；damage 仅标该叶 |
| 与 Svg | Canvas 可 **调用同一 `PathCommand` 核**；不要两套 path 代数 |
| 不做 | 抄齐 WHATWG Canvas2D（阴影滤镜/globalComposite 全家）；WebGLRenderingContext 别名（3D 走 §5） |

**验收（MVP）**：Gallery：指针拖线或时钟自绘；paint 内 `peek` Signal 不订根 effect；resize 走 `LayoutChanged`。  
**落地（2026-08-18）**：`primitives/canvas`；`<canvas>` / `canvas().paint`；`Canvas2D`（clear / fill_rect / stroke_rect / fill_path / stroke_path / 曲线 / clip / save·restore / draw_image / get·put_image_data / is_point_in_path / `request_frame`）；CPU pixmap → `rui_canvas:`；paint 包在 `untrack` 内。**交互**：path 核按段描边；静态叶 dirty 缓存（滚动不重光栅）；指针按住才标 dirty。

---

## 5. 2D / 3D（WP-3.13）

| 层 | 含义 | 与上两节关系 |
|----|------|----------------|
| **2D 能力** | 变换栈、层合成、可选离屏；Svg 场景 + Canvas paint **共用** | 3.11 + 3.12 的「能力层」文档化；不是第三套公开标签 |
| **3D 能力** | `<scene3d>`：camera + 简单 mesh/GLB 或程序几何；**同一 wgpu device** | 实验 feature；默认 feature **关闭**（体积 / ADR-3 GL 优先） |
| **矩阵变换** | 控件 `rotate`/`scale` 已是 2D MVP；完整 3D 矩阵 / perspective 归本包（原 B6 并入） | 不要为 3D 换渲染器 |

**禁区**：以 three.js / HTML-in-canvas 为运行时；默认 feature 塞进 glTF 解析与大 atlas。

**验收（2D）**：变换 + clip 有测或 Gallery。**验收（3D MVP）**：一旋转立方或 GLB；无 feature 时编译过、文档标明 opt-in。  
**落地（2026-08-18）**：

- **2D 能力层**：Canvas 变换栈 / clip AABB / save·restore；Svg 场景 Affine；控件 `rotate`/`scale`。**不是**第三套标签。`RenderGraph` + `TexturePool` 统一 path/canvas/image/scene3d/capture/fx。B16 wrap 复用离屏贴图 + 本层 clip。
- **GPU 2D（可选）**：`RUI_GPU_2D=1` 把 Chart/Svg/Canvas path 叶切到 tessellate 四边形；默认仍 CPU 覆盖率光栅（golden / headless 回退）。
- **3D**：feature `scene3d`：`<scene3d>` / `scene3d()` 透视旋转立方 → `rui_scene3d:` 贴图；`camera` / `light` / `material` builder；默认构建无此 feature 仍绿；Gallery：`cargo run -p gallery --features scene3d`。
- **B16 底座**：`effect_wrap("blur"|"glass"|"liquid"|"vhs"|…)` 捕获子树几何到离屏 → shader pass → 合成；**命中仍走 Element**。产品目录仍 experimental backlog。
- **B17 底座**：`Timeline`（duration / easing / repeat）接入 `tick_animations`；`text().glyph_fx("split"|"blur"|…)` 在 glyphon prepare 前处理簇级 opacity/offset。不拆每字 DOM span。
- **变换文字（Bake / SDF / Vector）**：
  - **Coverage**：轴对齐小字 — glyphon + Nearest（默认）。
  - **Bake**：`rotate` / `transition-transform` / spin 等 sticky 叶子 — 超采样 RGBA（`rui_text_xform_v2:`）+ Linear/mip + 与壳同 `world_linear`。
  - **Msdf**：缩放越出约 0.5–2 或 3D label — 覆盖率→SDF（`rui_text_msdf_v1:`）+ FS `median`/`pxRange`；`scene3d().label` 经 mat4 投影 billboard。
  - **Vector / Vello**：outline→`PathCommand`→`RUI_GPU_2D` 仅 spike（`utils/text_vector_spike.rs`）；Vello 对照不替换 glyphon。

对照后续：[canvas-ui](https://github.com/DavidHDev/canvas-ui)（B16 包住子树）· [react-bits](https://github.com/DavidHDev/react-bits)（B17 文字/背景/动画 UI）依赖本层，本包不搬 HTML-in-canvas / CSS-in-JS。

---

## 6. 未来创意组件库（B16 / B17）

两条**产品目录**，不是第二套框架。默认交付面仍是 shadcn（ADR-6）；创意库先 `experimental`，**不挡** W3 / Beta。分发倾向：**库内 crate 或可选独立 crate**，不把「复制进用户仓」当默认 DX。

| 轨 | Backlog | 对照 | 心智 | 依赖 |
|----|---------|------|------|------|
| **A · 包住子树的特效** | **B16** | [canvas-ui](https://github.com/DavidHDev/canvas-ui) | Liquid / Glass / 粒子 / VHS… **wrap children** | WP-3.11–3.13 |
| **B · 动效目录** | **B17** | [react-bits](https://github.com/DavidHDev/react-bits) | 文字动效 · 背景 · 动画 UI（≈165 件密度标尺） | 动画原语 + glyphon；背景/3D 跟 3.12/3.13 |

两轨可共享：离屏纹理、path 核、`peek` 不订根、命中仍走 Element。许可学 MIT 心智，**不搬** Commons Clause。

### 6.1 B16 · canvas-ui 式（包住子树）

| 学 | 不搬 |
|----|------|
| 「效果包住整页/子树」；Gallery + playground；typed props；MCP/registry 可选 | **HTML-in-canvas**、Chrome `canvas-draw-element`、DOM 当纹理、WebGL overlay fallback |
| 组件自包含 | 把默认组件面改成「只卖特效」 |

实现路径（依赖 3.11–3.13；**2026-08-18 底座已落地，产品目录仍 backlog**）：

1. 子树 → 离屏 wgpu 纹理（`effect_wrap` capture pass；damage 可复用）。  
2. 全屏或叶上 shader / Canvas paint / 3D pass 扭曲该纹理（`rui_fx:`）。  
3. 命中与 a11y **仍走底下 Element 树**（特效不得吞掉 Button/Input）。  
4. 产品面：`components/canvas_fx/` 或独立 `rui_canvas_ui` crate（**不挡** W3 / Beta）。

### 6.2 B17 · react-bits 式（动效目录）

对照 [DavidHDev/react-bits](https://github.com/DavidHDev/react-bits)（及 vue-bits / svelte-bits 端口）：**文字动效 / Animations / 创意 Components / Backgrounds**；shadcn/jsrepo 式安装。RUI 只取**密度与分类**，运行时仍是 Element + Signal + wgpu。

| react-bits 桶 | RUI 落点 | 不做 |
|---------------|----------|------|
| **Text Animations**（BlurText、SplitText、CountUp…） | glyphon **字形/簇**级 opacity·transform；复杂滤镜可走 Canvas | 每字一个 DOM `<span>`；GSAP / Framer Motion |
| **Animations**（hover 倾斜、入场、弹簧） | 现有 `rotate`/`scale` + 未来弹簧/时间线原语；3D 倾斜 → 3.13 | CSS `@keyframes` 运行时；每帧重建整棵 UI |
| **Components**（创意 chrome，非表单控件） | `#[component]` 组合原语；Gallery「绘制 / 动效」实验区 | 替代 Button/Input/Dialog 稳定面；抄 4 套 JS/TS×CSS/TW 变体 |
| **Backgrounds**（粒子、极光、网格…） | 全幅 Canvas/shader **装饰叶**，内容层仍可点 | 背景吞掉命中；HTML-in-canvas |

| 学 | 不搬 |
|----|------|
| 四分类目录 + playground；typed props；可定制、可 tree-shake | React / CSS-in-JS / Tailwind 变体矩阵；复制粘贴进用户仓为默认 DX |
| 背景与文字动效**可独立于**包住子树的特效（B16） | Background Studio / Shape Magic / Texture Lab 网页工具链 |

实现倾向（**2026-08-18 底座已落地，目录仍 backlog**）：

1. **先**通用动画原语（`Timeline` duration / easing / 弹簧；`peek` 驱动 chrome，禁止根 effect 风暴）。☑  
2. 文字件吃 glyphon 测量（测宽≡绘制仍成立；`glyph_fx` 前处理，不得破坏 WP-2.1）。☑ 底座  
3. 背景件 = 装饰 Canvas 叶 + 上层普通布局；与 B16 wrap 可组合（背景在下、Liquid 包内容）。  
4. 产品面：`components/bits/` 或独立 `rui_bits` crate；先 `experimental`。**不挡** W3 / Beta。

---

## 7. 与 WP-3 其它包的边界

| 包 | 管 | 不管 |
|----|----|------|
| **WP-3.2** Path → Chart | grammar（`define_chart` + cartesian/polar marks + log/time scale + tooltip/crosshair）+ 预设；path 核进批 | 公开 `<svg>` 语法糖（**归 3.11**）；Canvas paint（**归 3.12**）；通用运动原语（**归 B17**）；JS `d3-scale` 运行时（**禁区**） |
| **WP-3.11** Svg | 标签 + 场景图 + 与 path 核合一 | Chart 产品 API |
| **WP-3.12** Canvas | 叶 + paint / 2D context MVP | 3D API |
| **WP-3.13** 2D/3D 能力 | 2D 合成纪律；3D feature；矩阵加深 | 创意组件目录（B16 / B17） |
| **B16** | Liquid/Glass/… **wrap** 产品件 | 换核；文字 SplitText 目录（归 B17） |
| **B17** | 文字动效 / 背景 / 动画 UI 目录 | 换核；包住子树的 shader（归 B16） |

W3 **退出不要求** 3.13 3D 产品化 / B16 / B17 目录；3.11/3.12 MVP **建议**在 W3 内开工，不挡 Win-first 0.1。能力层（graph / canvas 正式面 / GPU 2D 可选 / effect wrap / timeline）可并行，**不升** SemVer 稳定面、**不挡** Beta。

---

## 8. Chart / 自绘：禁区 · 可加深 · 别的包

不要把「Chart 还没做完」写成永远不做。分层如下（对照 TanStack Charts 心智，**不搬 JS 运行时**）：

```text
禁区     DOM · script/foreignObject · 任意外链 image · JS D3 运行时 · 换渲染器（Skia/Vello）
可加深   polar 族 · 时间轴 · Rust 专用 scale（log / time / 将来 pow…）· Chart tooltip / crosshair
别的包   公开 <svg>（WP-3.11）· Canvas paint（WP-3.12）· 通用运动原语（B17）
```

| 层 | 含义 | Chart 怎么接 |
|----|------|----------------|
| **禁区** | 浏览器 SVG DOM / HTML-in-canvas；`script`/`foreignObject`；任意外链 image；`npm i d3-scale` 当核；第二渲染器主线 | 场景仍是 `PathCommand` → path 核 → wgpu |
| **可加深（本包 · WP-3.2）** | `radial_bar` / `radial_line` / `radial_area` / `radial_dot` + polar guides；`scale_time` / `scale_log`（同一 `ResolvedScale.map` 边界，对位 TanStack compact→专用，**实现是 Rust**）；`.tooltip(true)` / `.crosshair(true)`（钉在命中点，不跟 GSAP/Framer 运动核） | 继续叠 mark，不新开图表类型组件 |
| **别的包** | 公开 `<svg>` 叶、Canvas paint、duration/easing/弹簧 | Chart **应消费同一 path 核**；3.11 落地后组合图可画在 Svg 叶上，而不是只出 RGBA 贴图 |

**明确仍不做（全库，不只 Chart）**：

- 第二渲染器主线（Skia / Vello / FemtoVG）替代 wgpu  
- 浏览器 `<canvas>` / SVG DOM / HTML-in-canvas  
- 类 Web DOM 的 SVG（script / foreignObject / CSS 选子）或完整 WHATWG Canvas2D  
- 默认 feature 捆绑大型 3D 运行时  
- 用 Canvas 每帧清整棵 UI 树（即时模式退回 egui 禁区）  
- CSS-in-JS / Framer Motion / GSAP / 每字 DOM `<span>` 作为 B17 运行时  
- 用 B16/B17 替换 shadcn 默认产品面  

---

*规范随 WP-3.11–3.13 关闭而修订；新增公开绘制 API 须双书写并进 inventory（先 `experimental`）。创意库 B16/B17 不挡 SemVer 0.1。*
