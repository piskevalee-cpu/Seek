#!/usr/bin/env python3
"""Extract the SEEK logo bitmap from assets/logo.png into a Rust constant.

SPEC 12 requires the grid to be extracted from the PNG once, with a script, and committed as a
constant bitmap in `seek-tui`. The terminal never scales the PNG at runtime.

The script is deliberately dependency-free: it decodes the PNG with `zlib` and `struct` only.

Usage:
    python3 tools/extract_logo.py                 # print the Rust constant
    python3 tools/extract_logo.py --out crates/seek-tui/src/logo.rs
    python3 tools/extract_logo.py --preview        # print the grid as ASCII

The grid is discovered, not hardcoded:
  * the cell size is the GCD of every lit run length (the strokes are pixel-font cells),
  * the grid origin is the bounding box of the non-background pixels,
  * each cell is classified by luminance into three levels: dark, dim (anti-aliased), light.
"""

from __future__ import annotations

import argparse
import collections
import os
import struct
import sys
import zlib

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_PNG = os.path.join(REPO_ROOT, "assets", "logo.png")
DEFAULT_OUT = os.path.join(REPO_ROOT, "crates", "seek-tui", "src", "logo_bitmap.rs")

# Fraction of the background->lightness range that a cell must reach to count as lit / dim.
LIT_THRESHOLD = 0.75
DIM_THRESHOLD = 0.50


# --------------------------------------------------------------------------- PNG


def read_png(path: str):
    """Return (width, height, rows) for an 8-bit non-interlaced PNG."""
    with open(path, "rb") as fh:
        data = fh.read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"{path} is not a PNG file")

    pos = 8
    idat = bytearray()
    width = height = depth = color = None
    palette = None
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        ctype = data[pos + 4 : pos + 8]
        chunk = data[pos + 8 : pos + 8 + length]
        pos += 12 + length
        if ctype == b"IHDR":
            width, height, depth, color, _c, _f, interlace = struct.unpack(">IIBBBBB", chunk)
            if depth != 8 or interlace != 0:
                raise ValueError(f"unsupported PNG: depth={depth}, interlace={interlace}")
        elif ctype == b"PLTE":
            palette = chunk
        elif ctype == b"IDAT":
            idat += chunk
        elif ctype == b"IEND":
            break

    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[color]
    stride = width * channels
    raw = zlib.decompress(bytes(idat))
    out = bytearray(height * stride)
    prev = bytearray(stride)
    pos = 0
    for row in range(height):
        ftype = raw[pos]
        pos += 1
        line = bytearray(raw[pos : pos + stride])
        pos += stride
        if ftype == 1:  # Sub
            for i in range(channels, stride):
                line[i] = (line[i] + line[i - channels]) & 0xFF
        elif ftype == 2:  # Up
            for i in range(stride):
                line[i] = (line[i] + prev[i]) & 0xFF
        elif ftype == 3:  # Average
            for i in range(stride):
                left = line[i - channels] if i >= channels else 0
                line[i] = (line[i] + ((left + prev[i]) >> 1)) & 0xFF
        elif ftype == 4:  # Paeth
            for i in range(stride):
                a = line[i - channels] if i >= channels else 0
                b = prev[i]
                c = prev[i - channels] if i >= channels else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + pr) & 0xFF
        out[row * stride : (row + 1) * stride] = line
        prev = line

    rows = []
    for y in range(height):
        row = []
        base = y * stride
        for x in range(width):
            off = base + x * channels
            if color == 3:
                idx = out[off]
                row.append((palette[idx * 3], palette[idx * 3 + 1], palette[idx * 3 + 2]))
            elif color in (0, 4):
                g = out[off]
                row.append((g, g, g))
            else:
                row.append((out[off], out[off + 1], out[off + 2]))
        rows.append(row)
    return width, height, rows


# ----------------------------------------------------------------------- geometry


def luminance(rgb) -> float:
    r, g, b = rgb
    return 0.299 * r + 0.587 * g + 0.114 * b


def gcd_all(values):
    out = 0
    for value in values:
        out = math_gcd(out, value)
    return out


def math_gcd(a: int, b: int) -> int:
    while b:
        a, b = b, a % b
    return a


def classify(rows, bg_lum, lit_lum):
    span = max(lit_lum - bg_lum, 1e-6)
    lit_cut = bg_lum + LIT_THRESHOLD * span
    dim_cut = bg_lum + DIM_THRESHOLD * span
    return [
        [2 if luminance(p) >= lit_cut else (1 if luminance(p) >= dim_cut else 0) for p in row]
        for row in rows
    ]


def lit_runs(flags):
    runs = []
    current = 0
    for value in flags:
        if value:
            current += 1
        elif current:
            runs.append(current)
            current = 0
    if current:
        runs.append(current)
    return runs


def extract_grid(png_path: str):
    width, height, rows = read_png(png_path)
    counts = collections.Counter(p for row in rows for p in row)
    bg_rgb = counts.most_common(1)[0][0]
    bg_lum = luminance(bg_rgb)
    lit_lum = max(luminance(p) for p in counts)

    levels = classify(rows, bg_lum, lit_lum)

    # Cell size: GCD of every lit run length in both axes.
    lengths = []
    for y in range(height):
        lengths += lit_runs([levels[y][x] > 0 for x in range(width)])
    for x in range(width):
        lengths += lit_runs([levels[y][x] > 0 for y in range(height)])
    cell = gcd_all(lengths)
    if cell <= 0:
        raise ValueError("no lit pixels found")

    # Grid origin: bounding box of the non-background pixels.
    xs = [x for y in range(height) for x in range(width) if levels[y][x] > 0]
    ys = [y for y in range(height) for x in range(width) if levels[y][x] > 0]
    ox, oy = min(xs), min(ys)
    cols = (max(xs) - ox) // cell + 1
    rows_n = (max(ys) - oy) // cell + 1

    grid = []
    for r in range(rows_n):
        row = []
        for c in range(cols):
            # Sample the cell centre; fall back to the brightest pixel in the cell.
            x = min(ox + c * cell + cell // 2, width - 1)
            y = min(oy + r * cell + cell // 2, height - 1)
            level = levels[y][x]
            if level == 0:
                level = max(
                    levels[oy + r * cell + dy][ox + c * cell + dx]
                    for dy in range(min(cell, height - oy - r * cell))
                    for dx in range(min(cell, width - ox - c * cell))
                )
            row.append(level)
        grid.append(row)

    colors = {
        "bg": bg_rgb,
        "light": max(counts.items(), key=lambda kv: luminance(kv[0]))[0],
    }
    return grid, colors, cell, (ox, oy), (width, height)


# ------------------------------------------------------------------------ output


def rust_constant(grid) -> str:
    rows_n = len(grid)
    cols = len(grid[0])
    lines = []
    for row in grid:
        cells = "".join({0: " ", 1: ".", 2: "#"}[v] for v in row)
        lines.append(f'    "{cells}",')
    body = "\n".join(lines)
    return (
        "// @generated by tools/extract_logo.py from assets/logo.png -- do not edit by hand.\n"
        "//\n"
        f"// Logo grid: {cols} x {rows_n} cells. Each cell is one logo pixel.\n"
        "//\n"
        "// Levels: ` ` dark, `.` dim (anti-aliased), `#` lit.\n"
        "\n"
        "/// Logo width in cells.\n"
        f"pub const LOGO_COLS: usize = {cols};\n"
        "/// Logo height in cells.\n"
        f"pub const LOGO_ROWS: usize = {rows_n};\n"
        "\n"
        "/// The logo bitmap, one string per row, top to bottom.\n"
        "pub const LOGO: [&str; LOGO_ROWS] = [\n"
        f"{body}\n"
        "];\n"
    )


def preview(grid) -> None:
    for row in grid:
        print("  " + "".join({0: " ", 1: ".", 2: "#"}[v] for v in row))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--png", default=DEFAULT_PNG)
    parser.add_argument("--out", default=None, help="write the Rust constant to this path")
    parser.add_argument("--preview", action="store_true", help="print the grid as ASCII")
    args = parser.parse_args()

    grid, colors, cell, origin, size = extract_grid(args.png)
    print(
        f"source      : {os.path.relpath(args.png, REPO_ROOT)} ({size[0]}x{size[1]})",
        file=sys.stderr,
    )
    print(f"cell size   : {cell}px, origin {origin}", file=sys.stderr)
    print(f"grid        : {len(grid[0])} x {len(grid)}", file=sys.stderr)
    print(
        f"bg          : #{colors['bg'][0]:02X}{colors['bg'][1]:02X}{colors['bg'][2]:02X}"
        f"  light: #{colors['light'][0]:02X}{colors['light'][1]:02X}{colors['light'][2]:02X}",
        file=sys.stderr,
    )

    if args.preview:
        preview(grid)

    const = rust_constant(grid)
    if args.out:
        os.makedirs(os.path.dirname(args.out), exist_ok=True)
        with open(args.out, "w") as fh:
            fh.write(const)
        print(f"wrote {os.path.relpath(args.out, REPO_ROOT)}", file=sys.stderr)
    else:
        print(const)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
