# 样式、布局与对齐

Jadix 是自绘原生 UI，CSS 风格名称不表示支持全部浏览器 CSS 语法。

## 布局与文本

Flex/Grid 对齐由布局系统处理。`start` / `end` 按逻辑方向，`flex-start` / `flex-end` 按 Flex 主轴解释；反向布局时不要视为同义词。
基线、stretch 和分布对齐需匹配容器及约束。组件默认值由 Builder／Props 决定，不以全局方向改写替代局部缩进修复。

`text-align` 与 `text-align-last` 控制段落和末行，不替代容器布局。
不需要基线参与布局的普通文本避免额外 shaping；需要时仍按实际字体和盒模型计算。

## 绘制与边界

焦点环、边框和变换后前景需保留正确的裁剪、失效与命中范围，不能用整图容差掩盖布局错误。
参见 [Feature](FEATURES.md)、[组件作者接口](component-authoring.md)及[主题手册](../crates/jadix/THEMES.md)。
完整 CSS 属性研究、历史修复台账和上游快照属于开发审计资料，不作为已支持 API 的完整列表。
