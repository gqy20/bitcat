# 远程 Agent 监督计划

> 日期：2026-05-17 | 状态：规划中 | 范围：Mac/Linux 远程设备上的 Claude Code + Codex，通过 BitCat（Windows）统一监督

## 背景

Phase 1 已落地本机 Claude Code + Codex 的 hook 监督链路：PowerShell sender → TCP :5342 → `agent_monitor.rs` → 状态归一化 → Nudge 提醒 → Agent Watch 浮窗。

用户有多台设备（Mac + Linux），上面也在跑 Claude Code / Codex。希望这些远程会话也能统一汇总到 Windows 端的 BitCat 里看管——不需要盯着每台机器，桌宠替你看着，有事叫你。

## 目标

- 在 Mac/Linux 上**一条命令**完成安装（hook 配置 + sender 脚本 + 可选隧道）
- 远程 Claude Code / Codex 的状态实时显示在 Windows 端 Agent Watch
- 远程会话和本地会话**同一套 UI、同一套 Nudge 策略**，仅多一个设备标识列
- 用户能区分哪个会话来自哪台机器

## 不做

- 不做远程权限审批（Phase 3 范畴）
- 不做远程截图回传
- 不做 Cursor / OpenClaw / 其他 IDE 插件
- 不引入 WebSocket / HTTP server（复用现有 TCP 协议）
- 不改核心状态模型（`AgentStatus` / `AgentNudgePolicy` 不变）

---

## 架构

### 数据流

```
┌──────────────────────┐         SSH Reverse Tunnel (或直连)        ┌─────────────────────┐
│  Mac / Linux 远程    │                                            │  Windows (BitCat) │
│                      │   ───────────────────────────────────→    │                     │
│  Claude Code Hook    │   localhost:5343 → 隧道 → Win:5342       │  TCP :5342 监听      │
│  ↓                   │   或直连 Win:5342 (局域网)               │  ↓                  │
│  sender.sh           │   JSON: {source, machine, payload}       │  agent_monitor.rs   │
│  (从 stdin 读 hook)   │                                            │  ↓                  │
│                      │                                            │  parse_agent_hook_   │
│  Codex Hook          │                                            │  payload()          │
│  ↓                   │                                            │  ↓                  │
│  sender.sh           │                                            │  AgentSessionEvent  │
│  (同上, source=codex) │                                            │  (含 machine 字段)   │
└──────────────────────┘                                            └─────────────────────┘
```

### 关键设计决策

| 决策 | 选择 | 原因 |
|------|------|------|
| 传输协议 | 复用现有 TCP JSON | 零新增依赖；envelope 格式已支持 source 区分 |
| 远程连接方式 | SSH 反向隧道（首选）+ 局域网直连（备选） | SSH 自带加密/重连；autossh 保活；无需开新端口 |
| 安装方式 | curl \| bash 一键脚本 | 参考 Homebrew/nvm 模式；用户只需复制粘贴一条命令 |
| 设备标识 | `machine` 字段（hostname） | `AgentSession.machine` 已预留未使用；补通传递链即可 |
| Sender 语言 | POSIX sh | Mac/Linux 通用；不依赖 Python/Node/.NET |

### 与现有代码的关系

```
                    已有（不改）              本计划改动
                    ────────                ───────────
ClaudeHookEvent  ──→ into_session_event_from(source)  ← 加 machine 参数
AgentSession     ──→ .machine: Option<String>          ← 已有，补传递源
AgentSessionEvent→  (无 machine 字段)                  ← 新增字段
handle_hook_payload → envelope 解析                    ← 提取 machine
agent_watch.js   ──→ 卡片渲染                           ← 加 device badge
settings.html/js ──→ 本机 hook 安装                    ← 加远程安装入口
claude_hooks.rs   ──→ PowerShell 写入 + settings.json   ← 不动（纯 Windows）
codex_hooks.rs    ──→ 同上                               ← 不动（纯 Windows）
```

---

## 实施步骤

### Step 1：补齐 machine 字段传递链（Rust，~30 行）

**目标**：让远程发送的 `machine` 标识能从 TCP payload 一路传到前端 `AgentSessionView`。

#### 1.1 `core/src/agent_session.rs`

`AgentSessionEvent` 新增 `machine` 字段：

```rust
pub struct AgentSessionEvent {
    // ... 现有字段 ...
    pub machine: Option<String>,  // 新增
}
```

`into_session()` 传递 machine 到 `AgentSession`：

```rust
impl AgentSessionEvent {
    pub fn into_session(self) -> AgentSession {
        AgentSession {
            // ... 现有字段 ...
            machine: self.machine,  // 新增
        }
    }
}
```

更新测试：构造 event 时加 machine 断言。

#### 1.2 `core/src/claude_code.rs`

`into_session_event_from()` 接受可选 machine 参数并传入 event：

```rust
pub fn into_session_event_from(self, source: AgentSource, now_ms: u64, machine: Option<String>) -> Result<AgentSessionEvent, String> {
    Ok(AgentSessionEvent {
        // ... 现有字段 ...
        machine,  // 新增
    })
}
```

#### 1.3 `app/src/agent_monitor.rs`

`parse_agent_hook_payload()` 从 envelope 提取 machine：

```rust
fn parse_agent_hook_payload(raw: &str, now_ms: u64) -> Result<AgentSessionEvent, String> {
    if let Ok(envelope) = serde_json::from_str::<AgentHookEnvelope>(raw) {
        let machine = envelope.machine;  // 从 envelope 取出
        // ... 传给 into_session_event_from ...
    }
    // 直连格式无 envelope 时 machine = None（本机会话）
}
```

`AgentHookEnvelope` 加 `machine` 字段：

```rust
struct AgentHookEnvelope {
    source: String,
    machine: Option<String>,  // 新增
    payload: Value,
}
```

### Step 2：写远程安装脚本（Shell，~180 行）

**文件位置**：项目根目录 `scripts/remote-install.sh`（发布时随 portable zip 分发，或提供 raw URL）

#### 2.1 脚本流程

```
1. 参数解析
   --host <ip>       必填，Windows BitCat 所在 IP
   --port <port>     可选，默认 5342
   --tunnel [user@win] 可选，自动建 SSH 反向隧道
   --source claude_code|codex|all 可选，默认 all
   --uninstall       卸载模式

2. 环境检测
   - OS 类型 (macOS / Linux / 其他→报错)
   - ~/.claude 目录存在？→ 装 Claude Code hooks
   - ~/.codex 目录存在？→ 装 Codex hooks
   - nc (netcat) 是否可用？
   - jq 是否可用？（无 jq 则用 python/json 内建方案降级）
   - 到 --host:--port 的网络连通性（timeout 2s 测试）

3. 生成 sender.sh → ~/.bitcat/hooks/sender.sh
   - 注入 BITCAT_HOST, BITCAT_PORT, BITCAT_MACHINE, BITCAT_SOURCE
   - chmod +x

4. 安装 Claude Code hooks（如果检测到）
   - 读 ~/.claude/settings.json
   - 合并 BitCat 条目（逻辑镜像 claude_hooks.rs 的 ensure_bitcat_hooks）
   - command 用 "bash $SENDER_PATH"
   - 标记 bitcat_marker = "bitcat-remote-watch"
   - 备份原文件 → atomic write

5. 安装 Codex hooks（如果检测到）
   - 读 ~/.codex/config.toml
   - 合并条目（逻辑镜像 codex_hooks.rs）
   - command 用 "bash $SENDER_PATH"
   - commandLinux 字段（Codex 原生支持）
   - 备份 → atomic write

6. 可选：建立 autossh 隧道
   - 检测 autossh 是否安装
   - 生成 systemd launchd/user service 保活
   - 映射 remote_port → 127.0.0.1:5342

7. 输出摘要 & 诊断信息
```

#### 2.2 sender.sh 模板（被 install.sh 填充变量后写入）

```bash
#!/bin/bash
# BitCat remote hook sender
# Reads hook JSON from stdin, wraps in {source,machine,payload}, sends via TCP.
set -euo pipefail

MACHINE="__MACHINE__"       # install.sh 替换
HOST="__HOST__"             # install.sh 替换
PORT="__PORT__"             # install.sh 替换
SOURCE="__SOURCE__"         # install.sh 替换 (claude_code | codex)

raw=$(cat || true)
if [ -z "$raw" ]; then exit 0; fi

# 用 python 构造 JSON（避免 jq 依赖 + shell 转义地狱）
envelope=$(python3 -c "
import json,sys
print(json.dumps({'source':'$SOURCE','machine':'$MACHINE','payload':json.loads(sys.stdin.read())}))
" <<< "$raw" 2>/dev/null || printf '{"source":"%s","machine":"%s","payload":%s}' "$SOURCE" "$MACHINE" "$raw")

echo "$envelope" | nc -w 2 "$HOST" "$PORT" || true
exit 0
```

**设计要点**：
- 双策略 JSON 构建：优先 python3（可靠），fallback 纯 shell（兼容）
- `nc -w 2` 2 秒超时，不阻塞 hook
- `exit 0` 兜底 — 即使发送失败也不影响 Claude Code / Codex 运行
- 所有变量由 install.sh 在安装时硬编码写入，运行时零配置

#### 2.3 Hook 配置合并逻辑

与 `claude_hooks.rs` / `codex_hooks.rs` 保持一致的合并策略：
- 只操作带 `bitcat_marker = "bitcat-remote-watch"` 的条目
- 保留用户已有 hook
- 备份原文件
- atomic write（写 tmp → rename）

Claude Code settings.json 中每个事件的 command 形式：

```json
{
  "type": "command",
  "command": "bash /home/user/.bitcat/hooks/sender.sh",
  "bitcat_marker": "bitcat-remote-watch"
}
```

Codex config.toml 形式：

```toml
[[hooks.PreToolUse]]
matcher = "*"

[[hooks.PreToolUse.hooks]]
type = "command"
command = "bash /home/user/.bitcat/hooks/sender.sh"
commandLinux = "bash /home/user/.bitcat/hooks/sender.sh"
timeout = 5
bitcat_marker = "bitcat-remote-watch"
```

### Step 3：前端 Agent Watch 加设备标识（JS/CSS，~30 行）

#### 3.1 `agent_watch.js`

在卡片 meta 区域增加 device badge：

```javascript
// viewOf() 函数扩展
function viewOf(session) {
  return {
    // ... 现有字段 ...
    machine: session.machine || "local",  // 新增
  };
}

// render() 中 task-meta div 增加
`${view.machine !== 'local' ? `<span class="task-device">${escapeHtml(view.machine)}</span>` : ''}`
```

排序函数 `sortedSessions()` 可选按 machine 分组（本地优先 / 按字母序）。

#### 3.2 `agent_watch.css`

```css
.task-device {
  font-size: 11px;
  padding: 1px 6px;
  border-radius: 4px;
  background: rgba(100, 140, 200, 0.15);
  color: #648cc8;
  margin-left: 4px;
}
```

### Step 4：Windows 设置页加远程安装入口（HTML/JS/Rust，~90 行）

#### 4.1 新增 Tauri command

`app/src/agent_monitor.rs` 增加：

```rust
#[tauri::command]
pub async fn cmd_get_remote_install_cmd() -> Result<RemoteInstallInfo, String> {
    // 获取本机局域网 IP
    let local_ip = get_lan_ip()?;
    // 生成 install.sh 的 raw URL（或便携包内路径）
    let script_url = get_install_script_url();
    Ok(RemoteInstallInfo { local_ip, port: DEFAULT_AGENT_MONITOR_PORT, script_url })
}

#[tauri::command]
pub async fn cmd_list_remote_devices(
    monitor: tauri::State<'_, SharedAgentMonitor>,
) -> Result<Vec<DeviceSummary>, String> {
    // 按 machine 分组统计 sessions
}
```

`get_lan_ip()` 实现：枚举网络接口，跳过 127.0.0.1 和虚拟接口，返回首个 IPv4。

#### 4.2 设置页 UI

在现有 Agent Watch 设置区域下方增加：

```
┌─ 远程设备 ─────────────────────────────────────────┐
│                                                    │
│  把以下命令复制到远程 Mac/Linux 终端执行：           │
│  ┌──────────────────────────────────────────────┐  │
│  □ curl -fsSL https://raw.githubusercontent.com/ │  │
│    .../remote-install.sh | bash \                │  │
│    --host 192.168.1.50 --port 5342              │  │
│  └──────────────────────────────────────────────┘  │
│  [📋 复制]                                         │
│                                                    │
│  已连接设备：                                       │
│  • macbook-pro · 2 个活跃会话 · 1m 前更新          │
│  • linux-server · 离线                             │
│                                                    │
│  ☐ 启用时自动建立 SSH 隧道指南                       │
└────────────────────────────────────────────────────┘
```

交互：
- 点击「复制」将命令写入剪贴板
- 设备列表从 `cmd_list_remote_devices` 拉取，定时刷新（10s）
- 离线设备灰显（最后更新 > 60s 无事件）

---

## 文件变更清单

| 文件 | 操作 | 行数估算 | 说明 |
|------|------|---------|------|
| **新增** `scripts/remote-install.sh` | 新增 | ~180 | 远程一键安装脚本 |
| **新增** `docs/guide/remote-agent-setup.md` | 新增 | ~80 | 用户使用文档 |
| `core/src/agent_session.rs` | 改 | ~15 | `AgentSessionEvent` 加 `machine` |
| `core/src/claude_code.rs` | 改 | ~5 | `into_session_event_from` 传 machine |
| `app/src/agent_monitor.rs` | 改 | ~35 | envelope 提取 machine + 2 个新 command |
| `app/frontend/js/agent_watch.js` | 改 | ~20 | 设备 badge 渲染 |
| `app/frontend/css/agent_watch.css` | 改 | ~10 | badge 样式 |
| `app/frontend/js/settings.js` | 改 | ~40 | 远程安装区域逻辑 |
| `app/frontend/html/settings.html` | 改 | ~20 | 远程安装区域 HTML |
| **合计** | | **~405** | |

---

## 测试计划

### Rust 单测

| 测试 | 文件 | 验证内容 |
|------|------|---------|
| `event_carries_machine_to_session` | agent_session.rs | `AgentSessionEvent{machine:Some("mac")}.into_session().machine == Some("mac")` |
| `event_without_machine_defaults_none` | agent_session.rs | 不传 machine 时为 None |
| `envelope_with_machine_parsed` | agent_monitor.rs | `{source:"claude_code",machine:"mbp",payload:{...}}` 正确提取 |
| `envelope_without_machine_fallback` | agent_monitor.rs | 无 machine 字段时为 None |
| `remote_install_cmd_returns_local_ip` | agent_monitor.rs | 返回非 127.0.0.1 的 IPv4 |

### Shell 脚本测试

| 测试 | 验证内容 |
|------|---------|
| `--help` 输出用法 |
| 无 `--host` 报错退出 |
| 检测不到 ~/.claude 时跳过 Claude Code 安装 |
| 检测不到 ~/.codex 时跳过 Codex 安装 |
| sender.sh 生成且含正确 HOST/PORT/MACHINE |
| settings.json 合并保留已有 hook |
| config.toml 合并保留已有 hook |
| `--uninstall` 清理所有 bitcat-remote-watch 条目 |
| `--tunnel` 生成 autossh 命令 |

### 前端测试

| 测试 | 文件 | 验证内容 |
|------|------|---------|
| 远程会话显示设备 badge | agent_watch.test.js | `machine:"macbook-pro"` 时渲染 `.task-device` |
| 本机会话不显示 badge | agent_watch.test.js | `machine:null/undefined` 时不渲染 |
| 复制命令包含本机 IP | settings test | `cmd_get_remote_install_cmd` 返回值含局域网 IP |

### 手动端到端验证

1. **VM 或真机测试**：在 Mac VM 上执行 `curl ...| bash -- --host <Win_IP>`
2. **验证 hook 触发**：远程 Claude Code 提交 prompt → Windows Agent Watch 出现新会话且 machine = hostname
3. **验证 Nudge**：远程进入 PermissionRequest → Windows 端宠物弹出 waiting 提醒
4. **验证断线**：断开隧道 → 远程会话超时标记（或保持最后已知状态）
5. **验证卸载**：`--uninstall` 后远程 hook 全部清理，settings 恢复原样

---

## 风险与缓解

| 风险 | 等级 | 缓解措施 |
|------|------|---------|
| nc 在某些 Linux 发行版缺省不装 | 低 | 脚本检测到时提示 `apt install netcat-openbsd`；sender.sh 也接受用 `/dev/tcp` bash 内建 fallback |
| jq 未安装导致 JSON 构造失败 | 低 | 双策略：优先 python3，fallback 纯 printf（对标准 hook payload 够用） |
| SSH 隧道断开后无感知 | 中 | Windows 端按 session `updated_at_ms` 判断活跃度；超过 5min 无更新的远程 session 标记疑似离线 |
| 多台设备 session_id 冲突 | 低 | 前端展示时 `{machine}:{session_id}` 已足够区分；内部 map key 可考虑加前缀 |
| hook 脚本阻塞 Claude Code | 低 | `nc -w 2` 硬超时 + `exit 0`兜底 + try/catch（sender.sh 里是 `|| true`） |
| Claude Code/Codex hook 格式变更 | 低 | parser 已是宽松反序列化 + extra Map absorb；shell sender 只透传原始 JSON 不解析 |
| 用户在远程手动信任 hook | 中（Codex） | install.sh 输出明确提示："请在 Codex 中信任新 hook"；Claude Code 无此限制 |
| raw URL 被 GFW/网络环境拦截 | 低 | 提供 B 计划：脚本随 portable zip 分发；`--from-file /path/to/install.sh` |

---

## 依赖关系

```
Step 1 (Rust machine 字段)
  ├── 必须先完成：无前置依赖
  └── 验证：make test-core

Step 2 (remote-install.sh)
  ├── 必须先完成：Step 1（需要 machine 字段已就位）
  ├── 并行可做：Step 3、Step 4
  └── 验证：bash -n scripts/remote-install.sh；VM 手动测试

Step 3 (前端 badge)
  ├── 必须先完成：Step 1
  └── 验证：cd app/frontend && npx vitest run

Step 4 (设置页远程入口)
  ├── 必须先完成：Step 1
  └── 验证：make build + 设置页视觉检查
```

Step 2 可以和 Step 3/4 并行开发。Step 1 是其他所有步骤的前置条件（只有 ~30 行，可以快速完成）。
