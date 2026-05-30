# AI 对话与记忆

AI 对话是 BitCat 的主能力。它会流式回复、调用工具、根据对话结果改变宠物情绪，并把值得长期保留的信息写入可审查的记忆文件。

## 触发方式

| 方式 | 操作 | 说明 |
|------|------|------|
| 鼠标 | 点击猫咪嘴巴，或双击气泡 | 打开聊天输入框 |
| 面板 | 打开面板后选择“聊天” | 打开聊天输入框 |
| 语音 | 按住配置为 `voice` 的按键，默认 Y | 依赖 Windows 语音输入 |
| 手柄动作 | `Start` 默认启动 Claude CLI | 可在 `actions.yml` 改成其他动作 |

聊天输入支持中文 IME。Enter 发送，Esc 关闭。AI 回复结束后输入框会短暂展开，便于追问。

## 气泡窗口

- 回复流式显示，长内容会自动增高，最高约 340px。
- 超长内容可用鼠标滚轮滚动；向上滚动时会锁定阅读位置，回到底部后解锁。
- 可拖拽右下角调整大小，双击右下角恢复自动大小。
- 工具调用会在气泡中显示低干扰状态行，例如准备工具、执行中、已阻止、失败。
- `perform_dance` / `play_dance` 这类表演型工具会让气泡短暂提示后退场，把主视觉交给宠物。

## AI 工具

当前 Agent 注册了 15 个工具：

| 工具 | 用途 |
|------|------|
| `launch_program` | 启动程序 |
| `shell` | 执行 shell 命令 |
| `read_file` | 读取文件 |
| `get_time` | 获取当前时间 |
| `recent_screenshots` | 查看最近截图分析 |
| `search_memory` | 检索长期记忆 |
| `remember` | 保存明确的长期记忆 |
| `create_reminder` | 创建一次性或重复提醒 |
| `list_reminders` | 查看提醒任务 |
| `cancel_reminder` | 取消提醒 |
| `send_hotkey` | 发送键盘组合键 |
| `read_clipboard` | 读取剪贴板 |
| `force_foreground` | 聚焦窗口 |
| `perform_dance` | 编排并播放一段舞蹈 |
| `play_dance` | 播放已保存或内置舞蹈 |

完成、稍后和删除提醒由通知窗口与设置页处理，目前不作为 Agent Tool 暴露。

危险或系统控制类操作仍由权限边界拦截。被阻止时，工具结果会以可解释结果返回给模型，宠物也会进入对应的阻止/失败表现。

工具事件会写入 `~/.bitcat/logs/tool_events.jsonl`，只保存短 preview，不保存大文本。

## 宠物语义事件

宠物不再靠“回复里有哈哈就开心”这类关键词判断情绪。当前流程是：

1. 对话开始：发送 `AiThinking` / `AiWriting` 通知。
2. 模型准备工具：发送 `ToolPreparing`。
3. 工具运行、失败或被阻止：发送对应 `ToolRunning` / `ToolFailed` / `ToolBlocked`。
4. 对话结束：通过 `AgentReaction` 结构化抽取最终情绪、可选 speech 和长期记忆候选。
5. `PetEventBus` 统一做去重、节流、TTL 和最近 50 条事件日志。

在设置页“用量统计”中可以查看宠物事件日志，排查为什么某个动画被发送、去重或节流。

## 记忆系统

### 短期记忆

- 保存对话到 `~/.bitcat/memory/chat_summary.json`。默认 `memory.max_entries: 0` 表示不按条数淘汰，由 `max_context_chars` 控制注入长度。
- 单条用户消息和 AI 回复会按字符截断。
- 下次对话时作为最近上下文注入。

### 长期记忆

- 主文件：`~/.bitcat/memory/long_term.jsonl`，同时生成 `long_term.md` 作为人工审查和 `rg` 搜索视图。
- 一行一条记录，包含稳定 `id`、`created_at`、`summary`、`tags`、`importance`、`source`、`deleted`。
- 写入来源：对话结束后的 `AgentReaction.memory_candidates`，或模型显式调用 `remember` 工具。
- 检索方式：`search_memory` / `retrieve_with()` 按 text、tag、source、min_importance 做 grep-first 候选召回；自动注入预算默认 8000 字符，工具按需检索上限 12000 字符。
- 设置页可查看最近条目并按 id 删除。

项目明确不把 Embeddings / Vector RAG 作为记忆主线。需要召回历史时，先用可解释文本筛候选，再交给模型判断相关性。

### 用户画像

如果 `config/user.yml` 或设置页中填写了显式用户信息，它会优先注入：

```text
[关于主人]
...
[/关于主人]
```

显式画像全空时，才使用 AI 从长期记忆聚合出的 `profile.json`。

## TTS

TTS 默认关闭。开启后，AI 回复结束时使用 Windows SAPI 本地朗读：

- 不依赖网络。
- 新朗读会打断上一段未完成朗读。
- 使用系统默认语音，可在 Windows 语音设置中调整。

## 建议用法

- 想让它长期记住某事，可以直接说“记住：我更喜欢中文简洁回答”。
- 想引用最近屏幕，可以问“我刚才在看什么？”或“帮我总结最近屏幕活动”。
- 想让它行动，可以自然说“帮我打开 VS Code”“按一下 Ctrl+S”“跳个庆祝舞”。
