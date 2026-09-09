
下面分两块说清：**字重为什么「像失效」**，以及**笔画为什么像被裁掉**。这两件事在 RUI（glyphon + cosmic-text + fontdb + Swash）和浏览器（CSS + 各自文本栈）里机制本来就不一样，所以即使用了和 ShadCN 一样的 **class 语义**（`text-sm`、`font-medium`），**像素结果仍可能对不齐**。

> **文档地位**：字重 / 裁切 / 调试的**诊断手册**（「为何与 Web 不一致」）。策略契约见 [font_strategy.md](font_strategy.md)。**已冻结**（2026-09-06；与 `font_strategy.md` 同批签署）。纠错可改；策略变更须改 `font_strategy.md`。
>
> **配置速查**（与 [font_strategy.md](font_strategy.md) 同步）：全局可用 **`RUI_FONT_FAMILY` / `RUI_FONT_SIZE` / `RUI_FONT_WEIGHT`**、**`rui::set_text_defaults`** 或 **`WindowOptions::default_font*`**。局部可用 `class` 的 `font-sans` / `text-sm` / `font-medium` / `leading-none`，或 RSX 的 `font_family` / `text_size` / `font_weight` / `leading`。可变字体走 `wght` 插值；无 Medium 面时默认 **synthetic bold**（`RUI_SYNTHETIC_BOLD=0` 可关）。Inter 字体文件由**应用**随包提供（`fonts/` / `RUI_FONT_DIR`），核心不 embed。
>
> **Gallery**：`cargo run -p gallery` → 侧栏 **文本** → **7) 字体族 / 字重 / 行高覆盖（验收）**。冻结口径见 [font_strategy.md](font_strategy.md)「文档冻结」。

---

## 1. 字重「失效」或和 Web 不一致

**浏览器里** `font-weight: 500` 会走一整套逻辑：选族、选字重轴、可变字体插值、**没有 Medium 面时还可能做伪粗（synthetic bold）**，而且这套规则随平台（DirectWrite/CoreText）和字体而不同。

**RUI 里** 现在按同一条流水线靠拢：

1. **选族** `resolve_family`（CSS 逗号列表 + `system-ui` / `ui-sans-serif` 等泛型）。
2. **可变字体** 若族内 `wght` 轴覆盖请求值，把 **请求字重原样** 交给 cosmic-text 插值（不再把 Inter Variable 的 500 snap 成 400）。
3. **静态面** 用 CSS Fonts 4 匹配：Segoe 仅有 400/600/700 时，500 匹配 **Regular 400**（不会误跳 Semibold）。
4. **Synthetic bold**：匹配面轻于请求且请求 ≥ 500 时，绘制阶段用与文字同色的细阴影加粗（CSS `font-synthesis: weight`）。Lucide 除外。

因此 `font-medium` 在仅有系统 Segoe Regular 时也会比 400 略重；装上 Inter Variable 时则是真 500 轴。关闭：`RUI_SYNTHETIC_BOLD=0` 或 `WindowOptions::synthetic_bold(false)`。

另外：

- **主族名**仍是 `best_font_family`（Inter / Segoe UI…），**中文会再 fallback**；fallback 族里若只有 Regular，**整段中文都会是同一套轮廓**，和你在 Chrome 里看到的「西文 500 + 中文另一套匹配」也不会完全一致。
- **Lucide 等图标**在代码里还会把字重 **强行改成 400**，避免 PUA 跑飞；若某段文本误用了图标族或共用样式，也会表现为「字重不对」。

所以：**不是 ShadCN 的 token 没写进 RUI，而是「字重」在两条栈里的含义和补救手段不同**，CJK 上尤其明显。

`font-serif` / `Times New Roman` 若和 `font-sans` **西文也完全一样**，通常不是 class 没生效，而是 **fontdb 里没有衬线面**：启动只 mmap 有限文件，不会按族名去问 DirectWrite。`font-serif` 会落到 cosmic `Family::Serif`，未 `set_serif_family` 时与 UI sans（Segoe）同面。汉字仍走懒加载雅黑，本来就不会随 Times 变衬线。

---

## 2. 笔画「缺斤少两」——更像裁切，而不是字体缺字

这类现象在 RUI 里通常来自 **行盒太紧 + 绘制/排版按盒硬裁**，和浏览器默认「墨水可以溢出 line-box」的习惯不同。

**（1）`leading-none` → 行高 ≈ 1em**

Label 对齐 ShadCN 用了 **`leading-none`**，在 RUI 里对应 **`line_height_scale = 1.0`**，于是 cosmic 里用于排版的 **行高 ≈ 字号**（例如 14px 对应 14px 行高）。  
但汉字、降部字母的实际 **字形框往往高于这条「行盒」**；浏览器里很多布局下墨水仍可画在盒外，**不一定被裁**；而 RUI 在 `prepare_text` 里会用 **布局得到的矩形** 去参与 `buffer.set_size` 和 glyphon 的 **TextBounds** 一类裁剪，**盒外笔画更容易被切掉**。

**（2）`buffer.set_size` 高度只有 `scaled_h + 2`**

排版时竖直方向用的是 **布局高度加一点很小的余量**（当前是 **+2 逻辑像素**）。当 **Taffy 算出来的文本块高度** 本身就被 `leading-none`、flex 子项高度等压得很扁时，这点余量 **常常盖不住 CJK 的全高或上下边**，表现就是 **顶/底笔画缺一块**。

**（3）父级 `overflow: hidden` 与滚动视口**

布局树里若父节点是 scroll/hidden，**`clip_bounds` 会和文本矩形求交**；Web 上同样 `overflow:hidden` 会裁字，但 **line-height、padding、子项拉伸** 往往和 RUI 不完全一致，所以你会感觉「同一套 UI，Web 完整、原生缺角」。

**（4）字重与「细/断笔」的混淆**

若某笔画实际是 **抗锯齿 + 与背景对比** 造成的「看起来像断线」，还可能叠 **gamma / 没有子像素定位 / 缩放非整数** 等渲染差异；但你说「笔画不完整」时，**优先怀疑竖直/水平裁切**更贴切。

---

## 3. 和 ShadCN「不一致」该怎么理解

- ShadCN/Web：**语义**（字号、字重、行高类名）+ **浏览器文本与裁剪模型**。  
- RUI：**同一套语义映射到我们自己的布局与 glyphon 裁剪模型**，中间还有 **fontdb 选面规则** 和 **cosmic 行盒**。

所以：**不是简单「没按 ShadCN 配字体」**，而是 **栈不同 → 字重与裁切行为自然不同**；要更贴近 Web，需要在 RUI 侧单独做策略（例如：为文本绘制增加 **竖直 ink 余量**、或行高不要严格 1.0、或改进字重/fallback），那是 **刻意的渲染/布局策略**，而不是再改一个 token 就能对齐。

如果你希望下一步 **在工程上收敛**（例如在不大改布局的前提下给 glyph 多留竖直余量、或行高不要严格 1.0），可以说明优先场景（Label / Input / 滚动区内）。

---

## 4. 默认行高：对齐 Tailwind CSS v4

`crates/rui/src/style/tailwind_leading.rs` 中 [`tailwind_default_leading_scale`](https://tailwindcss.com/docs/font-size) 使用 **v4 文档**里的比值，例如：

- `text-sm` → `calc(1.25 / 0.875)`（不再写死「v3 的 20px/14px」，数值等价）
- `text-5xl` … `text-9xl` → 文档为 `line-height: 1`（比值 **1.0**）
- `text-9xl` 字号为 **8rem（128px @ 16 root）**，已单独占一档

未设置 `line_height_px` / `line_height_scale` 的文本，在 `prepare_text`、`text_hit` 与 `layout_tree` 测量里都会走上述回退。

**Label** 仍通过 `apply_label_defaults` 使用 **`leading-none`（倍率 1.0）**，与 ShadCN Label 一致，**不**走 `tailwind_default_leading_scale`。`class="leading-none"` 与属性 `leading="none"` 等价。

---

## 5. 调试：`RUI_TEXT_DEBUG`（观察族名 / 字重 / shaping 结果）

在 **文本准备阶段**（`prepare_text_frame`）会向 **stderr** 打印行（每帧、仅命中过滤时），用于对比 **请求的族名与字重** 和 **cosmic shaping 后 fontdb 实际用到的字面**（多 fallback 时 `resolved=[…]` 会出现多个族名，用 ` | ` 分隔）。

**用法（PowerShell 示例）：**

```powershell
$env:RUI_TEXT_DEBUG = "1"
cargo run -p gallery
```

**过滤：**

| `RUI_TEXT_DEBUG` | 行为 |
|------------------|------|
| `1` / `all` / `yes` / `true` | 打印所有文本节点 |
| `input` | **输入框内联文本**：`template_id` 为 `__rui_input:…:text`（**含失焦**，不再要求本帧有光标） |
| 其它任意字符串 | **子串匹配** `template_id`（不区分大小写），例如 `gal-s2`、`__rui_input`、`gal-s6`、`gal-s11` |

`gallery` 中为对照方便预留了 id：`gal-font-*`（字体覆盖卡）、`gal-s2-input-a`、`gal-s6-label-cjk`、`gal-s11-virt-title`；输入框内联文本的 `template_id` 形如 `__rui_input:<field_id>:text`。过滤可用 `RUI_TEXT_DEBUG=gal-font`。

每行含：`tid`、`kind`（`input` 表示输入相关）、`caret=y|n`（本帧是否绘制 IME/插入符；失焦时为 `caret=n`）、`shape=adv|basic`（cosmic shaping）、`extra=`（`shape_suffix` 码位，Input 为 `U+2009` 薄空格）、`w`（字重）、`sz`/`lh`（逻辑像素）、`lh_src`（显式倍率或 `tw-v4≈…`）、`hit=reuse|reshape`、`req`/`attrs`、`=> […]`（**按 glyph 顺序**出现的全部字面，含拉丁/薄空格等）、`cjk=>[…]`（**仅**簇内带 CJK/全角等相关码位的 glyph 所用字面）、文本预览。

**为何 Input 的 `=>` 像「Microsoft YaHei UI | Segoe UI」，而 Section 6 Label 中文行像「Yu Gothic | DengXian」？**

1. **字重不同（主因之一）**：`Label` 默认对齐 ShadCN 为 **`font-medium` → `w=500`**（`apply_label_defaults`）；`Input` 内联文本对齐 ShadCN Input 为 **`w=400`**。fontdb/cosmic 按字重选主字体与 fallback，**400 与 500 可能落到不同的中文字面**（例如一侧偏 UI 中文族、另一侧偏文档/日文化合字库等），因此 **`cjk=>[…]`** 也可能不一致——应用该字段对比「中文实际用的族」，而不是只看 `=>` 字符串是否相同。
2. **Input 的 shaping 后缀**：`INPUT_LINE_SHAPE_SUFFIX` 为 **U+2009**（薄空格），只参与排版、不在 `Element::text` 里；shaping 缓冲末尾多了一个拉丁类码位，**`=> […]` 全量列表里常会多出 `Segoe UI`**。对比中文渲染时应以 **`cjk=>[…]`** 为准，并看 `extra=U+2009`。
3. **行高**：未设 `leading` 的 Input 使用 `tailwind_default_leading_scale`（`lh_src=tw-v4≈…`）；带 `leading="normal"` 的 Label 为倍率 **1.5**。行高主要影响 `Metrics`，族名差异仍以上述字重 / fallback 为主。

**读你终端里那段 `gal-s6-label-cjk`：**

- `req=Segoe UI` / `attrs=Segoe UI`：来自 `best_font_family`，未单独设 `font_family`。
- `=> [Yu Gothic | DengXian]`：中文等码位 **fallback** 到系统日文字体 / 等线，**不是**「族名从 Segoe 变成别的」的 bug，而是多字面混排时的正常现象。
- `lh_src=1.5000`：该 Label 在 RSX 里写了 **`leading="normal"`** 或 `class="leading-normal"`，行高倍率为 **1.5**，故 `lh≈21`（14×1.5）；若要与 ShadCN Label 的 **`leading-none`** 一致，改用 `leading="none"` 或 `class="leading-none"`。

**刷屏：** 只要整帧文本缓存失效（例如插入符闪烁相位、滚动导致 `text_nodes` 变化），就会再次进入 `prepare_text_frame`，过滤命中时会每帧打印。设 **`RUI_TEXT_DEBUG_RESHAPE_ONLY=1`** 时，仅在 **`hit=reshape`**（本段文本重新 shape）时输出，适合抓「字体何时变化」。
