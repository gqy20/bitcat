"""
通过快捷键触发系统语音输入法
使用 Windows SendInput API 模拟按键，零外部依赖
"""

import ctypes
import time

user32 = ctypes.windll.user32

VK_CONTROL = 0x11
VK_LWIN = 0x5B
VK_MENU = 0x12


class KEYBDINPUT(ctypes.Structure):
    _fields_ = [
        ("wVk", ctypes.c_ushort),
        ("wScan", ctypes.c_ushort),
        ("dwFlags", ctypes.c_ulong),
        ("time", ctypes.c_ulong),
        ("dwExtraInfo", ctypes.c_ulong),
    ]


class INPUT(ctypes.Structure):
    class _U(ctypes.Union):
        _fields_ = [("ki", KEYBDINPUT)]
    _fields_ = [("type", ctypes.c_ulong), ("u", _U)]


KEYEVENTF_KEYUP = 0x0002
INPUT_KEYBOARD = 1

KEY_MAP = {
    "ctrl": VK_CONTROL, "control": VK_CONTROL,
    "win": VK_LWIN, "windows": VK_LWIN, "super": VK_LWIN,
    "alt": VK_MENU,
    "shift": 0x10,
    "enter": 0x0D, "return": 0x0D,
    "tab": 0x09, "esc": 0x1B, "space": 0x20,
    "a": 0x41, "b": 0x42, "c": 0x43, "d": 0x44,
    "e": 0x45, "f": 0x46, "g": 0x47, "h": 0x48,
}


def _press(vk: int):
    inp = INPUT(type=INPUT_KEYBOARD)
    inp.u.ki.wVk = vk
    user32.SendInput(1, ctypes.byref(inp), ctypes.sizeof(INPUT))


def _release(vk: int):
    inp = INPUT(type=INPUT_KEYBOARD)
    inp.u.ki.wVk = vk
    inp.u.ki.dwFlags = KEYEVENTF_KEYUP
    user32.SendInput(1, ctypes.byref(inp), ctypes.sizeof(INPUT))


def send_hotkey(keys: list[str], hold: float = 0.05):
    """模拟快捷键组合，如 ["ctrl", "win"]"""
    vk_codes = []
    for k in keys:
        vk = KEY_MAP.get(k.lower())
        if vk is None:
            print(f"    [警告] 未知按键: {k}")
            return
        vk_codes.append(vk)

    for vk in vk_codes:
        _press(vk)
    time.sleep(hold)
    for vk in reversed(vk_codes):
        _release(vk)

    label = "+".join(keys)
    print(f"    → 已触发 {label}")
