#!/usr/bin/env python3
"""从 sprite.js 像素数据生成应用图标（32/128/256 PNG + ICO）"""
from PIL import Image
import os

# 调色板：索引 → (r, g, b, a)
PALETTE = {
    0: (0, 0, 0, 0),           # 透明
    1: (30, 30, 40, 255),      # 轮廓
    2: (255, 180, 140, 255),   # 肤色
    3: (255, 220, 190, 255),   # 高光
    4: (40, 35, 50, 255),      # 眼睛
    5: (255, 120, 140, 255),   # 嘴巴/腮红
}

# IDLE_BASE 帧（16×16，行优先）
IDLE_BASE = [
    0,0,0,1,1,1,1,1,1,1,1,0,0,0,0,0,
    0,0,1,2,2,2,1,1,1,2,2,2,1,0,0,0,
    0,0,1,2,2,2,2,1,2,2,2,2,1,0,0,0,
    0,1,1,2,2,2,2,2,2,2,2,2,1,1,0,0,
    0,1,2,2,3,2,2,2,2,2,3,2,2,1,0,0,
    0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
    0,1,2,4,4,2,2,2,2,2,4,4,2,1,0,0,
    0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
    0,1,2,2,2,5,2,2,2,5,2,2,2,1,0,0,
    0,1,2,2,2,2,2,1,2,2,2,2,2,1,0,0,
    0,1,2,2,2,2,2,2,2,2,2,2,2,1,0,0,
    0,0,1,2,2,2,2,2,2,2,2,2,1,0,0,0,
    0,0,0,1,1,2,2,2,2,1,1,0,0,0,0,0,
    0,0,0,0,1,1,1,1,1,1,0,0,0,0,0,0,
    0,0,0,0,0,1,1,1,1,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
]


def make_image(size: int) -> Image.Image:
    """渲染 IDLE_BASE 到指定尺寸（最近邻缩放保持像素风格）"""
    small = Image.new('RGBA', (16, 16))
    pixels = small.load()
    for y in range(16):
        for x in range(16):
            pixels[x, y] = PALETTE[IDLE_BASE[y * 16 + x]]
    return small.resize((size, size), Image.NEAREST)


def main():
    script_dir = os.path.dirname(os.path.abspath(__file__))
    icon_dir = os.path.join(script_dir, '..', 'app', 'icons')
    icon_dir = os.path.normpath(icon_dir)
    os.makedirs(icon_dir, exist_ok=True)

    # PNG
    for name, size in [('32x32', 32), ('128x128', 128), ('128x128@2x', 256)]:
        img = make_image(size)
        path = os.path.join(icon_dir, f'{name}.png')
        img.save(path)
        print(f'  {name}.png ({size}x{size})')

    # ICO (多尺寸打包)
    ico_sizes = [16, 32, 48, 256]
    ico_images = [make_image(s) for s in ico_sizes]
    ico_path = os.path.join(icon_dir, 'icon.ico')
    ico_images[0].save(
        ico_path, format='ICO',
        sizes=[(s, s) for s in ico_sizes],
        append_images=ico_images[1:],
    )
    print(f'  icon.ico ({len(ico_sizes)} sizes)')

    # ICNS (macOS) — Pillow 不原生支持，保存 256x256 PNG 作占位
    icns_path = os.path.join(icon_dir, 'icon.icns')
    make_image(256).save(icns_path)
    print(f'  icon.icns (256x256 PNG placeholder)')

    print('done')


if __name__ == '__main__':
    main()
