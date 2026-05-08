"""
8BitDo Micro 手柄控制器
从 actions.yml 加载按键绑定，执行对应动作
按 Ctrl+C 退出
"""

import sys
import os
import time
import subprocess
from pathlib import Path

import pygame
import yaml

os.environ["PYTHONIOENCODING"] = "utf-8"
sys.stdout.reconfigure(encoding="utf-8", line_buffering=True)
sys.stderr.reconfigure(encoding="utf-8")

from ai_pad.config import BUTTON_NAMES, btn
from ai_pad.device import find_gamepad

_ACTIONS_PATH = Path(__file__).resolve().parents[2] / "actions.yml"


def _load_actions():
    with open(_ACTIONS_PATH, encoding="utf-8") as f:
        return yaml.safe_load(f)


def _window_style(name: str) -> str:
    mapping = {"maximized": "Maximized", "minimized": "Minimized", "normal": "Normal"}
    return mapping.get(name, "Normal")


def _make_handler(action_cfg, defaults):
    atype = action_cfg.get("type")

    if atype == "launch":
        program = action_cfg["program"]
        args = action_cfg.get("args", "")
        workdir = action_cfg.get("workdir", "")
        open_terminal = action_cfg.get("terminal", False)
        window = action_cfg.get("window", defaults.get("window", "normal"))

        def handler():
            if open_terminal:
                shell = defaults.get("terminal", "powershell")
                cmd_parts = []
                if workdir:
                    cmd_parts.append(f"cd {workdir}")
                cmd_parts.append(f"{program} {args}".strip())
                cmd_str = "; ".join(cmd_parts)
                style = _window_style(window)
                subprocess.Popen([
                    shell, "-Command",
                    f"Start-Process {shell} -ArgumentList "
                    f"'-NoExit','-Command','{cmd_str}' -WindowStyle {style}"
                ])
            else:
                cwd = workdir or None
                subprocess.Popen(
                    [program] + (args.split() if args else []),
                    cwd=cwd, creationflags=subprocess.DETACHED_PROCESS, close_fds=True,
                )
            label = f"{program} {args}".strip()
            loc = f" @ {workdir}" if workdir else ""
            print(f"    → 已启动 {label}{loc}")

        return handler

    elif atype == "voice":
        program = action_cfg.get("program", "")
        workdir = action_cfg.get("workdir", "")
        open_terminal = action_cfg.get("terminal", False)
        window = action_cfg.get("window", defaults.get("window", "normal"))
        voice_cfg = action_cfg.get("voice", {})
        hotkey = voice_cfg.get("trigger", [])
        delay = voice_cfg.get("delay", 1.0)

        def handler():
            from ai_pad.voice import send_hotkey

            # 先启动终端（如果需要）
            if open_terminal and program:
                shell = defaults.get("terminal", "powershell")
                cmd_parts = []
                if workdir:
                    cmd_parts.append(f"cd {workdir}")
                cmd_parts.append(program)
                cmd_str = "; ".join(cmd_parts)
                style = _window_style(window)
                subprocess.Popen([
                    shell, "-Command",
                    f"Start-Process {shell} -ArgumentList "
                    f"'-NoExit','-Command','{cmd_str}' -WindowStyle {style}"
                ])
                loc = f" @ {workdir}" if workdir else ""
                print(f"    → 已启动 {program}{loc}")
                time.sleep(delay)

            # 触发系统语音输入法
            if hotkey:
                send_hotkey(hotkey)
            else:
                print("    [警告] 未配置 voice.trigger 快捷键")

        return handler

    elif atype == "script":
        command = action_cfg["command"]

        def handler():
            subprocess.Popen(command, shell=True, creationflags=subprocess.DETACHED_PROCESS)
            print(f"    → 执行: {command}")

        return handler

    elif atype == "hotkey":
        print(f"    [提示] hotkey 类型暂未实现")
        return lambda: None

    else:
        return lambda: print(f"    [未知动作类型: {atype}]")


def run():
    joystick = find_gamepad()
    cfg = _load_actions()
    defaults = cfg.get("defaults", {})

    # 构建运行时动作表: {按钮编号: (描述, handler)}
    actions = {}
    for key_name, action_cfg in cfg.get("actions", {}).items():
        button_id = btn(key_name)
        if button_id is None:
            print(f"[警告] 未知按键名: {key_name}")
            continue
        desc = action_cfg.get("description", f"{key_name} → {action_cfg.get('type', '?')}")
        if "description" not in action_cfg:
            program = action_cfg.get("program", action_cfg.get("command", ""))
            desc = f"{key_name} → {program}"
        actions[button_id] = (desc, _make_handler(action_cfg, defaults))

    print("=" * 55)
    print(f"  8BitDo Micro 手柄控制器")
    print(f"  设备: {joystick.get_name()}")
    print("=" * 55)
    print("  按键绑定:")
    for btn_id, (desc, _) in actions.items():
        print(f"    {desc}")
    print("  未绑定按键仅显示名称")
    print("  Ctrl+C 退出")
    print("=" * 55)
    print()

    pressed_buttons = set()

    try:
        while True:
            pygame.event.pump()
            now = time.strftime("%H:%M:%S")

            for i in range(joystick.get_numbuttons()):
                pressed = joystick.get_button(i)
                name = BUTTON_NAMES.get(i, f"Btn{i}")

                if pressed and i not in pressed_buttons:
                    pressed_buttons.add(i)
                    print(f"[{now}] ▶ {name}", end="")
                    if i in actions:
                        desc, handler = actions[i]
                        handler()
                    else:
                        print("  (未绑定)")
                elif not pressed and i in pressed_buttons:
                    pressed_buttons.discard(i)
                    print(f"[{now}] ■ {name}")

            time.sleep(0.016)

    except KeyboardInterrupt:
        print(f"\n\n[{time.strftime('%H:%M:%S')}] 退出手柄控制器。")
    finally:
        pygame.quit()
