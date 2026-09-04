# 已知缺口 / 产品愿望（非正式 tracker）

> **已归档（2026-09-05）**。非正式 scratch，**不是**权威 backlog。巩固期 Backlog 以 [`AUTHORITY.md`](../AUTHORITY.md) §7 为准；缺陷请开 issue。  
> 下文后半混入了前端动画科普，仅作历史备忘，勿当规范。

## Components

## Style

- animation / transform 产品化加深

## Capabilities

- 跨平台文件读写深化（选择器已有 `utils/file_dialog`）
- 网络能力
- ~~透明 / 模糊窗口~~ 参考 DWMBlurGlass 优化
- tailwindcss 全量语法支持
- 并考虑：增加 popup_window 能力，为桌面端实现可超出视口的独立菜单/弹窗/通知等能力
- 平台能力特化
- 缺的主要是 steps / jump-* 以及 scroll/view `animation-timeline` 驱动（`animation` / `@keyframes` MVP 已落地：name/duration/timing/delay/iteration/direction/fill-mode/play-state；composition/range 仅存档）
- solid-dnd/sortablejs 拖拽、拖拽排序及切换父级、拖拽时的浮层、碰撞检测等 — 设计拆分见 [overlay_shells.md](../components/overlay_shells.md) §9
- orientation="vertical"  自动检测屏幕方向 auto、始终水平、始终垂直
- 浮层编排壳（设计）：[overlay_shells.md](../components/overlay_shells.md)（`anchored_overlay` / `viewport_modal` · WP-OVL）
- 无任何动画、阴影、模糊等特殊效果的性能模式
- 是否应将 lucide icon / lucide-animated 图标拆成独立 crate 发布
- sidebar 展开/收起时常发生卡顿、突变的问题
- 程序启动时首屏白屏的问题

## Bugs

不完全是。更准确地说：

> **很多前端动画确实会优先使用 CSS 的 `transition` / `animation` + `transform` / `opacity` 来实现，但并不是所有动画都基于它们。**

而且你提到的几个概念不是一类东西：

```css
transition: 过渡，控制属性变化如何动画
animation:  关键帧动画
transform:  变换属性
translate / rotate / scale: transform 的具体变换函数
```

比如：

```css
.box {
  transition: transform 0.3s ease;
}

.box:hover {
  transform: translateX(20px) rotate(10deg);
}
```

这里：

- `transition`：让变化过程有动画
- `transform`：决定元素怎么变
- `translateX` / `rotate`：具体变换效果

---

## 1. 常见前端动画实现方式

前端动画大概有几类：

| 技术 | 适合场景 | 特点 |
|---|---|---|
| CSS `transition` | hover、显示隐藏、简单状态变化 | 简单、性能好 |
| CSS `animation` / `@keyframes` | 循环动画、复杂关键帧动画 | 不依赖 JS，性能较好 |
| `transform` / `opacity` | 移动、缩放、旋转、透明度变化 | 最常做动画的属性，性能最好 |
| JavaScript + `requestAnimationFrame` | 复杂交互、物理动画、精确控制 | 灵活，但要注意性能 |
| Web Animations API | JS 控制 CSS 动画 | 介于 CSS 和 JS 动画之间 |
| Canvas / WebGL | 粒子、游戏、复杂图形、大量元素 | 性能高，但不是 DOM 动画 |
| SVG 动画 | 图标、路径描边、矢量动画 | 适合矢量图形 |
| Lottie | 设计师导出的复杂动画 | 还原 AE 动画 |
| 动画库 | 复杂交互动效 | 如 GSAP、Framer Motion、react-spring |

---

## 2. 大多数 UI 动画确实会优先用 transform / opacity

比如这些效果：

```css
transform: translateX(100px);
transform: translateY(-20px);
transform: scale(1.2);
transform: rotate(45deg);
opacity: 0;
```

它们很常见，因为浏览器对这类动画优化更好。

比如：

```css
.modal {
  opacity: 0;
  transform: translateY(20px);
  transition: opacity 0.3s ease, transform 0.3s ease;
}

.modal.open {
  opacity: 1;
  transform: translateY(0);
}
```

这是一个典型的弹窗进入动画。

---

## 3. 为什么推荐 transform / opacity？

因为浏览器渲染页面大致经过：

```text
JS / CSS
  ↓
Style 计算样式
  ↓
Layout 布局，计算位置大小
  ↓
Paint 绘制
  ↓
Composite 合成
```

如果你动画这些属性：

```css
width
height
top
left
margin
padding
```

往往会触发：

```text
Layout -> Paint -> Composite
```

开销较大。

但如果你动画：

```css
transform
opacity
```

通常只需要：

```text
Composite
```

浏览器可以把元素放到 GPU 合成层处理，所以更流畅。

例如不推荐：

```css
.box {
  left: 0;
  transition: left 0.3s;
}

.box:hover {
  left: 100px;
}
```

更推荐：

```css
.box {
  transform: translateX(0);
  transition: transform 0.3s;
}

.box:hover {
  transform: translateX(100px);
}
```

虽然视觉上差不多，但后者通常性能更好。

---

## 4. transition 和 animation 的区别

### `transition`：适合状态切换

例如 hover、focus、open、close。

```css
.button {
  transform: scale(1);
  transition: transform 0.2s ease;
}

.button:hover {
  transform: scale(1.05);
}
```

特点：

- 通常需要触发条件
- 从 A 状态到 B 状态
- 比较简单

---

### `animation`：适合关键帧动画

例如 loading、呼吸灯、轮播、复杂路径动画。

```css
@keyframes spin {
  from {
    transform: rotate(0deg);
  }

  to {
    transform: rotate(360deg);
  }
}

.loading {
  animation: spin 1s linear infinite;
}
```

特点：

- 可以自动播放
- 可以循环
- 可以定义多个关键帧
- 不需要 JS 也能运行

---

## 5. transform 和 translate / rotate 的关系

`transform` 是属性，后面可以接很多函数：

```css
transform: translateX(100px);
transform: translateY(50px);
transform: translate(100px, 50px);
transform: scale(1.5);
transform: rotate(45deg);
transform: skew(10deg);
```

也可以组合：

```css
transform: translateX(100px) rotate(45deg) scale(1.2);
```

所以不是：

```text
前端动画基于 translate / rotate
```

而是更准确地说：

```text
前端动画经常基于 transform 属性，
而 translate / rotate / scale 是 transform 常用的变换方式。
```

---

## 6. 但复杂动画经常需要 JavaScript

如果动画涉及：

- 鼠标跟随
- 拖拽
- 弹性物理效果
- 手势交互
- 根据滚动进度精确控制
- 大量元素协调
- 动态计算路径
- 复杂时间轴
- 中断、反转、续播
- 根据状态实时变化

那 CSS 就不一定够用了。

比如拖拽卡片：

```tsx
onPointerMove={event => {
  card.style.transform = `translate(${event.clientX}px, ${event.clientY}px)`;
}}
```

或者使用 `requestAnimationFrame`：

```js
let x = 0;

function animate() {
  x += 2;
  element.style.transform = `translateX(${x}px)`;

  if (x < 300) {
    requestAnimationFrame(animate);
  }
}

requestAnimationFrame(animate);
```

这种就是 JS 驱动的动画。

---

## 7. 常见动画库也不一定是纯 CSS

比如：

| 库 | 常见实现方式 |
|---|---|
| GSAP | JS 动画，功能很强 |
| Framer Motion | React 动画，声明式 |
| react-spring | 物理弹簧动画 |
| Anime.js | JS 动画 |
| Motion One | 基于 Web Animations API |
| Lottie | 播放 AE 导出动画 |
| Three.js | WebGL / 3D |
| PixiJS | Canvas / WebGL |

它们最终可能也是修改：

```css
transform
opacity
```

但驱动方式不一定来自 CSS `transition`。

---

## 8. 实际项目里怎么选？

## 简单交互动画：优先 CSS

例如：

```css
transition: transform 0.2s ease, opacity 0.2s ease;
```

适合：

- hover
- tooltip
- dropdown
- modal
- toast
- 图片放大
- 简单展开收起

---

## 复杂关键帧动画：CSS animation

例如：

```css
@keyframes fadeInUp {
  from {
    opacity: 0;
    transform: translateY(20px);
  }

  to {
    opacity: 1;
    transform: translateY(0);
  }
}
```

适合：

- 入场动画
- loading
- 呼吸灯
- 简单轮播
- 图标动效

---

## 复杂交互：JS 或动画库

适合：

- 拖拽
- 手势
- 物理弹性
- 滚动视差
- 时间轴动画
- SVG 路径动画
- 复杂页面转场

---

## 9. 一个推荐原则

前端动画性能上一般优先动画这些属性：

```css
transform
opacity
```

尽量少直接动画这些属性：

```css
width
height
top
left
right
bottom
margin
padding
```

除非必要。

比如：

```css
/* 不推荐 */
.panel {
  height: 0;
  transition: height 0.3s;
}
```

如果只是简单展开收起可以用，但高度变化会触发布局。

更高性能可以改成：

```css
.panel {
  opacity: 0;
  transform: translateY(-10px);
  transition: opacity 0.3s, transform 0.3s;
}
```

当然，展开收起如果必须真实改变布局高度，还是可能需要 `height`、`grid-template-rows`、`max-height` 等方案。

---

## 10. 总结

前端动画不是一定基于：

```css
transition / transform / translate / rotate
```

但可以说：

> **大多数常规 Web UI 动画，确实会优先使用 CSS 的 `transition` 或 `animation`，配合 `transform` 和 `opacity` 来实现。**

因为这种方式：

- 写法简单
- 性能好
- 浏览器支持好
- GPU 加速友好
- 适合大多数交互动效

但如果是复杂动画，会使用：

- JavaScript
- `requestAnimationFrame`
- Web Animations API
- Canvas
- WebGL
- SVG
- Lottie
- GSAP
- Framer Motion
- react-spring
- Three.js

所以一句话总结：

> **普通 UI 动画：优先 CSS transition/animation + transform/opacity。复杂交互动画：JS 或专业动画库。**
