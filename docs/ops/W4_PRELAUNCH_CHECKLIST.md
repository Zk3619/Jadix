# W4 开工前检查清单（可直接勾选）



适用范围：`AUTHORITY.md` §5.7（WP-4.1 ~ WP-4.7）开工前 1 次预检 + 开工后持续回填。  

执行原则：**先 Step 0 命名，再 P0 发布闭环**；避免无验收的大重构。



---



## A0. Step 0 — 命名决策与重命名（P0，**最先执行**，阻塞 A2/A5 及 publish）



> 预检结论：`rui` 在 crates.io 已被占用 → **必须改名**。选型已归档于 [`NAMING_DECISION.md`](../archive/NAMING_DECISION.md)：**`jadix`（琢玉）**。

- [x] 主 crate 已锁定：**`jadix`**（中文名 **琢玉**；slogan「玉不琢，不成器」/ “Gems/Jades unwrought can do nothing useful.”）。
- [x] 热重载 crate 固定为 **`jadix_hot_reload`**（**无** `_cli`）。
- [x] 四 crate 前缀一致：`jadix` / `jadix_macros` / `jadix_style_tokens` / `jadix_hot_reload`（命名层）。
- [ ] GitHub 仓库名 / 对外品牌 / README 首屏与 crate 名策略对齐（同名校或显式映射）。
- [ ] 完成 workspace 重命名（目录、`Cargo.toml`、path 依赖、`use` 路径、CI、示例）。**尚未执行。**
- [ ] CHANGELOG / 迁移说明写入「原 RUI → jadix」一段。
- [ ] 重命名后 `cargo check --workspace` + 核心测试全绿，再进入 A2。



---



## A. 阻塞发版（P0，未完成不得 `cargo publish`）



> **前置**：A0 完成。



### A1. 发布契约与版本面（WP-4.1 / 4.7）



- [ ] 明确并冻结 **SemVer 承诺面 / 实验面**（README + docs 同步）。

- [ ] `CHANGELOG.md` 建立首个对外版本条目（包含 breaking/known limits/migration notes）。

- [ ] README 状态从 Alpha 升级到 Beta 的条件写清，且与 `AUTHORITY.md` §6 一致。



### A2. 伞形 crate 发布可行性（WP-4.2 / 4.3）



- [ ] 发布模式固定为**伞形 crate**：对普通用户入口为 **`jadix`**；对外重导出宏与组件作者常用 API；高级用户可按需依赖子 crate。

- [ ] 核心发布闭环固定为**四 crate 同步**（主 crate + `_macros` + `_style_tokens` + `_hot_reload_cli`）。

- [ ] 四 crate 的 `Cargo.toml` 元数据齐全（license/repository/description/readme/keywords/categories）。

- [ ] 所有发布链路中的 path 依赖改为 crates.io 版本约束（发布前后流程明确）。

- [ ] 本地执行四 crate 的 `cargo package` 全通过。

- [ ] 四 crate 发布顺序、回滚策略、失败重试策略在 `docs/ops/PUBLISH.md` 写清。



### A2½. 主题令牌与颜色治理（WP-STY.0 / WP-STY.1，挡 WP-4.7）

> 完整计划：[`plans/global-stylesheet-loader.md`](../plans/global-stylesheet-loader.md) Phase 0。全局样式表**选择器加载器**发版后做（WP-STY.2+）。

- [ ] **WP-STY.0**：`ThemeColorPatch` + `load_theme_patch`（`.toml` / `.json` / `globals.css` `:root` 变量子集）+ `install_theme_patch`；`set_theme_mode` 保留用户 patch。
- [ ] **WP-STY.1**：库内产品色令牌化（shadow elevation、overlay scrim、`TRANSPARENT` 常量）；CI `check-token-colors` 挡裸 RGBA/hex。
- [ ] `color.md` 增「应用级主题覆盖」；Gallery 演示 `theme.toml` 覆盖 `destructive`。



### A3. 质量门与 CI（WP-4.5）



- [ ] CI 必跑并通过：`fmt` / `clippy -D warnings` / 核心测试 / golden（>=5）/ 最小示例 `check`。

- [ ] 失败即阻断 publish（分支保护或 release workflow 规则已落实）。

- [ ] release 构建参数与产物核验命令固定在文档（避免“本地口口相传”）。



### A4. 体积与内存复核（WP-4.6）



- [ ] 记录当前发布基线（包体、RSS、滚动 CPU、关键帧时延）并附命令。

- [ ] 与 Wave P 退出门数据对齐；若偏离，先处置回归再推进发布。



### A5. 文档站最小闭环（WP-4.4）



- [ ] Getting Started 提供“双写法”最小跑通示例（`ui!` + Builder）。

- [ ] 组件索引标注稳定级别（verified/experimental）并与 inventory 对齐。

- [ ] 从 crates.io README 可一跳到 Getting Started、变更日志、问题反馈入口。



---



## B. W4 同步补全（P1，强烈建议在首发前完成）



### B1. 热重载调用脚手架



- [ ] 在 README / docs 明确 **debug 开发优先路径**：先跑 UI（如 `cargo run -p gallery`），再跑热重载监听器（`cargo run -p jadix_hot_reload -- .`）。

- [ ] 给出“单机双终端”标准流程、常见端口/连接失败排查（最小故障手册）。

- [ ] 标注热重载作用边界（仅 dev/test、生效范围、非生产保证）。

- [ ] 明确热重载发布策略：`jadix_hot_reload` 为核心模块，随核心版本同步发布（策略 A）。



### B2. 框架-组件-示例同仓分目录策略



- [ ] 在文档写清：**一个仓库，多 crate / 多目录分工**（framework/components/examples/docs）。

- [ ] 对外分发策略写清：crates.io 分包发布；GitHub 单仓协同开发；两者职责边界明确。

- [ ] 给出“消费者如何选用”的指引（只依赖框架、按需启用组件、参考示例）。



### B3. 开源叙事与 README 精修



- [ ] README 增补“3 分钟上手”与“何时用/何时不用”。

- [ ] README 增补 feature matrix（默认开关、平台差异、实验特性说明）。

- [ ] README 增补贡献指南入口（Issue 模板、PR 约定、发布节奏）。

- [ ] README 增补“已知限制与路线图链接”，减少外部预期偏差。



### B4. 底层多核心 crate 拆分蓝图（架构补全）



- [ ] 在文档明确“当前形态”：`layout` / `renderer` 等底层仍在主 crate 内模块化维护。

- [ ] 在文档明确“目标形态”：演进为 `jadix_layout` / `jadix_renderer` / `jadix_core` 等可独立复用核心 crate（仍由伞形入口重导出）。

- [ ] 定义拆分准入门槛：API 边界稳定、编译收益可量化、CI 与回归矩阵可独立运行、迁移指南完备。



---



## C. 可并行但不阻塞首发（P2，进入 Beta 后继续）



- [ ] 扩展 CI 维度（多 DPI / dark mode / 真窗口矩阵）。

- [ ] 文档站补全高级主题（性能调优、架构深潜、组件作者最佳实践）。

- [ ] release 自动化（tag->publish->site deploy）与回滚演练。



---



## D. 开工当天执行建议（顺序）



1. [x] **A0 命名已锁定 `jadix`**；☐ 重命名落地（或开 PR）。

2. [ ] 用 30 分钟完成 A 区逐项红黄绿标记（阻塞=红）。  

3. [ ] 红项分配 owner 与截止时间（当天可验证）。  

4. [ ] 先打通四 crate `cargo package` 与 CI 全绿，再处理 README/叙事优化。  

5. [ ] 每完成一项，回写到 `AUTHORITY.md` §5.7 与 §6（DoD）状态。  



---



## E. 关联文档（回写位置）



- [`NAMING_DECISION.md`](../archive/NAMING_DECISION.md)：已锁定 **`jadix`（琢玉）**；历史查重归档。

- `docs/AUTHORITY.md`：W4 状态总表、Beta DoD 勾选。  

- `docs/ops/PUBLISH.md`：发布前检查、发布顺序、回滚策略。  

- `README.md`：对外入口、快速开始、热重载脚手架、状态叙事。  

- `docs/README.md`：文档导航入口。

