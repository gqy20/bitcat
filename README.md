# ai-pad

AI 驱动的蓝牙手柄控制器。通过手柄按键触发系统操作、AI 调用和语音交互。

## 快速开始

```bash
# 安装依赖
uv sync

# 按键测试（查看手柄按键编号和名称）
uv run ai-pad-read

# 启动手柄控制器
uv run ai-pad-ctl
```

## 手柄配对

以 8BitDo Micro 为例：

1. 将手柄背面模式开关拨到 **D**（D-Input 模式）
2. 按住 Pair 按钮 1 秒，LED 快闪
3. Windows 蓝牙设置中搜索配对
4. 首次使用方向键需激活：**按住 Select + ↑ 五秒**

## 项目结构

```
ai-pad/
├── buttons.yml         # 硬件按键映射（换手柄时改这里）
├── actions.yml         # 按键动作绑定（个性化配置）
├── pyproject.toml
└── src/ai_pad/
    ├── config.py       # 从 buttons.yml 加载映射
    ├── device.py       # 手柄查找与连接
    ├── reader.py       # 按键测试工具
    ├── ctl.py          # 控制器主程序
    └── voice.py        # 语音录制与识别
```

## 配置说明

### buttons.yml — 硬件映射

手柄按键到编号的映射，每种手柄不同。通过 `ai-pad-read` 实测校准后填入。

### actions.yml — 按键动作

支持以下动作类型：

**launch** — 启动程序

```yaml
Start:
  type: launch
  program: claude
  args: "--dangerously-skip-permissions"
  workdir: "D:\\C\\Desktop\\ai"
  terminal: true          # 在新终端中打开
  window: maximized       # maximized / normal / minimized
```

**voice** — 录音 + 语音识别 + 启动程序

```yaml
Y:
  type: voice
  program: claude
  args_template: "-p \"{text}\""   # {text} 替换为识别结果
  workdir: "D:\\C\\Desktop\\ai\\research"
  terminal: true
  voice:
    duration: 5           # 录音秒数
    language: "zh-CN"     # 识别语言
```

**script** — 执行 shell 命令

```yaml
A:
  type: script
  command: "python my_script.py"
```

## 支持的手柄

| 手柄 | 模式 | 状态 |
|------|------|------|
| 8BitDo Micro | D-Input (蓝牙) | 已测试 |
| 其他手柄 | D-Input / XInput | 需自行校准 buttons.yml |

添加新手柄：`uv run ai-pad-read` 逐个按键测试，将结果填入 `buttons.yml`。

## 技术栈

- Python 3.12+ / uv
- pygame — 手柄输入读取
- sounddevice + SpeechRecognition — 语音录制与识别
- PyYAML — 配置加载

## License

MIT
