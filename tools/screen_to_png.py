#!/usr/bin/env python3
"""Turn a screen dump into a PNG, so the interface can be looked at.

    cargo run -p seek-tui --example dump_screen -- home > /tmp/home.json
    python3 tools/screen_to_png.py /tmp/home.json /tmp/home.png
"""

from __future__ import annotations

import json
import struct
import sys
import zlib

# The size of a cell in the picture.
CELL_WIDTH = 9
CELL_HEIGHT = 19

# The colors of the named colors of the terminal.
NAMED = {
    "Black": "#000000",
    "Red": "#cd0000",
    "Green": "#00cd00",
    "Yellow": "#cdcd00",
    "Blue": "#0000ee",
    "Magenta": "#cd00cd",
    "Cyan": "#00cdcd",
    "White": "#e5e5e5",
    "Gray": "#7f7f7f",
    "DarkGray": "#555555",
    "LightRed": "#ff5f5f",
    "LightGreen": "#5fff5f",
    "LightYellow": "#ffff5f",
    "LightBlue": "#5f5fff",
    "LightMagenta": "#ff5fff",
    "LightCyan": "#5fffff",
    "LightYellow": "#ffff5f",
    # "reset" is the background of the terminal, which is the background of the theme.
    "reset": "#24283b",
    "none": None,
}


def to_rgb(value, fallback):
    """The RGB of a color of the dump."""
    if value is None:
        return fallback
    if value.startswith("#"):
        return tuple(int(value[i : i + 2], 16) for i in (1, 3, 5))
    if value.startswith("i"):
        # The 256 colors of the terminal.
        index = int(value[1:])
        if index < 16:
            return None
        if index < 232:
            index -= 16
            levels = [0, 95, 135, 175, 215, 255]
            return (levels[index // 36], levels[(index // 6) % 6], levels[index % 6])
        grey = 8 + (index - 232) * 10
        return (grey, grey, grey)
    named = NAMED.get(value)
    if named is None:
        return None
    return tuple(int(named[i : i + 2], 16) for i in (1, 3, 5))


def write_png(path, width, height, pixels):
    """Write an RGB PNG."""
    raw = bytearray()
    for row in pixels:
        raw.append(0)
        for pixel in row:
            raw.extend(pixel)

    def chunk(kind, data):
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
        )

    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    with open(path, "wb") as fh:
        fh.write(b"\x89PNG\r\n\x1a\n")
        fh.write(chunk(b"IHDR", header))
        fh.write(chunk(b"IDAT", zlib.compress(bytes(raw), 9)))
        fh.write(chunk(b"IEND", b""))


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__)
        return 1
    dump = json.loads(open(sys.argv[1]).read())
    width = dump["width"]
    height = dump["height"]
    cells = dump["cells"]

    background = (0x24, 0x28, 0x3B)
    pixels = []
    for y in range(height):
        row = []
        for x in range(width):
            cell = cells[y * width + x]
            fg = to_rgb(cell["fg"], (0xB4, 0xB4, 0xB4))
            bg = to_rgb(cell["bg"], background)
            character = cell["c"][0] if cell["c"] else " "
            # Every character fills its cell: this is a layout picture, not a font.
            for _ in range(CELL_WIDTH):
                if character != " ":
                    row.append(fg or bg or background)
                else:
                    row.append(bg or background)
        # The rows of the picture are the lines of the text.
        for _ in range(CELL_HEIGHT):
            pixels.append(row)

    write_png(sys.argv[2], width * CELL_WIDTH, height * CELL_HEIGHT, pixels)
    print(f"wrote {sys.argv[2]} ({width * CELL_WIDTH} x {height * CELL_HEIGHT})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
