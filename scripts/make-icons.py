#!/usr/bin/env python3
"""Regenerates assets/icon.ico (Windows .exe) and assets/icon-512.png (Linux menus)
from assets/icon.png.

Usage:  python3 scripts/make-icons.py      (needs:  pip install pillow)

assets/icon.png should be a square PNG, at least 512x512 (1024x1024 is ideal),
ideally with a transparent background and some padding around the artwork.
The macOS .icns is built from the same PNG by scripts/bundle-macos.sh.
The Linux launcher entry is installed by scripts/install-linux.sh.
"""
from pathlib import Path
from PIL import Image

root = Path(__file__).resolve().parent.parent
src = Image.open(root / "assets" / "icon.png").convert("RGBA")
if src.width != src.height:
    raise SystemExit(f"icon.png must be square, got {src.width}x{src.height}")

sizes = [(s, s) for s in (16, 24, 32, 48, 64, 128, 256)]
src.save(root / "assets" / "icon.ico", format="ICO", sizes=sizes)
print("wrote assets/icon.ico with sizes", [s[0] for s in sizes])

src.resize((512, 512), Image.LANCZOS).save(root / "assets" / "icon-512.png", optimize=True)
print("wrote assets/icon-512.png")
