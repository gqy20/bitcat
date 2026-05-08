"""手柄设备查找与初始化"""

import pygame
import sys


def find_gamepad() -> pygame.joystick.Joystick:
    """查找并返回 8BitDo Micro 手柄，找不到则退出"""
    pygame.init()
    pygame.joystick.init()

    target = None
    for i in range(pygame.joystick.get_count()):
        j = pygame.joystick.Joystick(i)
        j.init()
        if "Controller" in j.get_name() and "Wireless" in j.get_name():
            target = i
            break

    if target is None:
        print("[错误] 未找到 8BitDo Micro 手柄")
        print("请确认: 1.手柄已开启 2.模式开关在 D 挡 3.蓝牙已配对")
        pygame.quit()
        sys.exit(1)

    joystick = pygame.joystick.Joystick(target)
    joystick.init()
    return joystick
