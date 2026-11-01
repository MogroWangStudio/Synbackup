#!/usr/bin/env python3
"""生成 Synbackup 占位应用图标（纯 Python 标准库，无第三方依赖）。

输出: scripts/app-icon-source.png (1024x1024 RGBA)
图形: 系统蓝圆角方块 + 白色双向同步箭头。
此为占位设计，后续将替换为正式 logo。重新生成: python3 scripts/generate_icon.py
"""

import math
import struct
import zlib

SIZE = 1024
BG = (10, 132, 255)      # 系统蓝
FG = (255, 255, 255)     # 白色图形

# 布局（相对 SIZE 的比例）
MARGIN = 0.045           # 画布内边距
CORNER = 0.2237          # macOS 图标圆角比例
RING_R = 0.255           # 同步圆环半径
RING_W = 0.085           # 圆环描边宽
ARC_DEG = 38.0           # 弧段两端留出的缺口角度
ARROW_LEN = 0.085        # 箭头长度
ARROW_W = 0.105          # 箭头宽度
AA = 1.0                 # 抗锯齿过渡宽度（像素）


def clamp01(x: float) -> float:
    return 0.0 if x < 0.0 else (1.0 if x > 1.0 else x)


def sd_rounded_rect(px: float, py: float, half: float, r: float) -> float:
    qx = abs(px) - (half - r)
    qy = abs(py) - (half - r)
    ax, ay = max(qx, 0.0), max(qy, 0.0)
    return math.hypot(ax, ay) + min(max(qx, qy), 0.0) - r


def _seg_dist(px: float, py: float, x1: float, y1: float, x2: float, y2: float) -> float:
    ex, ey = x2 - x1, y2 - y1
    l2 = ex * ex + ey * ey
    t = 0.0 if l2 == 0 else max(0.0, min(1.0, ((px - x1) * ex + (py - y1) * ey) / l2))
    return math.hypot(px - (x1 + t * ex), py - (y1 + t * ey))


def sd_triangle(px: float, py: float, v: tuple) -> float:
    """凸三角形 SDF（内部为负）。先统一边方向，再按内/外分别求距离。"""
    # 质心处的符号确定"内部为正"的方向
    cx = sum(p[0] for p in v) / 3.0
    cy = sum(p[1] for p in v) / 3.0
    sign = 1.0
    for i in range(3):
        x1, y1 = v[i]
        x2, y2 = v[(i + 1) % 3]
        ex, ey = x2 - x1, y2 - y1
        length = math.hypot(ex, ey)
        if ((cx - x1) * ey - (cy - y1) * ex) / length <= 0:
            sign = -1.0
            break
    dists = []
    for i in range(3):
        x1, y1 = v[i]
        x2, y2 = v[(i + 1) % 3]
        ex, ey = x2 - x1, y2 - y1
        length = math.hypot(ex, ey)
        s = sign * ((px - x1) * ey - (py - y1) * ex) / length
        dists.append(s)
    if all(s > 0 for s in dists):
        return -min(dists)  # 内部
    # 外部：到最近线段的欧氏距离
    return min(
        _seg_dist(px, py, *v[i], *v[(i + 1) % 3]) for i in range(3)
    )


def sd_arc(px: float, py: float, radius: float, width: float, a0: float, a1: float) -> float:
    """圆弧段（环带 ∩ 角度区间）的 SDF，负值在内部。角度单位为度。"""
    r = math.hypot(px, py)
    dr = abs(r - radius) - width / 2.0
    ang = math.degrees(math.atan2(py, px))
    # 把 ang 平移到与 a0 同一周期
    while ang < a0 - 180.0:
        ang += 360.0
    while ang > a0 + 180.0:
        ang -= 360.0
    dist_deg = max(a0 - ang, ang - a1)
    dist_px = math.radians(max(dist_deg, 0.0)) * radius
    return max(dr, dist_px if dist_deg > 0 else -1e9)


def _arrow_verts(
    px: float, py: float, tx: float, ty: float, al: float, aw: float
) -> tuple:
    """箭头三角形顶点：质点在弧线末端 (px,py)，沿单位切向 (tx,ty) 指向前方。"""
    nx, ny = -ty, tx
    apex = (px + tx * al, py + ty * al)
    back_x, back_y = px - tx * al * 0.35, py - ty * al * 0.35
    b1 = (back_x + nx * aw / 2, back_y + ny * aw / 2)
    b2 = (back_x - nx * aw / 2, back_y - ny * aw / 2)
    return (apex, b1, b2)


def glyph_sd(px: float, py: float) -> float:
    R = RING_R * SIZE
    W = RING_W * SIZE
    AL = ARROW_LEN * SIZE
    AW = ARROW_W * SIZE

    top = sd_arc(px, py, R, W, -180.0 + ARC_DEG, -ARC_DEG)
    bottom = sd_arc(px, py, R, W, ARC_DEG, 180.0 - ARC_DEG)

    # 顺时针流动：箭头位于弧线末端，沿切线方向
    rad = math.radians(ARC_DEG)
    # 右侧：上弧末端 θ=-ARC_DEG，切向 (sin θ 的正向旋转)
    ex, ey = R * math.cos(rad), -R * math.sin(rad)
    tx, ty = math.sin(rad), math.cos(rad)
    right = sd_triangle(px, py, _arrow_verts(ex, ey, tx, ty, AL, AW))
    # 左侧：下弧末端 θ=180-ARC_DEG，与右侧中心对称
    left = sd_triangle(px, py, _arrow_verts(-ex, -ey, -tx, -ty, AL, AW))
    return min(top, bottom, right, left)


def main() -> None:
    half = SIZE * (0.5 - MARGIN)
    radius = CORNER * half  # 圆角半径

    cx = cy = SIZE / 2.0
    rows = bytearray(SIZE * SIZE * 4)

    for y in range(SIZE):
        py = y + 0.5 - cy
        row_base = y * SIZE * 4
        for x in range(SIZE):
            px = x + 0.5 - cx

            sd_bg = sd_rounded_rect(px, py, half, radius)
            a_bg = clamp01(0.5 - sd_bg / AA)
            if a_bg <= 0.004:
                continue

            sd_gl = glyph_sd(px, py)
            a_gl = clamp01(0.5 - sd_gl / AA) if abs(px) < SIZE * 0.4 and abs(py) < SIZE * 0.4 else 0.0

            # 白色图形叠加在蓝色底上
            r = BG[0] + (FG[0] - BG[0]) * a_gl
            g = BG[1] + (FG[1] - BG[1]) * a_gl
            b = BG[2] + (FG[2] - BG[2]) * a_gl

            base = row_base + x * 4
            rows[base] = int(r + 0.5)
            rows[base + 1] = int(g + 0.5)
            rows[base + 2] = int(b + 0.5)
            rows[base + 3] = int(a_bg * 255 + 0.5)

    write_png("scripts/app-icon-source.png", SIZE, rows)
    print("OK: scripts/app-icon-source.png")


def write_png(path: str, size: int, rgba: bytearray) -> None:
    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data)) + tag + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    stride = size * 4
    raw = b"".join(b"\x00" + bytes(rgba[y * stride:(y + 1) * stride]) for y in range(size))
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


if __name__ == "__main__":
    main()
