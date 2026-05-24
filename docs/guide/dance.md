# 舞蹈、音乐舞动与小游戏

BitCat 的娱乐能力分三类：AI 编舞、音乐响应舞动、桌面小游戏。

## AI 舞蹈

触发方式：

| 方式 | 操作 |
|------|------|
| 对话 | 对 AI 说“跳个舞”“来个庆祝动作” |
| 手柄 | 短按 Y |
| 面板 | 点击“跳舞” |

对话触发时，模型会调用 `perform_dance` 工具，直接提交完整动作序列。Rust 只负责 schema、校验、保存和播放，不再用关键词或 mood 查表生成舞蹈。

## 舞蹈动作

当前支持：

| 动作 | 表现 |
|------|------|
| `jump` | 跳跃 |
| `spin` | 翻转/旋转感 |
| `wave` | 上下波动 |
| `shake` | 左右摇动 |
| `idle` | 回到默认姿态 |

舞蹈定义示例：

```yaml
name: happy_twist
loop_: true
steps:
  - action: jump
    duration_ms: 400
    repeat: 2
  - action: spin
    duration_ms: 500
    repeat: 1
  - action: wave
    duration_ms: 600
    repeat: 2
```

校验规则：

| 约束 | 限制 |
|------|------|
| 步数 | 1 到 24 |
| 单步时长 | 80 到 5000ms |
| repeat | 1 到 8 |
| 单轮总时长 | 不超过 30 秒 |
| 名称 | 最长 64 字符，仅 ASCII 字母数字、`_`、`-` |

保存位置：

| 优先级 | 路径 |
|--------|------|
| 用户/AI 生成 | `~/.bitcat/dances/{name}.yaml` |
| 内置预设 | `config/dances/{name}.yaml` |

## 音乐响应舞动

音乐舞动不是固定编舞，而是实时根据电脑声音驱动宠物 canvas 内动作。为稳定性，当前默认只做 sprite/canvas 内部动作，不移动真实桌面窗口。

入口：

- 设置页 → “用量统计” → Music Reactive Dance 区域
- 托盘菜单或宠物右键菜单 → “停止舞动”

按钮：

| 按钮 | 作用 |
|------|------|
| 模拟 | 使用 fake source 测试舞动表现 |
| WASAPI | 使用 Windows WASAPI loopback 捕获电脑当前播放声音 |
| 停止 | 停止当前音乐舞动会话 |

诊断区会显示：

- 状态、来源、会话 ID、更新时间
- energy / bass
- onset / silence 等标志

音乐帧字段当前包含 `energy`、`bass`、`onset`、`silence`。前端会用 `MusicReactivePlayer` 把这些信号转换成跳、摇、挥等动作。

## 小游戏

面板默认有“游戏”入口，会打开透明全屏游戏窗口。当前已可玩默认 Snake：

| 输入 | 键盘 | 手柄 |
|------|------|------|
| 方向 | 方向键 / WASD | D-pad |
| 确认 | Enter / Space | A |
| 暂停 | P | Start |
| 退出 | Escape | B |

游戏激活时，手柄输入由游戏独占，方向键不会再滚动桌面窗口。游戏结束会通知宠物进入胜利或失败表现。

当前默认路径：

```text
面板“游戏”
  → cmd_start_game
  → app/src/game.rs 创建透明 game 窗口
  → game.html / game_engine.js 加载默认 Snake
  → cmd_game_end 回传结果
```

AI 直接生成小游戏工具仍在计划中。底层 `GameDef`、默认 Snake、窗口生命周期和手柄独占已经落地，后续会复用这条通道接入 `perform_game` / `play_game`。
