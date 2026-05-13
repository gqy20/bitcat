# 舞蹈系统

8Bit Cat 的舞蹈系统是 AI 驱动的核心玩法之一。AI 可以自主编排完整的舞蹈动作序列，猫咪会即时表演。

## 触发舞蹈

有三种方式触发舞蹈：

| 方式 | 操作 | 说明 |
|------|------|------|
| AI 工具调用 | 对 AI 说"跳个舞" | AI 自主编排舞蹈定义并播放 |
| 手柄 | 短按 **Y** 键 | 播放默认舞蹈 `happy_twist` |
| 面板 | 点击舞蹈按钮 | 播放默认舞蹈 |

## AI 编排舞蹈

当你在对话中让 AI 跳舞时，AI 会调用 `perform_dance` 工具，直接提交完整的舞蹈定义。这不是查表模板——**动作序列的长度、节奏、组合全部由 AI 自主决定**。

示例对话：
```
你：跳个舞
AI：好嘞！让我给你表演一段~
    [调用 perform_dance：name="celebration", steps=[jump×300ms, spin×400ms, wave×500ms, shake×300ms]]
```

AI 编排的舞蹈会自动保存到 `~/.ai-pad/dances/` 目录，下次可以通过名称回放。

## 舞蹈动作

每种动作同时驱动精灵内动画和窗口级移动：

| 动作 | 精灵效果 | 窗口移动 |
|------|---------|---------|
| **jump** | 像素上移 12px | 大跳跃（屏幕高度 14%，bounce 弹性缓动）+ 左右微摇摆 |
| **spin** | 每 60ms 翻转朝向 | 小幅度随机抖动 |
| **wave** | Y 轴正弦波动 ±6px | 上下浮动（屏幕高度 4%） |
| **shake** | X 轴正弦抖动 ±6px | 左右大幅摆动（屏幕宽度 4%） |
| **idle** | 回到默认姿态 | — |

舞蹈结束时窗口以 250ms easeOutCubic 缓动平滑回到起始位置。

## 舞蹈定义

每个舞蹈由一个 YAML 文件定义，包含名称、步骤序列和控制参数：

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
  - action: shake
    duration_ms: 300
    repeat: 3
  - action: idle
    duration_ms: 200
    repeat: 1
```

### 字段说明

- `name`：舞蹈名称，最长 64 字符，仅支持英文/数字/下划线/短横线
- `loop_`：是否循环播放（`true` / `false`）
- `steps`：动作序列

### 校验规则

| 约束 | 限制 |
|------|------|
| 步骤数量 | 1 ~ 24 步 |
| 单步时长 | 80 ~ 5000 ms（建议 150-900） |
| 单步重复次数 | 1 ~ 8 次 |
| 单轮总时长 | 不超过 30 秒 |
| 舞蹈名称 | 最长 64 字符，仅 ASCII 字母数字 + `_` + `-` |

超出限制的舞蹈定义会被拒绝，AI 会收到错误信息并重新编排。

## 舞蹈存储

舞蹈文件按优先级从两个位置加载：

| 优先级 | 路径 | 说明 |
|--------|------|------|
| 1（优先） | `~/.ai-pad/dances/{name}.yaml` | AI 生成 / 用户自定义 |
| 2（兜底） | `config/dances/{name}.yaml` | 项目内置预设 |

内置预设包括 `happy_twist` 和 `default` 等。用户和 AI 生成的舞蹈保存在 `~/.ai-pad/dances/` 目录，优先级高于内置预设。

## 舞蹈播放期间的特殊行为

舞蹈播放期间以下功能会自动暂停：

- **截图管线**：跳过 Vision API 调用，避免浪费 token
- **AI 对话**：不会触发新的对话

舞蹈结束后一切自动恢复。

## 播放已保存的舞蹈

AI 也可以播放之前保存的舞蹈：

```
你：再跳一次 happy_twist
AI：好！再给你跳一遍~
    [调用 play_dance：name="happy_twist"]
```

手柄 Y 键短按默认播放 `happy_twist`。
