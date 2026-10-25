#!/usr/bin/env python3
"""Derive native app/package icons from the checked-in transparent master.

Requires ImageMagick's `convert`; release builds consume the checked-in outputs
and do not require ImageMagick or image generation services.
"""

from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def main():
    source = ROOT / "assets/branding/meshcraft.png"
    for output, arguments in [
        ("assets/branding/meshcraft-64.png", ["-resize", "64x64"]),
        ("packaging/meshcraft.png", ["-resize", "1024x1024"]),
        ("packaging/windows/meshcraft.ico", ["-define", "icon:auto-resize=256,128,64,48,32,24,16"]),
    ]:
        # Strip PNG timestamps/profiles so repeated conversions have identical bytes.
        subprocess.run(["convert", str(source), "-strip", *arguments, str(ROOT / output)], check=True)
        print(output)


if __name__ == "__main__":
    main()
