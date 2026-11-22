# Codex ↔ Cursor 文件交接 MCP

Node.js 22+，无 npm 依赖。Codex 负责规划和审阅；Cursor 编辑器或 Agent CLI 负责执行。
两者共享工作区 `.cursor-handoff/`，不要求开放端口，不把业务代码交给额外服务。

## 安装

在项目根目录运行：

```powershell
python scripts/cursor-mcp/install.py
node --test scripts/cursor-mcp/server.test.mjs
```

安装器创建/合并项目级 `.codex/config.toml` 和 `.cursor/mcp.json`，保留其他服务并备份已有文件。
已有同名服务时拒绝覆盖。配置包含本机绝对路径，保持本地，不提交到 Git。
其他工作区可用 `--workspace <absolute-path>` 注册。
重开 Codex 和 Cursor 的工作区，使新的 MCP 工具加载。当前运行中的聊天不会凭空获得工具。

CLI 模式要求安装并登录 Cursor Agent CLI（编辑器登录不等于 CLI 登录）：

```powershell
# 官方 Windows 安装方式；macOS/Linux 见官方文档
irm 'https://cursor.com/install?win32=true' | iex
agent login
agent status
```

本机已安装原生 CLI 包，MCP 已配置为直接调用该包；首次登录可直接运行：

```powershell
& "$env:LOCALAPPDATA/cursor-agent/versions/2026.11.19-f005e9d/node.exe" "$env:LOCALAPPDATA/cursor-agent/versions/2026.11.19-f005e9d/index.js" login
```

该安装未修改 PATH，因此本机无需依赖 `agent` 命令。登录需在浏览器中由用户完成。

默认从 PATH 启动 `agent`。Windows 安装器也自动识别 `%LOCALAPPDATA%/cursor-agent/versions/` 下的原生包，
直接使用包内 `node.exe index.js`，避免通过 cmd 拼接 prompt。手动指定启动器：

```powershell
python scripts/cursor-mcp/install.py --cli-command '["C:/path/node.exe","C:/path/index.js"]'
```

此参数用于首次注册。已注册时直接修改 `.codex/config.toml` 的
`mcp_servers.cursor.env.CURSOR_MCP_COMMAND`（内容是 JSON argv 数组）。
不要把带参数的完整命令拼成一个字符串，也不要在配置中写 API key；CLI 使用自己的登录凭据。

## 编辑器模式：先启动一个执行端

在 Cursor 中打开同一个工作区，启用 `cursor_handoff` MCP，向一个 Agent 聊天发送一次：

> 你是 Codex 的执行端。调用 cursor_handoff 的 cursor_next_task 等待任务。按返回的 prompt 执行、验证，保留无关改动。完成、失败或阻塞都调用 cursor_complete 回报 task_id、status、summary、checks，然后立即再次调用 cursor_next_task 等待 Codex 审阅后的下一步。等待超时就继续等待，不能自行开展下一阶段。用户要求停止时结束。

然后在 Codex 中说：

> 使用 cursor MCP。通过 cursor_dispatch(mode="editor", prompt="具体任务", timeout_seconds=300) 下发。pending 就调用 cursor_wait 等待同一个 task_id。结果到达后审查真实 diff 并运行适当检查，再调用 cursor_review 记录 accept 或 revise 和反馈；需要继续时再下发下一步。

`cursor_dispatch` 超时返回时也附带 `bootstrap_prompt` 和 `bootstrap_url`，供尚未启动执行端时使用。
Cursor 官方 deeplink 只预填提示，需要用户确认发送，不会自动执行；持续执行依靠上述已启动的 Agent 等待循环。
保持一个编辑器执行端；不要让多个 Cursor 聊天同时领取同一任务。
Cursor 客户端若提前终止长工具调用，可将 `timeout_seconds` 缩短为 30 并继续调用。

## CLI 模式：自动启动执行

Codex 调用 `cursor_dispatch(mode="cli", prompt="具体任务", timeout_seconds=300)`。
MCP 使用参数数组启动独立 CLI（不通过 shell 拼接），隐藏 Windows 子进程窗口，记录 stdout/stderr。
任务 prompt 指示 CLI 最后原子写入完成文件；进程非正常退出或未提交结果会生成失败报告，不能当成执行成功。
CLI 按现有权限策略执行；需要额外权限/认证时会失败或回报 blocked，由 Codex 审阅并调整。

## 工具与生命周期

| 端 | 工具 | 用途 |
| --- | --- | --- |
| Codex | `cursor_dispatch` | 创建任务并等待结果文件 |
| Codex | `cursor_wait` | 超时或重连后等待同一个任务 |
| Codex | `cursor_status` | 查看当前任务、结果、审阅 |
| Codex | `cursor_review` | 实际审阅后记录 accept/revise，释放任务槽 |
| Cursor | `cursor_next_task` | 阻塞等待编辑器任务 |
| Cursor | `cursor_complete` | 原子写入完成文件 |

`pending → awaiting_review → reviewed`。一工作区同时只允许一个未审阅任务，跨 MCP 进程同样生效。
accept/revise 都允许下发新任务；revise 的具体修订要求必须放进下一条 prompt。
完成报告不是审阅结论，MCP 不会代替 Codex 判断正确性。

等待工具内部用 `fs.watch` 监听并以 1 秒读取兜底；不是模型持续调用 status。
默认等待 300 秒，单次上限 1800 秒；Codex 工具超时配置为 1860 秒。
超时只表示 pending，不取消任务、不重复启动 CLI。取消 MCP 等待也只取消监听。
CLI 可以继续写文件；若 MCP 服务被强制结束，退出错误兜底可能无法写回，需要检查 CLI 日志和进程。
重新启动后用 `cursor_status` / `cursor_wait` 恢复，不重新下发。

```text
.cursor-handoff/
  active.json                 # 当前任务 ID，原子且排他发布
  <task-id>/
    task.json                 # 任务内容，工作区，result_file
    result.json               # Cursor 提交结果；Codex 等待的文件
    review.json               # Codex 审阅及调整意见
    completion.claim          # 防止 MCP 重复提交终态
    review.claim              # 防止重复释放任务槽
    stdout.log / stderr.log   # CLI 执行日志
    process.json              # CLI 退出状态
```

也可以让 Cursor 直接写 `result_file`（先写同目录临时文件，再 rename）：

```json
{"task_id":"任务返回的 UUID","status":"completed","summary":"实际修改和遗留问题","checks":["运行了什么检查及其结果"]}
```

`status` 为 completed、failed 或 blocked，ID 必须匹配。每轮独立文件防止旧完成信号误唤醒。
共享工作区是信任边界；其他本地进程可以修改交接文件，任务 ID 不提供身份认证。
若执行端中断且无法写结果，先确认它已停止，再为原任务写 failed/blocked 报告并审阅后重新下发。

## 已验证范围

测试覆盖两轮编辑器交接、审阅门禁、超时续等、重连、并发下发、重复结果、取消、错 ID 拒绝、
实际子进程启动/写结果、启动失败和真实 stdio MCP initialize/tools/list/ping/cancel。
测试用可控 CLI 替身，不消耗 Cursor 模型调用额度；真实模型执行需要本机 CLI 登录或编辑器 Agent 启动。

官方参考：[Cursor MCP](https://prod.cursor.com/docs/mcp)、
[Cursor deeplinks](https://prod.cursor.com/docs/reference/deeplinks)、
[Cursor CLI 参数](https://prod.cursor.com/docs/cli/reference/parameters)、
[Codex MCP](https://developers.openai.com/codex/mcp)。
