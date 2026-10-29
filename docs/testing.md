# 测试规范

| 测试类型 | 规范位置 | 边界 |
| --- | --- | --- |
| 私有单元／模块回归 | `src/<domain>/tests/<module>.rs`（模块根也可用 `tests/mod.rs`） | 生产模块通过原有 cfg + 显式 path 引入，保持私有访问和逻辑名称 |
| 原父模块顶层回归 | `src/<domain>/tests/<module>_cases.rs` | cfg(test) 的 include 保留原父模块测试身份；不新增公共入口 |
| 包内跨模块回归 | `crates/jadix/src/crate_tests/<domain>/` | 使用既有 pub(crate)，不扩大生产 API |
| 公开 API／feature | `<crate>/tests/*.rs` | 消费者可见接口；helper 放子目录 |
| 原生热更夹具 | `crates/jadix/src/app/tests/native_acceptance/` | 真实窗口与 GPU，串行执行 |
| Python 工具契约 | `scripts/tests/test_*.py` | 从文件位置解析被测工具 |
| 应用与配置示例 | `examples/` | 示例不替代回归 |

检查、验收、发布、基准和资源脚本按用途分组，入口见[脚本目录](../scripts/README.md)。在工作区根目录运行：

```sh
python scripts/checks/check-test-layout.py --output target/governance/test-layout.json
python scripts/checks/check-public-docs.py --output target/governance/public-docs.json
python -m unittest discover -s scripts/tests -p "test_*.py"
cargo test -p jadix_core --lib --locked --offline
cargo test -p jadix --lib --features hot-reload,devtools,gallery-tests --locked --offline -- --test-threads=1
cargo test -p jadix --test css_alignment --locked --offline
cargo test -p jadix_components --test input_fidelity --locked --offline
```

`--offline` 要求依赖已缓存；首次安装先获取依赖。图标工具契约使用 Pillow，workflow 契约使用 PyYAML。
不得把 ignored、零命中或只编译当作已运行通过。GPU／原生用例按平台显式选择 `--ignored --exact`，需要可用桌面与驱动，不与其他窗口验收并发。
记录 package、target、feature、命令、退出码与日志；迁移后核对完整身份，不只比较数量。

依赖工作区 Gallery 的私有 `gallery-tests` 夹具只承诺源码工作区运行；普通消费者不启用此内部测试组合。
原始日志、性能记录和截图保留在开发证据仓，不作为公开文档或生产资源。

维护叉保留上游测试目录和发现方式，避免扩大合并差异；其来源与本地补丁一并维护。纯公开契约包无需为了目录一致而新增空的私有测试树。测试专用 helper 随用例放入所属 `tests/`，生产模块仅保留被其他测试桥实际使用的 cfg(test) hook。

GitHub 托管 runner 不可用时，先在本机重放轻量门，再运行编译与用例：

```sh
python scripts/checks/check-ci-workflow.py --output target/governance/ci-workflow.json
python -m unittest discover -s scripts/tests -p "test_*.py"
python scripts/checks/check-test-layout.py --output target/governance/test-layout.json
python scripts/checks/check-public-docs.py --output target/governance/public-docs.json
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --no-deps --locked -- -D warnings
cargo clippy -p jadix --all-targets --no-deps --features remote-asset --locked -- -D warnings
cargo clippy -p jadix --all-targets --no-deps --features image-stream --locked -- -D warnings
cargo clippy -p jadix --all-targets --no-deps --features text-undo --locked -- -D warnings
cargo clippy -p jadix --all-targets --no-deps --features remote-asset,image-stream,text-undo --locked -- -D warnings
cargo deny check licenses bans sources
cargo build --workspace --all-targets --locked
cargo test --workspace --locked
```

workflow 引用检查覆盖静态包名、显式 feature、test/example 名称与脚本文件；CI 使用 `--tracked-only`，同时确认脚本包含在 checkout 中。本地检查不替代 workflow 的平台依赖安装、额外 feature 组合和 GPU 用例；Linux、Windows、macOS 分别记录结果。对 CI 的修改应由包含修复的新提交触发验证，不能把旧提交的重跑结果作为新提交证据。
