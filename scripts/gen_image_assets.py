"""Generate the deterministic PNG assets for examples/images.motion.

Run from the repository root:

    python scripts/gen_image_assets.py

The files are small (well under 100 KB total) and byte-for-byte stable, so
the example and its goldens never depend on an external download.  Only the
Python standard library is used (zlib + struct), which keeps the script
portable.
"""

import os
import struct
import zlib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = os.path.join(ROOT, "examples", "assets")


def write_png(path, width, height, pixels):
    """Write straight-alpha RGBA8 pixels (row-major, 4 bytes each)."""
    raw = bytearray()
    for y in range(height):
        raw.append(0)  # filter: none
        start = y * width * 4
        raw.extend(pixels[start:start + width * 4])

    def chunk(tag, data):
        body = tag + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", header)
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as handle:
        handle.write(png)


def stripes(width=96, height=48):
    pixels = bytearray()
    colors = [
        (255, 90, 110, 255),
        (255, 170, 70, 255),
        (90, 200, 140, 255),
        (80, 150, 255, 255),
        (160, 110, 255, 255),
    ]
    for y in range(height):
        for x in range(width):
            band = (x * len(colors)) // width
            r, g, b, a = colors[band]
            # A light diagonal makes the crop/stretch visible at a glance.
            if (x + y) % 24 < 2:
                r, g, b = min(255, r + 40), min(255, g + 40), min(255, b + 40)
            pixels.extend((r, g, b, a))
    return width, height, pixels


def alpha_badge(width=256, height=256):
    # Drawn larger than it is shown, so the example exercises the mip chain
    # (downscaling) rather than magnification.
    pixels = bytearray()
    cx, cy = (width - 1) / 2, (height - 1) / 2
    radius = width * 0.42
    ring = width / 16
    for y in range(height):
        for x in range(width):
            dx, dy = x - cx, y - cy
            distance = (dx * dx + dy * dy) ** 0.5
            if distance > radius + 1:
                pixels.extend((0, 0, 0, 0))
                continue
            # Soft edge: one pixel of anti-aliasing.
            coverage = max(0.0, min(1.0, radius + 0.5 - distance))
            alpha = int(round(coverage * 255))
            # A cyan disc with a darker ring and a white highlight.
            if distance > radius - ring:
                r, g, b = 20, 120, 160
            elif distance < radius * 0.28:
                r, g, b = 255, 255, 255
            else:
                r, g, b = 51, 242, 230
            pixels.extend((r, g, b, alpha))
    return width, height, pixels


def main():
    os.makedirs(ASSETS, exist_ok=True)
    for name, (width, height, pixels) in [
        ("stripes.png", stripes()),
        ("alpha_badge.png", alpha_badge()),
    ]:
        path = os.path.join(ASSETS, name)
        write_png(path, width, height, pixels)
        print(f"wrote {path} ({os.path.getsize(path)} bytes)")


if __name__ == "__main__":
    main()
