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

# How bright a cell has to be to count as lit, as a fraction of the range between the background
# and the brightest ink.
LIT_THRESHOLD = 0.75
# How bright a cell has to be to count as dim, with an opaque background. The pixels that are
# blended into the background stay out of the grid.
DIM_THRESHOLD = 0.5
# With a transparent background there is nothing to blend into, so every tone of the ink counts.
# The tones are measured against the ink itself: the brightest is lit, the rest is dim.
DIM_THRESHOLD_TRANSPARENT = 0.0


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
            elif color == 6:
                row.append((out[off], out[off + 1], out[off + 2], out[off + 3]))
            else:
                row.append((out[off], out[off + 1], out[off + 2]))
        rows.append(row)
    return width, height, rows


# ----------------------------------------------------------------------- geometry


def luminance(rgb) -> float:
    r, g, b = rgb[0], rgb[1], rgb[2]
    return 0.299 * r + 0.587 * g + 0.114 * b


def alpha_of(pixel) -> int:
    """The alpha of a pixel; 255 when the image has no alpha channel."""
    return pixel[3] if len(pixel) > 3 else 255


def is_transparent(pixel) -> bool:
    """Whether a pixel is see-through, so it counts as the background."""
    return alpha_of(pixel) < 128


def math_gcd(a: int, b: int) -> int:
    while b:
        a, b = b, a % b
    return a


def gcd_all(values):
    out = 0
    for value in values:
        out = math_gcd(out, value)
    return out


def coverage_map(levels):
    """How much ink every pixel carries: 1 for lit, 0 for the background."""
    return [[1.0 if level == 2 else (0.5 if level == 1 else 0.0) for level in row] for row in levels]


def integral_image(cover):
    """The summed area table, so that a cell can be measured in constant time."""
    height = len(cover)
    width = len(cover[0]) if height else 0
    table = [[0.0] * (width + 1) for _ in range(height + 1)]
    for y in range(height):
        row_sum = 0.0
        for x in range(width):
            row_sum += cover[y][x]
            table[y + 1][x + 1] = table[y][x + 1] + row_sum
    return table


def cell_mean(table, x, y, width, height):
    """The mean coverage of a rectangle.

    The rectangle may have fractional edges, when the image was scaled down; the integer box
    that covers it is used, divided by the exact area.
    """
    left = int(x // 1)
    top = int(y // 1)
    right = min(int(-(-(x + width) // 1)), len(table[0]) - 1)
    bottom = min(int(-(-(y + height) // 1)), len(table) - 1)
    if bottom <= top or right <= left:
        return 0.0
    total = (
        table[bottom][right]
        - table[top][right]
        - table[bottom][left]
        + table[top][left]
    )
    return total / (width * height)


def otsu_score(values):
    """How well the values split into two classes: inked cells and empty ones.

    A grid that fits the logo well has cells that are either covered or empty, so the
    separation between the two groups is large.
    """
    if len(values) < 4:
        return 0.0
    mean = sum(values) / len(values)
    variance = sum((value - mean) ** 2 for value in values) / len(values)
    if variance <= 0.0:
        return 0.0
    best = 0.0
    for threshold in sorted(set(values)):
        low = [value for value in values if value <= threshold]
        high = [value for value in values if value > threshold]
        if not low or not high:
            continue
        low_mean = sum(low) / len(low)
        high_mean = sum(high) / len(high)
        weight = len(low) * len(high)
        between = weight * (low_mean - high_mean) ** 2
        if between > best:
            best = between
    return best / (len(values) * variance)


def crisp_runs(rows, lit_lum):
    """The runs of the crisp foreground, column by column.

    A stroke of the pixel font is one cell wide, so the runs of the letters are the cells of the
    font; the gaps between them are one cell as well.
    """
    height = len(rows)
    width = len(rows[0]) if height else 0
    profile = []
    for x in range(width):
        count = 0
        for y in range(height):
            pixel = rows[y][x]
            if not is_transparent(pixel) and luminance(pixel) >= lit_lum:
                count += 1
        profile.append(count)

    runs = []
    start = None
    for x, count in enumerate(profile):
        if count > 0 and start is None:
            start = x
        if count == 0 and start is not None:
            runs.append((start, x - 1))
            start = None
    if start is not None:
        runs.append((start, width - 1))
    return runs


def cell_size_from_runs(runs):
    """The cell size, from the pitch of the repeating letters.

    The letters of the pixel font are three cells wide and one cell apart, so the pitch of two
    letters is four cells. The pitch is exact, even when the image was scaled down.
    """
    starts = [run[0] for run in runs]
    pitches = [starts[i + 1] - starts[i] for i in range(len(starts) - 1)]
    if not pitches:
        return None
    # The pitch of the letters is the most common distance between two runs.
    pitch = max(set(pitches), key=pitches.count)
    if pitch <= 0:
        return None
    return pitch / 4.0


def classify(rows, bg_rgb, bg_lum, lit_lum):
    """The level of every pixel: 0 is the background, 1 dim ink, 2 lit ink.

    With an opaque background the ink is what is brighter than the background, so the pixels that
    are blended into the background stay out of the grid. With a transparent background there is
    nothing to blend into: the tones of the ink are measured against the ink itself, so a logo
    with two tones (the letters and the fainter sparkles) keeps them apart.
    """
    if bg_rgb is None:
        ink = [
            luminance(pixel) for row in rows for pixel in row if not is_transparent(pixel)
        ]
        if not ink:
            ink = [lit_lum]
        darkest = min(ink)
        brightest = max(ink)
        span = max(brightest - darkest, 1e-6)
        lit_cut = darkest + LIT_THRESHOLD * span
        dim_cut = darkest + DIM_THRESHOLD_TRANSPARENT * span
    else:
        span = max(lit_lum - bg_lum, 1e-6)
        lit_cut = bg_lum + LIT_THRESHOLD * span
        dim_cut = bg_lum + DIM_THRESHOLD * span

    levels = []
    for row in rows:
        line = []
        for pixel in row:
            if is_transparent(pixel) or (bg_rgb is not None and pixel[:3] == bg_rgb):
                line.append(0)
            else:
                value = luminance(pixel)
                line.append(2 if value >= lit_cut else (1 if value >= dim_cut else 0))
        levels.append(line)
    return levels


def extract_grid(png_path: str):
    width, height, rows = read_png(png_path)
    opaque = [p for row in rows for p in row if not is_transparent(p)]
    counts = collections.Counter(opaque)
    # When the image is see-through, the background is the transparency itself and every opaque
    # pixel is ink. Otherwise the most common opaque color is the background.
    transparent = len(rows) * len(rows[0]) - len(opaque)
    if opaque and transparent > 0.05 * len(rows) * len(rows[0]):
        bg_rgb = None
        bg_lum = 0.0
    else:
        bg_rgb = counts.most_common(1)[0][0]
        bg_lum = luminance(bg_rgb)
    lit_rgb = max(counts.items(), key=lambda kv: luminance(kv[0]))[0]
    lit_lum = luminance(lit_rgb)

    levels = classify(rows, bg_rgb, bg_lum, lit_lum)
    cover = coverage_map(levels)

    # The cell size comes from the pitch of the letters; it is exact, even when the image was
    # scaled down and the edges are anti-aliased.
    runs = crisp_runs(rows, lit_lum)
    cell = cell_size_from_runs(runs)
    if not cell or cell <= 0:
        raise ValueError("the cell size of the logo could not be found")

    xs = [x for y in range(height) for x in range(width) if levels[y][x] > 0]
    ys = [y for y in range(height) for x in range(width) if levels[y][x] > 0]
    if not xs:
        return [], {}, cell, (0, 0), (width, height)

    # The grid of the logo is exactly the bounding box of its ink, so the origin is the top left
    # of the ink and the number of cells follows from the cell size.
    origin_x = float(min(xs))
    origin_y = float(min(ys))
    columns = max(1, round((max(xs) - origin_x + 1) / cell))
    rows_n = max(1, round((max(ys) - origin_y + 1) / cell))

    table = integral_image(cover)

    grid = []
    for row in range(rows_n):
        line = []
        for column in range(columns):
            x = origin_x + column * cell
            y = origin_y + row * cell
            if x >= width or y >= height:
                line.append(0)
                continue
            mean = cell_mean(table, x, y, cell, cell)
            if mean > 0.6:
                line.append(2)
            elif mean > 0.25:
                line.append(1)
            else:
                line.append(0)
        grid.append(line)

    # The grid is trimmed: empty rows and columns at the edge are not part of the logo.
    while grid and all(cell == 0 for cell in grid[0]):
        grid.pop(0)
    while grid and all(cell == 0 for cell in grid[-1]):
        grid.pop()
    if grid:
        width_used = max(
            index for index in range(len(grid[0])) if any(row[index] for row in grid)
        ) + 1
        grid = [row[:width_used] for row in grid]

    colors = {"bg": bg_rgb, "light": lit_rgb}
    return grid, colors, cell, (origin_x, origin_y), (width, height)


# ------------------------------------------------------------------------ output


def write_png(path, width, height, pixels):
    """Write an RGBA PNG. The rows are written without a filter."""
    raw = bytearray()
    for row in pixels:
        raw.append(0)
        for pixel in row:
            if len(pixel) == 4:
                raw.extend(pixel)
            else:
                raw.extend((pixel[0], pixel[1], pixel[2], 255))

    def chunk(kind, data):
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
        )

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    with open(path, "wb") as fh:
        fh.write(b"\x89PNG\r\n\x1a\n")
        fh.write(chunk(b"IHDR", header))
        fh.write(chunk(b"IDAT", zlib.compress(bytes(raw), 9)))
        fh.write(chunk(b"IEND", b""))


def crop_to_ink(source, target):
    """Crop an image to its ink and write it out.

    The padding around a logo is of no use to anybody; the asset is cropped so that the grid of
    the logo is the whole image.
    """
    width, height, rows = read_png(source)
    opaque = [
        (x, y)
        for y in range(height)
        for x in range(width)
        if not is_transparent(rows[y][x])
    ]
    if not opaque:
        raise ValueError("the image has no ink")
    xs = [x for x, _ in opaque]
    ys = [y for _, y in opaque]
    left, top = min(xs), min(ys)
    right, bottom = max(xs), max(ys)

    cropped = []
    for y in range(top, bottom + 1):
        row = []
        for x in range(left, right + 1):
            pixel = rows[y][x]
            row.append(pixel if len(pixel) == 4 else (pixel[0], pixel[1], pixel[2], 255))
        cropped.append(row)
    write_png(target, right - left + 1, bottom - top + 1, cropped)
    return (right - left + 1, bottom - top + 1)


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
    parser.add_argument(
        "--crop",
        metavar="PATH",
        help="crop the image to its ink and write it there, then stop",
    )
    args = parser.parse_args()

    if args.crop:
        size = crop_to_ink(args.png, args.crop)
        print(f"wrote {args.crop} ({size[0]} x {size[1]})", file=sys.stderr)
        return 0

    grid, colors, cell, origin, size = extract_grid(args.png)
    print(
        f"source      : {os.path.relpath(args.png, REPO_ROOT)} ({size[0]}x{size[1]})",
        file=sys.stderr,
    )
    print(f"cell size   : {cell}px, origin {origin}", file=sys.stderr)
    print(f"grid        : {len(grid[0])} x {len(grid)}", file=sys.stderr)
    background = colors.get("bg")
    background_text = (
        "transparent"
        if background is None
        else f"#{background[0]:02X}{background[1]:02X}{background[2]:02X}"
    )
    print(
        f"bg          : {background_text}"
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
