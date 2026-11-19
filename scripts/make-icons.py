#!/usr/bin/env python3
"""draw the launcher icon: icons/saillight.svg, the reference, and icons/<size>/saillight.png
for each size, from the same description. png files are drawn with Pillow because the build
environments have no svg renderer.
    python3 scripts/make-icons.py

the icon is on one of the base shapes of the Sailfish app icon guide (the fully rounded one, a
circle), greyscale as the one of sailtree: 86 units wide, a 0.3 gap on each side
"""
import math
import os
from PIL import Image, ImageDraw

SIZES = [86, 108, 128, 172]
# drawn bigger then reduced, for smooth edges
OVERSAMPLING = 8
BOTTOM_LEFT, TOP_RIGHT = (0x2e, 0x2e, 0x2e), (0x8a, 0x8a, 0x8a)
CENTRE = 43
# gap between two sectors, in icon units
GAP = 1.6
# a ring chart: (inner radius, outer radius, opacity, [(start, end) in turns from the top, clockwise])
RINGS = [
    (10, 21, 1.0, [(0, 0.42), (0.42, 0.68), (0.68, 0.86), (0.86, 0.96)]),
    (23, 34, 0.7, [(0, 0.22), (0.22, 0.36), (0.42, 0.60), (0.68, 0.78)]),
]


def sectors():
    """(inner, outer, start, end, opacity) of each sector, the gap taken from its sides"""
    for inner, outer, opacity, spans in RINGS:
        trim = GAP / 2 / (math.pi * (inner + outer))
        for start, end in spans:
            yield inner, outer, start + trim, end - trim, opacity


def point(radius, turn):
    angle = turn * 2 * math.pi
    return CENTRE + radius * math.sin(angle), CENTRE - radius * math.cos(angle)


def svg():
    lines = [
        '<?xml version="1.0" encoding="utf-8"?>',
        "<!-- launcher icon, written by scripts/make-icons.py with the png files of each size: on the",
        "     fully rounded base shape of the Sailfish app icon guide, greyscale -->",
        '<svg xmlns="http://www.w3.org/2000/svg" width="86" height="86" viewBox="0 0 86 86">',
        "  <defs>",
        '    <linearGradient id="background" gradientUnits="userSpaceOnUse" x1="8" y1="78" x2="78" y2="8">',
        '      <stop offset="0" stop-color="#%02x%02x%02x"/>' % BOTTOM_LEFT,
        '      <stop offset="1" stop-color="#%02x%02x%02x"/>' % TOP_RIGHT,
        "    </linearGradient>",
        "  </defs>",
        '  <circle fill="url(#background)" cx="43" cy="43" r="42.7"/>',
        "  <!-- a ring chart: two levels of folders -->",
    ]
    for inner, outer, start, end, opacity in sectors():
        large = 1 if end - start > 0.5 else 0
        a, b, c, d = point(outer, start), point(outer, end), point(inner, end), point(inner, start)
        lines.append(
            '  <path fill="#ffffff" fill-opacity="%g" d="M%.2f %.2f A%g %g 0 %d 1 %.2f %.2f L%.2f %.2f A%g %g 0 %d 0 %.2f %.2f Z"/>'
            % (opacity, a[0], a[1], outer, outer, large, b[0], b[1], c[0], c[1], inner, inner, large, d[0], d[1])
        )
    lines.append("</svg>")
    return "\n".join(lines) + "\n"


def icon(size):
    scale = size * OVERSAMPLING / 86
    side = size * OVERSAMPLING

    # background: gradient from the bottom left to the top right
    gradient = Image.new("RGB", (side, side))
    pixels = gradient.load()
    for y in range(side):
        for x in range(side):
            # position along the diagonal from (8, 78) to (78, 8), in icon units
            t = ((x / scale - 8) + (78 - y / scale)) / 140
            t = min(1, max(0, t))
            pixels[x, y] = tuple(round(a + (b - a) * t) for a, b in zip(BOTTOM_LEFT, TOP_RIGHT))

    mask = Image.new("L", (side, side), 0)
    ImageDraw.Draw(mask).ellipse([value * scale for value in (0.3, 0.3, 85.7, 85.7)], fill=255)
    result = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    result.paste(gradient, (0, 0), mask)

    def disc(radius):
        return [(CENTRE - radius) * scale, (CENTRE - radius) * scale,
                (CENTRE + radius) * scale, (CENTRE + radius) * scale]

    for inner, outer, start, end, opacity in sectors():
        # a slice of the outer disc, without the inner one
        shape = Image.new("L", (side, side), 0)
        pen = ImageDraw.Draw(shape)
        pen.pieslice(disc(outer), start * 360 - 90, end * 360 - 90, fill=round(255 * opacity))
        pen.ellipse(disc(inner), fill=0)
        white = Image.new("RGBA", (side, side), (255, 255, 255, 255))
        white.putalpha(shape)
        result = Image.alpha_composite(result, white)
    return result.resize((size, size), Image.LANCZOS)


def main():
    root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
    with open(os.path.join(root, "icons", "saillight.svg"), "w") as file:
        file.write(svg())
    print("icons/saillight.svg")
    for size in SIZES:
        folder = os.path.join(root, "icons", f"{size}x{size}")
        os.makedirs(folder, exist_ok=True)
        icon(size).save(os.path.join(folder, "saillight.png"))
        print(f"icons/{size}x{size}/saillight.png")


if __name__ == "__main__":
    main()
