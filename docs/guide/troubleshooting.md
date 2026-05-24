# 常见问题

## 启动后看不到猫

- 看系统托盘是否有 BitCat 图标。
- 如果开启了“启动时折叠”，猫会以屏幕边缘竖条显示，点击竖条恢复。
- 猫可能在其他显示器边缘或被置顶窗口遮住。可在设置页重置外观，或删除 `~/.bitcat/app_settings.json` 中的 `appearance.pet_position`。

## 启动报错或闪退

- 用 debug 模式启动：

```powershell
.\bitcat.exe --debug
```

- 查看 `~/.bitcat/logs/`。
- 确认 WebView2 Runtime 已安装。
- 源码构建 app/Tauri/SDL2 相关逻辑时优先用 `make build`。如果直接跑 cargo，Windows 下需设置：

```powershell
$env:CMAKE_POLICY_VERSION_MINIMUM="3.5"
cargo check -p ai-pad-app
```

## AI 没响应

- 设置页确认 API Key、Base URL、Model 当前有效值。
- 如果使用代理，确认兼容 Anthropic Messages 和 tool_use 协议。
- 看 `~/.bitcat/logs/` 是否有 API 错误。
- 设置页“用量统计”如果没有 chat token 记录，通常说明请求没有成功完成。

## 工具调用被阻止

这是正常安全边界。`shell`、热键、启动程序等系统操作可能被权限策略拦截。气泡会显示工具状态，模型会收到可解释的 blocked 结果。

工具事件可在 `~/.bitcat/logs/tool_events.jsonl` 查看。

## 手柄不响应

- 8BitDo Micro 确认拨到 D-Input。
- 首次激活方向键：Select + 上方向键约 5 秒。
- 用 `--debug` 启动，看日志里的设备列表和按键编号。
- 如果按键编号不匹配，修改 `config/buttons.yml`。
- 断连后程序会自动重连；长时间失败时重新开关手柄或在 Windows 蓝牙设置中重连。

## 面板按键和普通动作冲突

面板打开时 A/B/D-pad 由面板独占；游戏窗口激活时 D-pad/A/B/Start 由游戏独占。这是预期行为。

## 语音输入没有文字

- Windows 默认语音输入通常是 `Win+H`；当前默认配置是 `["ctrl", "win"]`，请按你的输入法改 `config/actions.yml`。
- 语音输入依赖系统输入法，不是应用自己录音。
- 如果松开后识别结果来得太晚，增大 `voice.delay`。

## 语音串入上次内容

应用使用 generation 防残留机制：每次打开语音窗口都会递增 generation 并清空旧文本，只接受当前 generation 的输入。若仍偶发串入，通常是输入法延迟过长，调大 `delay` 后再试。

## 截图分析不工作

- 确认模型和代理支持图片输入。
- 对话、舞蹈、黑屏、熄屏、画面未变化时会自动跳过截图。
- 手动触发一次托盘“立即截图”，看 `~/.bitcat/screenshots/YYYY-MM-DD/` 是否生成文件。
- 设置页“用量统计”可查看 vision token 是否增长。

## 截图太费 token

- 增大截屏分析间隔。
- 查看是否有频繁变化的动态画面导致 dHash 去重失效。
- 屏幕摘要也会消耗 token，可在 `config/prompts.yml` 调整 `screen_summary.interval_min`。

## 记忆不符合预期

- 短期记忆默认不按条数淘汰，但注入 prompt 时受 `memory.max_context_chars` 限制。
- 长期记忆由模型结构化候选或 `remember` 工具写入，不是所有聊天都会保存。
- 在设置页“记忆与画像”审查长期记忆，必要时删除。
- 重要身份信息建议写进 `config/user.yml` 或设置页显式画像。

## 设置保存后没生效

- 大多数设置会立即生效，gamepad loop 最多下一个 80ms tick 刷新。
- `panel_action.yml` 通常重新打开面板后体现。
- 全局快捷键修改需要重启。
- 如果直接编辑 yml，可用托盘“重载配置”。

## TTS 没声音

- 确认设置页打开了 TTS。
- 确认系统音量和默认语音正常。
- TTS 使用 Windows SAPI，本地播放，不依赖网络。

## 音乐舞动停不下来

有三处停止入口：

- 设置页“用量统计”里的 Music Reactive Dance → 停止。
- 宠物右键菜单 → 停止舞动。
- 系统托盘菜单 → 停止舞动。

当前音乐舞动默认只做 canvas 内动作，不移动真实窗口。如果感觉 UI 卡顿，优先使用托盘停止入口。
