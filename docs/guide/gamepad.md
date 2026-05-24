# 手柄操作

8Bit Cat 原生支持 SDL2 手柄。默认配置按 8BitDo Micro D-Input 模式校准，其他手柄可通过 `config/buttons.yml` 调整按键编号映射。

## 8BitDo Micro 配对

1. 将背面模式拨到 D，也就是 D-Input。
2. 长按 Pair 约 1 秒，LED 快闪。
3. 在 Windows 蓝牙设置中配对。
4. 首次使用方向键时，按住 Select + 上方向键约 5 秒激活 D-pad。

程序启动后会自动扫描手柄，断连后每约 1 秒尝试重连。

## 默认按键

| 按键 | 面板关闭时 | 面板打开时 | 游戏中 |
|------|------------|------------|--------|
| A | 夸奖猫咪 | 确认选择 | 确认/开始 |
| B | 随机走动 | 关闭面板 | 取消/退出 |
| Y 短按 | 默认舞蹈 | 忽略 | 游戏独占时忽略普通动作 |
| Y 按住 | 语音输入 | 忽略 | 游戏独占时忽略普通动作 |
| Start | 执行 `actions.yml` 的 Start 动作 | 忽略 | 暂停/继续 |
| Select | 睡眠/唤醒 | 忽略 | 游戏独占时忽略普通动作 |
| Home | 打开/关闭面板 | 打开/关闭面板 | 游戏窗口优先 |
| L1 | Alt+Tab | 忽略 | 游戏独占时忽略普通动作 |
| R1 | Alt+` | 忽略 | 游戏独占时忽略普通动作 |
| L2 | Ctrl+Tab | 忽略 | 游戏独占时忽略普通动作 |
| R2 | 立即截图分析 | 忽略 | 游戏独占时忽略普通动作 |
| D-pad | 滚动当前窗口 | 移动选中项 | 控制游戏方向 |

## 输入优先级

按键处理按以下优先级拦截：

1. 游戏窗口激活：D-pad / A / B / Start 发送给游戏，不触发桌面滚轮或宠物动作。
2. Home：切换面板。
3. 面板可见：A/B/D-pad 由面板独占。
4. 语音按住态：处理语音输入生命周期。
5. 普通宠物动作和 `actions.yml` 绑定。

这能避免一边玩游戏一边滚动网页，或在面板里确认时误触其他动作。

## 方向键

面板关闭时，方向键模拟鼠标滚轮：

| 方向 | 效果 |
|------|------|
| 上/下 | 垂直滚动 |
| 左/右 | 水平滚动 |

面板打开时，方向键在 `config/panel_action.yml` 配置的网格中移动。当前默认 2x2，对应四个内置小游戏入口。

## 自定义按键

编辑 `config/actions.yml` 或在设置页“按键与操作”中修改。

```yaml
actions:
  L1:
    type: hotkey
    trigger: ["alt", "tab"]

  R2:
    type: screenshot
    keyboard_shortcut: "CommandOrControl+Alt+S"

  Y:
    type: voice
    voice:
      trigger: ["ctrl", "win"]
      delay: 1.0
```

不同手柄按键编号不一致时，修改 `config/buttons.yml`。用 `ai-pad-app.exe --debug` 启动后，按键日志会显示原始编号，方便校准。
