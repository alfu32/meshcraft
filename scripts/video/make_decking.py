#!/usr/bin/env python3
"""Draw the decking texture used by tutorial 8: four boards with grain, gaps and staggered joints.

Deterministic, so re-running it reproduces docs/tutorials/projects/textures/decking.png exactly.
"""
import math
from pathlib import Path
import random

from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "docs/tutorials/projects/textures/decking.png"
SIZE = 512
BOARDS = 4


def board(rng, width, height):
    base = [rng.randint(150, 175), rng.randint(100, 118), rng.randint(58, 72)]
    image = Image.new("RGB", (width, height))
    pixels = image.load()
    phase = rng.uniform(0, math.tau)
    knots = [(rng.uniform(0, width), rng.uniform(0, height), rng.uniform(8, 16)) for _ in range(2)]
    for x in range(width):
        for y in range(height):
            wave = math.sin(y * 0.09 + phase + 2.5 * math.sin(x * 0.012 + phase))
            grain = 0.82 + 0.12 * wave + 0.06 * math.sin(y * 0.6 + x * 0.02)
            for kx, ky, r in knots:
                d = math.hypot((x - kx) * 0.6, y - ky)
                if d < r * 2.2:
                    grain -= 0.18 * max(0.0, 1 - d / (r * 2.2)) * (0.6 + 0.4 * math.sin(d * 0.9))
            pixels[x, y] = tuple(max(0, min(255, round(c * grain))) for c in base)
    return image


def main():
    rng = random.Random(8)
    image = Image.new("RGB", (SIZE, SIZE), (46, 34, 24))
    height = SIZE // BOARDS
    for row in range(BOARDS):
        # Boards run left to right with one butt joint each, staggered by row.
        joint = (row * 3 + 1) * SIZE // 8 % SIZE
        for start, end in ((0, joint), (joint, SIZE)):
            if end - start < 4:
                continue
            piece = board(rng, end - start - 3, height - 6)
            image.paste(piece, (start + 2, row * height + 3))
    image = image.filter(ImageFilter.SMOOTH)
    draw = ImageDraw.Draw(image)
    for row in range(BOARDS + 1):
        draw.line([(0, row * height), (SIZE, row * height)], fill=(40, 30, 22), width=3)
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    image.save(OUTPUT, optimize=True)
    print(OUTPUT)


if __name__ == "__main__":
    main()
