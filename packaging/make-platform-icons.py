#!/usr/bin/env python3
"""Derive Windows (.ico) and Linux (hicolor PNG) icons from the 1024px
master produced by packaging/macos/generate-icon.py."""

import os
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
MASTER = os.path.join(HERE, "macos", "RustLab-1024.png")


def main():
    master = Image.open(MASTER).convert("RGBA")

    # Windows: multi-size .ico
    ico_path = os.path.join(HERE, "windows", "RustLab.ico")
    os.makedirs(os.path.dirname(ico_path), exist_ok=True)
    master.save(
        ico_path,
        format="ICO",
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (256, 256)],
    )
    print(f"wrote {ico_path}")

    # Linux: hicolor theme PNGs
    for size in (128, 256, 512):
        out_dir = os.path.join(HERE, "linux", "icons", str(size))
        os.makedirs(out_dir, exist_ok=True)
        out = os.path.join(out_dir, "rustlab.png")
        master.resize((size, size), Image.LANCZOS).save(out)
        print(f"wrote {out}")


if __name__ == "__main__":
    main()
