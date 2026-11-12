"""Render the app and tray icons (original artwork; no third-party packages).

    python3 -I tools/make_icons.py

Writes app/icons/:
  icon.png                         512px app icon: a purple tile with a progress bar
  tray-{green,amber,red,grey}.png  44px tray icon: the same bar, tinted by pace

The purple follows kiro.dev's palette (purple-400 to purple-600). No Kiro logo or mascot is used.
"""
import math
import os
import struct
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
ICONS = os.path.join(HERE, "..", "app", "icons")

TILE_TOP = (0x9E, 0x61, 0xFF)  # kiro.dev purple-400
TILE_BOTTOM = (0x87, 0x28, 0xE6)  # kiro.dev purple-600
STATE = {
    "green": (46, 160, 67),
    "amber": (230, 150, 20),
    "red": (214, 48, 49),
    "grey": (140, 146, 156),
}
FILL = 0.62  # how full the bar is drawn


def write_png(path, size, rows):
    raw = bytearray()
    for row in rows:
        raw.append(0)
        for px in row:
            raw.extend(px)

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    with open(path, "wb") as f:
        f.write(
            b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
            + chunk(b"IEND", b"")
        )


def rrect(px, py, left, top, right, bottom, radius):
    """Anti-aliased coverage (0..1) of a rounded rectangle at pixel centre (px, py)."""
    cx, cy = (left + right) / 2, (top + bottom) / 2
    hx, hy = (right - left) / 2, (bottom - top) / 2
    dx = max(abs(px - cx) - (hx - radius), 0.0)
    dy = max(abs(py - cy) - (hy - radius), 0.0)
    return max(0.0, min(1.0, 0.5 - (math.hypot(dx, dy) - radius)))


def lerp(a, b, t):
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


def over(base, colour, alpha):
    return lerp(base, colour, max(0.0, min(1.0, alpha)))


def app_icon(size=512):
    s = size / 512.0
    track = (96 * s, 224 * s, 416 * s, 288 * s)  # left, top, right, bottom
    radius = 32 * s
    fill_right = track[0] + (track[2] - track[0]) * FILL
    rows = []
    for y in range(size):
        row = []
        for x in range(size):
            px, py = x + 0.5, y + 0.5
            tile = rrect(px, py, 0, 0, size, size, 112 * s)
            col = lerp(TILE_TOP, TILE_BOTTOM, py / size)
            col = over(col, (255, 255, 255), 0.32 * rrect(px, py, *track, radius))
            col = over(col, (255, 255, 255), rrect(px, py, track[0], track[1], fill_right, track[3], radius))
            row.append((int(col[0]), int(col[1]), int(col[2]), int(255 * tile)))
        rows.append(row)
    return rows


def tray_icon(colour, size=44):
    track = (3, 15, size - 3, 29)
    radius = 7
    fill_right = track[0] + (track[2] - track[0]) * FILL
    rows = []
    for y in range(size):
        row = []
        for x in range(size):
            px, py = x + 0.5, y + 0.5
            a_track = 0.38 * rrect(px, py, *track, radius)
            a_fill = rrect(px, py, track[0], track[1], fill_right, track[3], radius)
            alpha = max(a_track, a_fill)
            row.append((*colour, int(255 * alpha)))
        rows.append(row)
    return rows


def main():
    write_png(os.path.join(ICONS, "icon.png"), 512, app_icon())
    for name, colour in STATE.items():
        write_png(os.path.join(ICONS, f"tray-{name}.png"), 44, tray_icon(colour))
    print("wrote", sorted(f for f in os.listdir(ICONS) if f.endswith(".png")))


if __name__ == "__main__":
    main()
