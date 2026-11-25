"""Generates a crisp, high-contrast math problem PNG using only Python stdlib (no Pillow required)."""

from __future__ import annotations

import pathlib
import struct
import sys
import zlib

# Simple 5x7 bitmap font for clear, unambiguous math problem text
FONT_5X7: dict[str, list[str]] = {
    " ": [".....", ".....", ".....", ".....", ".....", ".....", "....."],
    "0": [".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###."],
    "1": ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."],
    "2": [".###.", "#...#", "....#", "..##.", ".#...", "#....", "#####"],
    "3": [".###.", "#...#", "....#", "..##.", "....#", "#...#", ".###."],
    "4": ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#."],
    "5": ["#####", "#....", "####.", "....#", "....#", "#...#", ".###."],
    "6": [".###.", "#....", "####.", "#...#", "#...#", "#...#", ".###."],
    "7": ["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#..."],
    "8": [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###."],
    "9": [".###.", "#...#", "#...#", ".####", "....#", "...#.", ".##.."],
    "A": [".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"],
    "D": ["####.", "#...#", "#...#", "#...#", "#...#", "#...#", "####."],
    "E": ["#####", "#....", "#....", "####.", "#....", "#....", "#####"],
    "F": ["#####", "#....", "#....", "####.", "#....", "#....", "#...."],
    "I": [".###.", "..#..", "..#..", "..#..", "..#..", "..#..", ".###."],
    "L": ["#....", "#....", "#....", "#....", "#....", "#....", "#####"],
    "N": ["#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#", "#...#"],
    "O": [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."],
    "S": [".####", "#....", "#....", ".###.", "....#", "....#", "####."],
    "V": ["#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#.."],
    "X": ["#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#"],
    "+": [".....", "..#..", "..#..", "#####", "..#..", "..#..", "....."],
    "-": [".....", ".....", ".....", "#####", ".....", ".....", "....."],
    "=": [".....", ".....", "#####", ".....", "#####", ".....", "....."],
    ":": [".....", "..#..", ".....", ".....", ".....", "..#..", "....."],
    "?": [".###.", "#...#", "....#", "..##.", "..#..", ".....", "..#.."],
    "(": ["...#.", "..#..", ".#...", ".#...", ".#...", "..#..", "...#."],
    ")": [".#...", "..#..", "...#.", "...#.", "...#.", "..#..", ".#..."],
    "^": ["..#..", ".#.#.", "#...#", ".....", ".....", ".....", "....."],
    "/": ["....#", "...#.", "...#.", "..#..", ".#...", ".#...", "#...."],
}


def make_chunk(chunk_type: bytes, data: bytes) -> bytes:
    crc = zlib.crc32(chunk_type + data) & 0xFFFFFFFF
    return struct.pack(">I", len(data)) + chunk_type + data + struct.pack(">I", crc)


def render_text_png(lines: list[str], out_path: pathlib.Path, scale: int = 5) -> None:
    padding = 24
    char_w = 6 * scale
    line_h = 10 * scale
    max_chars = max(len(line) for line in lines)
    width = padding * 2 + max_chars * char_w
    height = padding * 2 + len(lines) * line_h

    # RGB white background
    pixels = bytearray([255] * (width * height * 3))

    def fill_rect(x0: int, y0: int, w: int, h: int, r: int, g: int, b: int) -> None:
        for yy in range(max(0, y0), min(height, y0 + h)):
            row_off = yy * width * 3
            for xx in range(max(0, x0), min(width, x0 + w)):
                idx = row_off + xx * 3
                pixels[idx] = r
                pixels[idx + 1] = g
                pixels[idx + 2] = b

    # Subtle border
    fill_rect(0, 0, width, 2, 30, 41, 59)
    fill_rect(0, height - 2, width, 2, 30, 41, 59)
    fill_rect(0, 0, 2, height, 30, 41, 59)
    fill_rect(width - 2, 0, 2, height, 30, 41, 59)

    for li, line in enumerate(lines):
        y_base = padding + li * line_h
        for ci, ch in enumerate(line.upper()):
            glyph = FONT_5X7.get(ch, FONT_5X7[" "])
            x_base = padding + ci * char_w
            for gy, row in enumerate(glyph):
                for gx, px in enumerate(row):
                    if px == "#":
                        fill_rect(
                            x_base + gx * scale,
                            y_base + gy * scale,
                            scale,
                            scale,
                            15,
                            23,
                            42,
                        )

    raw_rows = bytearray()
    stride = width * 3
    for y in range(height):
        raw_rows.append(0)  # filter type 0
        raw_rows.extend(pixels[y * stride : (y + 1) * stride])

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    png_data = (
        b"\x89PNG\r\n\x1a\n"
        + make_chunk(b"IHDR", ihdr)
        + make_chunk(b"IDAT", zlib.compress(bytes(raw_rows), level=6))
        + make_chunk(b"IEND", b"")
    )
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_bytes(png_data)


if __name__ == "__main__":
    target = (
        pathlib.Path(sys.argv[1])
        if len(sys.argv) > 1
        else pathlib.Path(__file__).parent / "test_problem.png"
    )
    render_text_png(["SOLVE: 2X + 3 = 11", "FIND X = ?"], target)
    print(f"Generated {target} ({target.stat().st_size} bytes)")
