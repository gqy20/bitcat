# ai-pad

AI 驱动的蓝牙手柄控制器。通过手柄按键触发系统操作、AI 调用和语音交互。

## 快速开始

```bash
# 构建
make build

# 启动（后台运行，系统托盘图标）
make ctl

# 调试模式（弹出控制台窗口，显示日志）
cargo run --bin ai-pad-ctl -- --debug

# 按键测试（查看手柄按键编号和名称）
make read
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
├── Cargo.toml
├── Makefile            # 构建快捷命令
└── src/
    ├── main.rs         # 入口：托盘 + 单实例检测 + 手柄循环
    ├── lib.rs          # 库根模块声明
    ├── config.rs       # YAML 配置加载（buttons / actions）
    ├── action.rs       # 动作定义与解析
    ├── device.rs       # 按键编号 → 名称映射
    ├── joystick.rs     # SDL2 手柄输入读取
    ├── hotkey.rs       # Win32 SendInput 键鼠模拟
    └── tray.rs         # 系统托盘 + GDI 绘制手柄图标
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
```

**voice** — 触发系统语音输入法快捷键

```yaml
Y:
  type: voice
  voice:
    trigger: ["ctrl", "win"]   # 系统语音输入法快捷键
    delay: 1.0                  # 启动后等待时间（秒）
```

**hotkey** — 发送键盘组合键

```yaml
L1:
  type: hotkey
  trigger: ["alt", "tab"]      # Alt+Tab 切换窗口（支持按住状态）

R1:
  type: hotkey
  trigger: ["alt", "backtick"] # Alt+` 打开 uTools

L2:
  type: hotkey
  trigger: ["ctrl", "tab"]     # Ctrl+Tab 切换标签页（支持按住状态）
```

**script** — 执行 PowerShell 命令

```yaml
A:
  type: script
  command: "python my_script.py"
```

### 方向键映射

方向键默认映射为鼠标滚轮滚动：
- 上/下 → 垂直滚动
- 左/右 → 水平滚动
- 长按持续滚动（80ms 间隔，3 倍速）

## 运行方式

| 方式 | 行为 |
|------|------|
| 双击 exe | 后台运行，无控制台窗口，托盘显示手柄图标 |
| `--debug` 参数 | 弹出控制台，输出带时间戳的日志 |
| 右键托盘 | 重载配置 / 退出 |
| 重复启动 | 弹窗提示已在运行 |

## 支持的手柄

| 手柄 | 模式 | 状态 |
|------|------|------|
| 8BitDo Micro | D-Input (蓝牙) | 已测试 |
| 其他手柄 | D-Input / XInput (SDL2) | 需自行校准 buttons.yml |

添加新手柄：`make read` 逐个按键测试，将结果填入 `buttons.yml`。

## 技术栈

- Rust 2024 edition
- SDL2 (`bundled`) — 手柄输入读取（DirectInput 后端）
- windows-sys 0.61 — SendInput 键鼠模拟、系统托盘、GDI 图标绘制
- serde + serde_yaml — 配置加载
- 零运行时依赖：单文件 exe + yml 配置即可运行

## License

MIT
