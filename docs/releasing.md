# 发布流程

## 发布前

1. 完成测试与文档整理；工作区及每个待发布包均有 `README.md` 和 `README.zh.md`，用户文档不依赖开发证据仓。
2. 对当前源码重算闭包、版本、feature、target 和 DAG；修改后不能直接复用旧 `.crate`。
3. 检查实际归档的源码、README、手册、许可、本地链接及 checksum。
4. 查询目标 registry 的名称、版本、owner，确认 Cargo 发布凭据和实际权限。下载镜像不是发布 registry。
5. 运行正常测试与身份检查，防止迁移后出现零命中、测试丢失或 passed 转非 passed。

```sh
python scripts/release/check-r3-closure.py --help
python scripts/release/check-r5-registry.py --help
python scripts/release/check-r4-packages.py --help
python scripts/release/check-r4-documents.py --help
```

凭据存在性不等于已验证上传权限；名称未出现不等于已保留。不要把 token 放进仓库、日志或聊天。

## 安全登录与待发布门

Windows 发布者在本机交互运行 `./scripts/release/login-crates-io.ps1`：隐藏输入 token，经 stdin 交给 `cargo login --registry crates-io`，由 `cargo:wincred` 保存到 Windows Credential Manager。不要把 token 放进命令参数、仓库、日志或聊天。Cargo 用户配置中的 crates-io provider 应为 `cargo:wincred`；下载镜像与发布地址分开配置。配置 provider 并不代表已经登录，也不证明 token 拥有全部 48 包的发布权限。

发布前须通过 `cargo fmt --all -- --check`、公开文档与测试布局门、工具契约、当前源码归档对照及默认 10 MiB 上传体积门。已获得 registry 单包扩容许可时须另存授权证据，不能自行假设无限制。CI 的代码或测试失败应先修复；若托管 CI 因付款、额度等外部原因无法启动，可由发布负责人明确接受该限制，记录授权、实际本地替代检查及未覆盖平台后继续发布，不把未执行的 job 标为通过。预检脚本存在或外部限制例外均不能代替实际测试、上传和消费证据。

## 实际发布与验证

按当前 48 包 DAG 从叶子到门面发布至 `crates-io`。逐项记录命令、退出码和上传归档摘要，等待 index 可解析后推进依赖者。
既有版本不能覆盖；同名同版本已存在时核对内容和所有权，不自动换版本或静默跳过。
发布输入必须绑定实际提交及归档摘要；工作树存在改动时明确记录，不伪造干净提交身份。

`cargo publish --dry-run`、受控本地 registry 和 `--no-verify` 均不是实际上传或完整编译证明。

发布后在工作区外建立两个项目：框架 Builder 最小窗口、框架加组件／图标／拖拽。仅依赖真实 registry 的已发布版本，不使用根 patch 或 path/git 旁路。
记录 metadata、锁文件、构建和真实原生启动／退出，核对下载与上传 checksum，并检查用户手册及迁移说明。
这些步骤完成后才登记基线发布完成；项目仍为 Alpha，不自动升级为 Beta。
