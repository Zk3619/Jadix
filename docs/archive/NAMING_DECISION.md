# 命名决策（W4 Step 0 · **已归档**）

> **状态**：**已锁定**（2026-08-20）  
> **下一步**：workspace 重命名（目录 / `Cargo.toml` / path / `use` / 文档）→ 再进 WP-4.2。  
> **本文档**：选型过程归档；后续以「最终锁定」为准。下方为历史查重摘要。

---

## 最终锁定

| 项 | 决定 |
|----|------|
| **主 crate** | **`jadix`** |
| **中文名** | **琢玉** |
| **中文 slogan** | **玉不琢，不成器** |
| **英文 slogan** | **Gems/Jades unwrought can do nothing useful.** |
| **热重载 crate** | **`jadix_hot_reload`**（**无** `_cli`） |
| **四 crate 前缀** | `jadix` / `jadix_macros` / `jadix_style_tokens` / `jadix_hot_reload` |
| **造词路线** | 形西神中：Jade（玉）+ `-ix`；直接雕琢原生 = 无 VDOM |
| **否决回顾** | `rui`（crates.io 占用）· `rnui`（React Native 歧义） |

### 品牌与发布口径

- **意象**：玉不琢不成器 —— 框架像玉雕，**直接在原生渲染层雕刻**，无泥模（VDOM）、无冗余工序。
- **查重**：crates.io `jadix` ✅；根词 `jade` 已占（异域）；南非 Jadix Marketing 为异域清洁品牌，开源 GUI 冲突轻。
- **迁移**：CHANGELOG / README 须写明 `rui` → `jadix`（原内部代号 RUI）。
- **重命名**：**尚未执行**（2026-08-20 锁定名称；代码与目录仍为 `rui*`）。

### Step 0 落地清单

- [x] 锁定主 crate 名：`jadix`（琢玉）
- [x] 热重载：`jadix_hot_reload`（无 `_cli`）
- [x] 四 crate 前缀一致（命名层）
- [ ] GitHub 仓库 / org 策略
- [ ] CHANGELOG：`rui` → `jadix` 迁移说明
- [ ] workspace 重命名 + `cargo check --workspace` 全绿 → WP-4.2

---

## 命名偏好（选型时已确认）

- **短名**：主 crate 尽量 ≤8 字符
- **寓意优先**：类似 Solid / Floem——一词讲清哲学，而非缩写拼盘
- **哲学对齐**：细粒度 · 无 VDOM · Signal 驱动 · 直达底层
- **Rust 调性**：安全、高性能、硬核、可记忆
- **形西神中**：拼写国际化，意象可源自中国古典
- **避免**：与 React Native、Audulus `rui`、npm NatUI、Windows Fluent **Mica** 材质、知名 Git/WASM 工具同名硬撞

---

## 历史查重归档（摘要 · 只读）

> 多轮候选与冲突表压缩存档；**不再作为选型菜单**。

### 1. 已否决 / 不可裸用（硬冲突）

| 名 | 原因 |
|----|------|
| `rui` | crates.io 被 [Audulus/rui](https://crates.io/crates/rui) 占用 |
| `rnui` | React Native（`rn`）歧义 |
| Crisp / Grit / Pith / Plumb / Mica / Axon / Chisel / Pinion | 主名 crates.io 已占或品牌硬撞（Grit↔GitButler；Mica↔Windows Fluent） |
| `rend` / `zync` / `zynk` / `kore` / `dirx` / `tether` | crates.io 已占；`tether` 另有 tether-egui |
| `runix` / `irix` / `ygg` / `yggr` / `vectis` | crates.io 已占；`runix`=flox/Nix；`irix`=SGI IRIX |
| `guix` / `maix` | GNU Guix；Sipeed Maix 生态 |
| `zenix` | crates.io 已占 + GPUI 桌面项目（同属 Rust GUI 圈） |
| 任意 `ripple*` | [Ripple-TS](https://www.ripple-ts.com/) 细粒度 UI 框架（同赛道） |

### 2. 曾进入短名单但未采用（crates.io 可用）

| 名 | 路线 | 备注 |
|----|------|------|
| `pulson` | 造词 · 脉冲粒子 | Signal 隐喻强、冲突极少 |
| `fengix` / `zhuox` / `luox` | 中国神话造词 | 烽火 / 斫 / 络 |
| `aegix` / `mycix` / `clothix` | 西方神话 / 菌丝 | 神盾 / 依赖图 / 命运之线 |
| `serix` / `meridix` / `vermix` / `zenik` | 形西神中 | 丝绸 / 经络 / 朱砂 / 禅（避 zenix） |
| `filament` / `pinionui` / `crispui` | 整词或变体 | 早期推荐安全牌 |
| `rendix` / `quartix` / `signalix` | 实用性造词 | 异域或品牌噪声 |

### 3. 形西神中 · 玉系变体（选定上下文）

| 拼写 | crates.io | 说明 |
|------|-----------|------|
| **`jadix`** | ✅ **已选定** | Jade + `-ix`；主品牌 |
| `jadik` / `jadic` | ✅ | `-ik`/`-ic` 备用；未采用 |
| `jade` | ❌ | 根词已占，勿裸用 |

### 4. 后缀取舍（归档说明）

- **`-ix`**：开源工具圈熟悉（Unix / Nix / Matrix 感）→ **最终采用**。
- **`-ik`**：更锐、更造词；同根可用作备用。
- **`-ic`**：crates.io 常可用，但听感偏英语形容词，品牌辨识弱于 `-ix`/`-ik`。

---

## 关联文档

- [`W4_PRELAUNCH_CHECKLIST.md`](W4_PRELAUNCH_CHECKLIST.md) §A0
- [`PUBLISH.md`](PUBLISH.md) §0
- [`AUTHORITY.md`](AUTHORITY.md) §5.7 / §6
