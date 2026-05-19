#!/usr/bin/env python3
"""Generate app icons from a polished Hackmark app mark.

The pet spritesheet is optimized for a tiny desktop companion window. App
icons need cleaner geometry, heavier small-size strokes, and more intentional
padding, so this script draws a dedicated high-resolution Hackmark mark while
keeping the same visual identity as the V2 pet.
"""

from __future__ import annotations

import os
from pathlib import Path

import math

from PIL import Image, ImageDraw, ImageFilter


ROOT = Path(__file__).resolve().parents[1]
ICON_DIR = ROOT / "app" / "icons"
SOURCE_SIZE = 1024


def polygon_points(cx: float, cy: float, radius: float) -> list[tuple[float, float]]:
    return [
        (cx, cy - radius),
        (cx + radius * 0.866, cy - radius * 0.5),
        (cx + radius * 0.866, cy + radius * 0.5),
        (cx, cy + radius),
        (cx - radius * 0.866, cy + radius * 0.5),
        (cx - radius * 0.866, cy - radius * 0.5),
    ]


def draw_glow(image: Image.Image, xy: tuple[float, float], radius: float, color: tuple[int, int, int, int]) -> None:
    layer = Image.new("RGBA", image.size, (0, 0, 0, 0))
    draw = ImageDraw.Draw(layer)
    x, y = xy
    draw.ellipse((x - radius, y - radius, x + radius, y + radius), fill=color)
    layer = layer.filter(ImageFilter.GaussianBlur(radius * 0.45))
    image.alpha_composite(layer)


def draw_polyline(draw: ImageDraw.ImageDraw, points: list[tuple[float, float]], fill, width: int) -> None:
    draw.line(points, fill=fill, width=width, joint="curve")


def draw_chevron(draw: ImageDraw.ImageDraw, x: float, y: float, size: float, fill, width: int, flip: bool = False) -> None:
    direction = -1 if flip else 1
    draw_polyline(
        draw,
        [
            (x - direction * size * 0.58, y - size * 0.62),
            (x + direction * size * 0.22, y),
            (x - direction * size * 0.58, y + size * 0.62),
        ],
        fill,
        width,
    )


def render_source_icon() -> Image.Image:
    scale = 4
    size = SOURCE_SIZE * scale
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    def s(value: float) -> float:
        return value * scale

    cx = s(512)
    cy = s(462)
    outer_r = s(318)
    inner_r = s(258)
    core_r = s(205)

    cyan = (112, 246, 214, 255)
    cyan_soft = (70, 197, 204, 210)
    blue = (88, 166, 255, 255)
    dark = (13, 21, 43, 255)
    deep = (8, 13, 29, 255)
    panel = (18, 35, 61, 255)
    mint = (145, 255, 226, 255)
    white = (246, 255, 248, 255)

    # Soft outer aura and grounding shadow.
    draw_glow(img, (cx, cy), s(250), (59, 255, 219, 58))
    draw_glow(img, (cx + s(70), cy - s(20)), s(160), (103, 120, 255, 48))
    draw.ellipse((s(282), s(812), s(742), s(910)), fill=(2, 8, 22, 86))

    outer = polygon_points(cx, cy, outer_r)
    mid = polygon_points(cx, cy, s(292))
    inner = polygon_points(cx, cy, inner_r)
    core = polygon_points(cx, cy, core_r)

    # Layered beveled hex body.
    draw.polygon(outer, fill=(4, 10, 24, 255))
    draw.line(outer + [outer[0]], fill=(178, 255, 239, 170), width=int(s(18)), joint="curve")
    draw.line(outer + [outer[0]], fill=cyan, width=int(s(11)), joint="curve")

    draw.polygon(mid, fill=(16, 26, 52, 255))
    draw.line(mid + [mid[0]], fill=(34, 63, 100, 255), width=int(s(14)), joint="curve")

    # Subtle top-left highlight panel.
    draw.polygon(inner, fill=dark)
    highlight = polygon_points(cx - s(12), cy - s(18), inner_r * 0.94)
    draw.line(highlight[:3], fill=(120, 255, 232, 90), width=int(s(5)), joint="curve")
    draw.polygon(core, fill=panel)

    # Screen gradient approximation with translucent bands.
    for i in range(7):
        inset = s(i * 10)
        points = polygon_points(cx, cy + s(8), core_r - inset)
        alpha = max(16, 88 - i * 10)
        draw.polygon(points, fill=(25, 48, 83, alpha))

    # Scan line and screen details.
    draw.line((cx - s(205), cy - s(100), cx + s(205), cy - s(100)), fill=(132, 255, 237, 82), width=int(s(6)))
    draw.line((cx - s(190), cy + s(90), cx + s(190), cy + s(90)), fill=(82, 169, 255, 72), width=int(s(5)))

    # Command glyph face. Heavier strokes survive 16px better.
    draw_chevron(draw, s(406), cy - s(8), s(92), cyan, int(s(28)))
    draw_chevron(draw, s(618), cy - s(8), s(92), cyan, int(s(28)), flip=True)
    draw.line((s(432), cy + s(126), s(592), cy + s(126)), fill=blue, width=int(s(24)))
    draw.line((s(432), cy + s(126), s(515), cy + s(126)), fill=(135, 205, 255, 255), width=int(s(9)))

    # Small terminal plinth gives the mark app identity without making it busy.
    plinth = (s(336), s(762), s(688), s(896))
    draw.rounded_rectangle(plinth, radius=int(s(48)), fill=deep, outline=(38, 48, 79, 255), width=int(s(10)))
    draw_chevron(draw, s(408), s(828), s(48), white, int(s(20)))
    draw.line((s(482), s(830), s(602), s(830)), fill=cyan, width=int(s(18)))
    draw.line((s(482), s(830), s(536), s(830)), fill=mint, width=int(s(7)))

    # Corner pixels echo the sprite, but are aligned and crisp.
    draw.rectangle((s(226), s(222), s(260), s(264)), fill=(230, 238, 230, 255))
    draw.rectangle((s(764), s(222), s(798), s(264)), fill=(230, 238, 230, 255))
    draw.rectangle((s(245), s(690), s(279), s(732)), fill=(230, 238, 230, 220))
    draw.rectangle((s(745), s(690), s(779), s(732)), fill=(230, 238, 230, 220))

    # Final crisp inner strokes.
    draw.line(inner + [inner[0]], fill=cyan_soft, width=int(s(5)), joint="curve")
    draw.line(core + [core[0]], fill=(100, 255, 226, 132), width=int(s(4)), joint="curve")

    return img.resize((SOURCE_SIZE, SOURCE_SIZE), Image.Resampling.LANCZOS)


def make_icon(size: int) -> Image.Image:
    source = render_source_icon()
    resample = Image.Resampling.LANCZOS
    icon = source.resize((size, size), resample)
    if size <= 32:
        # Tiny taskbar/tray sizes benefit from a touch more contrast.
        alpha = icon.getchannel("A")
        sharpened = icon.filter(ImageFilter.UnsharpMask(radius=0.7, percent=95, threshold=3))
        sharpened.putalpha(alpha)
        return sharpened
    return icon


def main() -> None:
    ICON_DIR.mkdir(parents=True, exist_ok=True)

    png_specs = [("32x32.png", 32), ("128x128.png", 128), ("128x128@2x.png", 256)]
    for filename, size in png_specs:
        out = ICON_DIR / filename
        make_icon(size).save(out)
        print(f"  {filename} ({size}x{size})")

    ico_sizes = [16, 32, 48, 64, 128, 256]
    make_icon(256).save(
        ICON_DIR / "icon.ico",
        format="ICO",
        sizes=[(size, size) for size in ico_sizes],
    )
    print(f"  icon.ico ({len(ico_sizes)} sizes)")

    icns = make_icon(512)
    try:
        icns.save(ICON_DIR / "icon.icns", format="ICNS")
        print("  icon.icns")
    except Exception:
        # Some Pillow builds lack full ICNS support; Tauri Windows builds do not
        # consume this file, so keep a PNG-compatible fallback at the same path.
        icns.save(ICON_DIR / "icon.icns", format="PNG")
        print("  icon.icns (PNG fallback)")

    print("done")


if __name__ == "__main__":
    os.chdir(ROOT)
    main()
