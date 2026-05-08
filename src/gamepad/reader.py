"""
8BitDo Micro 按键测试工具 (D-Input 模式)
实时显示按键编号、名称、别名
按 Ctrl+C 退出

提示: 方向键需先激活 (按住 Select+↑ 五秒)
"""

import sys
import os
import time
import pygame

os.environ["PYTHONIOENCODING"] = "utf-8"
sys.stdout.reconfigure(encoding="utf-8", line_buffering=True)
sys.stderr.reconfigure(encoding="utf-8")

from gamepad.config import BUTTON_MAP, HAT_DIRS
from gamepad.device import find_gamepad


def run():
    joystick = find_gamepad()
    num_btn = joystick.get_numbuttons()
    num_axes = joystick.get_numaxes()
    num_hats = joystick.get_numhats()

    print()
    print("╔═══════════════════════════════════════════════════════╗")
    print("║         8BitDo Micro 按键测试工具                    ║")
    print("╠═══════════════════════════════════════════════════════╣")
    print(f"║  设备: {joystick.get_name():<44s}║")
    print(f"║  按键: {num_btn}  轴: {num_axes}  Hat: {num_hats}                               ║")
    print("║  Ctrl+C 退出                                         ║")
    print("╚═══════════════════════════════════════════════════════╝")
    print()

    print("┌──────┬──────────┬────────────────────┬──────────────┐")
    print("│ 编号 │   名称   │ 别名               │ 位置         │")
    print("├──────┼──────────┼────────────────────┼──────────────┤")
    for btn_id in range(num_btn):
        info = BUTTON_MAP.get(btn_id)
        if info:
            name, aliases, pos = info
            alias_str = "/".join(aliases)
            print(f"│  {btn_id:2d}  │ {name:<8s} │ {alias_str:<18s} │ {pos:<12s} │")
        else:
            print(f"│  {btn_id:2d}  │ {'─':<8s} │ {'(未映射)':<18s} │ {'':<12s} │")
    print("├──────┼──────────┼────────────────────┼──────────────┤")
    print("│ Hat0 │ 方向键   │ ↑↓←→              │ 左侧十字键   │")
    print("└──────┴──────────┴────────────────────┴──────────────┘")
    print()
    print("  提示: 方向键无反应时，按住 Select + ↑ 五秒激活")
    print("─" * 58)

    pressed = set()
    prev_hat = (0, 0)
    hat_active = False
    total = 0

    try:
        while True:
            pygame.event.pump()
            now = time.strftime("%H:%M:%S")

            for i in range(num_btn):
                p = joystick.get_button(i)
                info = BUTTON_MAP.get(i)
                name = info[0] if info else f"按钮{i}"
                alias_str = "/".join(info[1]) if info else ""
                pos = info[2] if info else ""

                if p and i not in pressed:
                    pressed.add(i)
                    total += 1
                    label = f"{name} ({alias_str})" if alias_str else name
                    print(f"[{now}] ▶ 按下  按钮{i:2d} | {label:<18s} | {pos}")
                elif not p and i in pressed:
                    pressed.discard(i)
                    print(f"[{now}] ■ 释放  按钮{i:2d} | {name}")

            for i in range(num_hats):
                val = joystick.get_hat(i)
                if val != prev_hat:
                    if val != (0, 0):
                        arrow, cname = HAT_DIRS.get(val, ("?", str(val)))
                        total += 1
                        print(f"[{now}] ▶ 方向键 | {arrow} {cname:<6s} | Hat值: {val}")
                        hat_active = True
                    elif hat_active:
                        print(f"[{now}] ■ 方向键 回中")
                        hat_active = False
                    prev_hat = val

            time.sleep(0.016)

    except KeyboardInterrupt:
        print()
        print("─" * 58)
        print(f"结束。总按键: {total}，触发按钮: {sorted(pressed)}")
        pygame.quit()
