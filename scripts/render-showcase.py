#!/usr/bin/env python3
"""Rasterize the synthetic Ignatius showcase at 2x resolution.

Requires Pillow, the four JetBrains Mono Nerd Font Mono styles and a
monospace fallback font with UI symbols (for example Menlo on macOS).
From the repository root, export the actual UI buffer:

    IGNATIUS_SHOWCASE_CAPTURE_DIR=/tmp/ignatius-showcase \
        cargo test --locked --lib workbench_showcase_scenes_preserve_elevated_chrome_and_overlays
    python3 scripts/render-showcase.py /tmp/ignatius-showcase/showcase.jsonl \
        docs/assets/screenshots --font-dir /path/to/fonts --fallback-font /path/to/Menlo.ttc
"""

import argparse
from functools import lru_cache
import json
from pathlib import Path
import re

from PIL import Image, ImageDraw, ImageFont, PngImagePlugin


SCALE = 2
CELL_WIDTH, CELL_HEIGHT = 10 * SCALE, 21 * SCALE
MARGIN, TOP = 22 * SCALE, 44 * SCALE
BASELINE = 16 * SCALE
FRAME = (19, 23, 32)
DEFAULT_FG = (226, 232, 240)
DEFAULT_BG = (11, 14, 20)
RGB = re.compile(r"Rgb\((\d+), (\d+), (\d+)\)")
FONT_SIZE = 16 * SCALE + 1

# Ratatui's exported Modifier bits.
BOLD, DIM, ITALIC, UNDERLINED = 1, 2, 4, 8
REVERSED, HIDDEN, CROSSED_OUT = 64, 128, 256


def color(value, default):
    if value == "Reset":
        return default
    if match := RGB.fullmatch(value):
        return tuple(map(int, match.groups()))
    raise ValueError(f"Expected a true-color capture, found {value!r}")


def glyph_masks(fonts, fallback):
    missing = {font: bytes(font.getmask("\U0010ffff")) for font in fonts.values()}

    @lru_cache(maxsize=2048)
    def mask(symbol, bold, italic):
        font = fonts[bold, italic]
        if any(bytes(font.getmask(char)) == missing[font] for char in symbol):
            font = fallback
        width = max(CELL_WIDTH, round(font.getlength(symbol)))
        tile = Image.new("L", (width, CELL_HEIGHT))
        ImageDraw.Draw(tile).text(
            (0, BASELINE), symbol, font=font, fill=255, anchor="ls"
        )
        return tile

    return mask


def draw_buffer(image, scene, mask):
    draw = ImageDraw.Draw(image)
    width, height = scene["width"], scene["height"]
    expected_index = 0
    spans = []
    for run in scene["runs"]:
        col, row, symbols = run["x"], run["y"], run["symbols"]
        if (
            row * width + col != expected_index
            or not 0 <= row < height
            or not 0 <= col < width
        ):
            raise ValueError(f"{scene['scene']}: gap or overlap at ({col}, {row})")
        if not symbols or col + len(symbols) > width:
            raise ValueError(f"{scene['scene']}: invalid run at ({col}, {row})")
        expected_index += len(symbols)
        fg = color(run["fg"], DEFAULT_FG)
        bg = color(run["bg"], DEFAULT_BG)
        styles = run["modifiers"]
        if styles & REVERSED:
            fg, bg = bg, fg
        if styles & DIM:
            fg = tuple((front + back) // 2 for front, back in zip(fg, bg))
        x, y = MARGIN + col * CELL_WIDTH, TOP + row * CELL_HEIGHT
        draw.rectangle(
            (x, y, x + len(symbols) * CELL_WIDTH - 1, y + CELL_HEIGHT - 1),
            fill=bg,
        )
        spans.append((x, y, symbols, fg, styles))
    if expected_index != width * height:
        raise ValueError(f"{scene['scene']}: capture does not cover the whole buffer")

    # Draw foreground after all backgrounds so a wide symbol can span cells.
    for x, y, symbols, fg, styles in spans:
        if styles & HIDDEN:
            continue
        for offset, symbol in enumerate(symbols):
            cell_x = x + offset * CELL_WIDTH
            if symbol.strip():
                glyph = mask(symbol, bool(styles & BOLD), bool(styles & ITALIC))
                box = cell_x, y, cell_x + glyph.width, y + glyph.height
                image.paste(fg, box, glyph)
            if styles & UNDERLINED:
                baseline = y + BASELINE + 4
                draw.line(
                    (cell_x, baseline, cell_x + CELL_WIDTH - 1, baseline),
                    fill=fg,
                    width=SCALE,
                )
            if styles & CROSSED_OUT:
                baseline = y + BASELINE - 9
                draw.line(
                    (cell_x, baseline, cell_x + CELL_WIDTH - 1, baseline),
                    fill=fg,
                    width=SCALE,
                )


def render_scene(scene, fonts, mask):
    cols, rows = scene["width"], scene["height"]
    width = cols * CELL_WIDTH + MARGIN * 2
    height = rows * CELL_HEIGHT + TOP + MARGIN
    image = Image.new("RGB", (width, height), FRAME)
    draw = ImageDraw.Draw(image)
    caption = f"{scene['title']}  /  Synthetic TUI fixture"
    dimension = f"{cols}x{rows}  /  2x"
    font = fonts[False, False]
    draw.text((MARGIN, TOP // 2), caption, font=font, fill=DEFAULT_FG, anchor="lm")
    draw.text(
        (width - MARGIN, TOP // 2), dimension, font=font, fill=(153, 165, 187), anchor="rm"
    )
    draw_buffer(image, scene, mask)
    meta = PngImagePlugin.PngInfo()
    meta.add_text("Title", scene["title"])
    meta.add_text("Description", f"{scene['subtitle']}. Synthetic Ratatui buffer at 2x resolution.")
    return image, meta


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("capture", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--font-dir", type=Path, required=True)
    parser.add_argument("--fallback-font", type=Path, required=True)
    args = parser.parse_args()
    fonts = {
        (bold, italic): ImageFont.truetype(
            args.font_dir / f"JetBrainsMonoNerdFontMono-{style}.ttf", FONT_SIZE
        )
        for bold, italic, style in [
            (False, False, "Regular"),
            (True, False, "Bold"),
            (False, True, "Italic"),
            (True, True, "BoldItalic"),
        ]
    }
    mask = glyph_masks(fonts, ImageFont.truetype(args.fallback_font, FONT_SIZE))
    scenes = set()
    args.output.mkdir(parents=True, exist_ok=True)
    with args.capture.open(encoding="utf-8") as capture:
        for line in capture:
            scene = json.loads(line)
            name = scene["scene"]
            if not re.fullmatch(r"[a-z0-9-]+", name) or name in scenes:
                raise ValueError(f"Invalid or duplicate scene name {name!r}")
            scenes.add(name)
            image, meta = render_scene(scene, fonts, mask)
            image.save(args.output / f"{name}.png", optimize=True, pnginfo=meta)


if __name__ == "__main__":
    main()
