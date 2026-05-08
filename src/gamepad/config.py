"""
8BitDo Micro 按键配置
从 buttons.yml 加载映射表
"""

from pathlib import Path
import yaml

_CONFIG_PATH = Path(__file__).resolve().parents[2] / "buttons.yml"


def _load_config():
    with open(_CONFIG_PATH, encoding="utf-8") as f:
        return yaml.safe_load(f)


_cfg = _load_config()

# {按钮编号: (名称, 别名列表, 位置)}
BUTTON_MAP = {
    int(k): (v["name"], v["aliases"], v["position"])
    for k, v in _cfg["buttons"].items()
}

# {Hat值元组: (箭头, 中文名)}
HAT_DIRS = {
    tuple(int(x) for x in k.strip("()").split(",")): (v["arrow"], v["name"])
    for k, v in _cfg["hat"].items()
}

BUTTON_NAMES = {btn_id: info[0] for btn_id, info in BUTTON_MAP.items()}

DPAD_HINT = _cfg.get("dpad_activation", "")


def btn(name: str) -> int | None:
    """按名称反查按钮编号"""
    for btn_id, (n, _, _) in BUTTON_MAP.items():
        if n == name:
            return btn_id
    return None
