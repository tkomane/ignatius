#!/usr/bin/env python3
"""Rasterize the synthetic Ignatius showcase as 2x Retina studio captures.

Uses Pillow with CommitMono Nerd Font Mono (or JetBrains Mono Nerd Font
fallback), subpixel vector box-drawing, luminous DIM blending, and Apple/
Anthropic/Google-inspired editorial macOS window chrome over theme-matched
ambient studio mats.

From the repository root, export the actual UI buffer:

    IGNATIUS_SHOWCASE_CAPTURE_DIR=/private/tmp/ignatius-showcase \
        cargo test --locked --lib workbench_showcase_scenes_preserve_elevated_chrome_and_overlays
    python3 scripts/render-showcase.py /private/tmp/ignatius-showcase/showcase.jsonl \
        docs/assets/screenshots --font-dir "$HOME/Library/Fonts" --fallback-font /System/Library/Fonts/Menlo.ttc
"""

import argparse
from functools import lru_cache
import json
import math
from pathlib import Path
import re

from PIL import Image, ImageDraw, ImageFilter, ImageFont, PngImagePlugin


SCALE = 2
CELL_WIDTH, CELL_HEIGHT = 10 * SCALE, 24 * SCALE
FONT_SIZE = 16 * SCALE + 1
BASELINE = 18 * SCALE

# Outer studio mat and floating macOS window geometry.
MAT_PAD_X, MAT_PAD_TOP, MAT_PAD_BOTTOM = 44 * SCALE, 34 * SCALE, 48 * SCALE
TITLEBAR_H = 38 * SCALE
WIN_PAD_X, WIN_PAD_Y = 16 * SCALE, 10 * SCALE
WINDOW_RADIUS = 13 * SCALE

DEFAULT_FG = (242, 236, 225)
DEFAULT_BG = (20, 18, 16)
RGB = re.compile(r"Rgb\((\d+), (\d+), (\d+)\)")

# Ratatui's exported Modifier bits.
BOLD, DIM, ITALIC, UNDERLINED = 1, 2, 4, 8
REVERSED, HIDDEN, CROSSED_OUT = 64, 128, 256

# Exact vector stroke definitions: (up, right, down, left, stroke_scale, rounded)
BOX_GLYPHS = {
    "\u2500": (0, 1, 0, 1, 1, False),  # ─
    "\u2502": (1, 0, 1, 0, 1, False),  # │
    "\u250c": (0, 1, 1, 0, 1, False),  # ┌
    "\u2510": (0, 0, 1, 1, 1, False),  # ┐
    "\u2514": (1, 1, 0, 0, 1, False),  # └
    "\u2518": (1, 0, 0, 1, 1, False),  # ┘
    "\u251c": (1, 1, 1, 0, 1, False),  # ├
    "\u2524": (1, 0, 1, 1, 1, False),  # ┤
    "\u252c": (0, 1, 1, 1, 1, False),  # ┬
    "\u2534": (1, 1, 0, 1, 1, False),  # ┴
    "\u253c": (1, 1, 1, 1, 1, False),  # ┼
    "\u256d": (0, 1, 1, 0, 1, True),   # ╭
    "\u256e": (0, 0, 1, 1, 1, True),   # ╮
    "\u2570": (1, 1, 0, 0, 1, True),   # ╰
    "\u256f": (1, 0, 0, 1, 1, True),   # ╯
    "\u2501": (0, 1, 0, 1, 2, False),  # ━
    "\u2503": (1, 0, 1, 0, 2, False),  # ┃
    "\u250f": (0, 1, 1, 0, 2, False),  # ┏
    "\u2513": (0, 0, 1, 1, 2, False),  # ┓
    "\u2517": (1, 1, 0, 0, 2, False),  # ┗
    "\u251b": (1, 0, 0, 1, 2, False),  # ┛
    "\u2523": (1, 1, 1, 0, 2, False),  # ┣
    "\u252b": (1, 0, 1, 1, 2, False),  # ┫
    "\u2533": (0, 1, 1, 1, 2, False),  # ┳
    "\u253b": (1, 1, 0, 1, 2, False),  # ┻
    "\u254b": (1, 1, 1, 1, 2, False),  # ╋
}

BLOCK_BARS = {
    "\u258f": 1 / 8,  # ▏
    "\u258e": 1 / 4,  # ▎
    "\u258d": 3 / 8,  # ▍
    "\u258c": 1 / 2,  # ▌
    "\u2588": 1.0,    # █
}

LOWER_BLOCKS = {
    chr(0x2581 + i): (i + 1) / 8 for i in range(7)  # ▁..▇
}


def color(value, default):
    if value == "Reset":
        return default
    if match := RGB.fullmatch(value):
        return tuple(map(int, match.groups()))
    raise ValueError(f"Expected a true-color capture, found {value!r}")


def luminance(rgb):
    def lin(ch):
        s = ch / 255.0
        return s / 12.92 if s <= 0.03928 else ((s + 0.055) / 1.055) ** 2.4

    return 0.2126 * lin(rgb[0]) + 0.7152 * lin(rgb[1]) + 0.0722 * lin(rgb[2])


def lerp_rgb(a, b, t):
    return tuple(round(a[i] + (b[i] - a[i]) * t) for i in range(3))


def is_pua_or_icon(ch):
    cp = ord(ch)
    return (0xE000 <= cp <= 0xF8FF) or cp >= 0xF0000


def resolve_mono_font(font_dir: Path, family: str | None, style: str) -> Path:
    candidates = []
    if family:
        candidates.append(family)
    candidates.extend([
        "CommitMonoNerdFontMono",
        "GeistMonoNerdFontMono",
        "BlexMonoNerdFontMono",
        "JetBrainsMonoNerdFontMono",
        "JetBrainsMonoNLNerdFont",
    ])
    for fam in candidates:
        for ext in ("otf", "ttf"):
            path = font_dir / f"{fam}-{style}.{ext}"
            if path.exists():
                return path
    return font_dir / f"{candidates[0]}-{style}.ttf"


def load_chrome_fonts(regular_mono: ImageFont.FreeTypeFont):
    brand_candidates = [
        Path("/System/Library/Fonts/Supplemental/NewYorkItalic.ttf"),
        Path("/System/Library/Fonts/Supplemental/Palatino.ttc"),
        Path("/System/Library/Fonts/Supplemental/Baskerville.ttc"),
    ]
    ui_candidates = [
        Path("/System/Library/Fonts/SFNS.ttf"),
        Path("/System/Library/Fonts/HelveticaNeue.ttc"),
    ]
    brand_font = next(
        (ImageFont.truetype(p, 16 * SCALE) for p in brand_candidates if p.exists()),
        regular_mono,
    )
    ui_font = next(
        (ImageFont.truetype(p, 14 * SCALE) for p in ui_candidates if p.exists()),
        regular_mono,
    )
    return brand_font, ui_font


def glyph_masks(fonts, fallback):
    missing = {font: bytes(font.getmask("\U0010ffff")) for font in fonts.values()}

    @lru_cache(maxsize=4096)
    def mask(symbol, bold, italic):
        font = fonts[bold, italic]
        if any(bytes(font.getmask(char)) == missing[font] for char in symbol):
            font = fallback

        if len(symbol) == 1 and is_pua_or_icon(symbol):
            # Center Nerd Font Codicons / Devicons cleanly in the 1-cell slot.
            raw = font.getmask(symbol, mode="L")
            bbox = font.getbbox(symbol, anchor="ls")
            tile = Image.new("L", (CELL_WIDTH, CELL_HEIGHT))
            if bbox is not None and raw.size[0] > 0 and raw.size[1] > 0:
                gw = bbox[2] - bbox[0]
                gh = bbox[3] - bbox[1]
                max_w = CELL_WIDTH - 2
                max_h = CELL_HEIGHT - 8
                if gw > max_w or gh > max_h:
                    factor = min(max_w / gw, max_h / gh)
                    nw, nh = max(1, round(raw.size[0] * factor)), max(1, round(raw.size[1] * factor))
                    glyph_img = Image.new("L", raw.size)
                    glyph_img.putdata(list(raw))
                    glyph_img = glyph_img.resize((nw, nh), Image.Resampling.LANCZOS)
                else:
                    nw, nh = raw.size
                    glyph_img = Image.new("L", raw.size)
                    glyph_img.putdata(list(raw))
                dx = (CELL_WIDTH - nw) // 2
                dy = (CELL_HEIGHT - nh) // 2
                tile.paste(glyph_img, (dx, dy))
            return tile, 0

        adv = round(font.getlength(symbol))
        tile_w = max(CELL_WIDTH, adv + 2)
        tile = Image.new("L", (tile_w, CELL_HEIGHT))
        draw_x = max(0, (CELL_WIDTH - adv) // 2) if adv <= CELL_WIDTH else 0
        ImageDraw.Draw(tile).text(
            (draw_x, BASELINE), symbol, font=font, fill=255, anchor="ls"
        )
        return tile, 0

    return mask


def draw_vector_box(draw, ch, cx, cy, fg):
    up, right, down, left, scale_mul, rounded = BOX_GLYPHS[ch]
    stroke = max(2, scale_mul * SCALE)
    mx = cx + CELL_WIDTH // 2 - (stroke // 2)
    my = cy + CELL_HEIGHT // 2 - (stroke // 2)
    x0, x1 = cx, cx + CELL_WIDTH - 1
    y0, y1 = cy, cy + CELL_HEIGHT - 1

    if rounded:
        r = min(CELL_WIDTH // 2, CELL_HEIGHT // 2) - 1
        cx_c = cx + CELL_WIDTH // 2
        cy_c = cy + CELL_HEIGHT // 2
        if ch == "\u256d":  # ╭ (right + down)
            draw.line((cx_c, cy_c + r, cx_c, y1), fill=fg, width=stroke)
            draw.line((cx_c + r, cy_c, x1, cy_c), fill=fg, width=stroke)
            draw.arc((cx_c, cy_c, cx_c + 2 * r, cy_c + 2 * r), 180, 270, fill=fg, width=stroke)
        elif ch == "\u256e":  # ╮ (left + down)
            draw.line((cx_c, cy_c + r, cx_c, y1), fill=fg, width=stroke)
            draw.line((x0, cy_c, cx_c - r, cy_c), fill=fg, width=stroke)
            draw.arc((cx_c - 2 * r, cy_c, cx_c, cy_c + 2 * r), 270, 360, fill=fg, width=stroke)
        elif ch == "\u2570":  # ╰ (right + up)
            draw.line((cx_c, y0, cx_c, cy_c - r), fill=fg, width=stroke)
            draw.line((cx_c + r, cy_c, x1, cy_c), fill=fg, width=stroke)
            draw.arc((cx_c, cy_c - 2 * r, cx_c + 2 * r, cy_c), 90, 180, fill=fg, width=stroke)
        elif ch == "\u256f":  # ╯ (left + up)
            draw.line((cx_c, y0, cx_c, cy_c - r), fill=fg, width=stroke)
            draw.line((x0, cy_c, cx_c - r, cy_c), fill=fg, width=stroke)
            draw.arc((cx_c - 2 * r, cy_c - 2 * r, cx_c, cy_c), 0, 90, fill=fg, width=stroke)
        return

    cx_c = cx + CELL_WIDTH // 2
    cy_c = cy + CELL_HEIGHT // 2
    if up:
        draw.line((cx_c, y0, cx_c, cy_c), fill=fg, width=stroke)
    if down:
        draw.line((cx_c, cy_c, cx_c, y1), fill=fg, width=stroke)
    if left:
        draw.line((x0, cy_c, cx_c, cy_c), fill=fg, width=stroke)
    if right:
        draw.line((cx_c, cy_c, x1, cy_c), fill=fg, width=stroke)


def scene_surface_bg(scene):
    # Prefer row 1 col 0 (below header bar) to detect true base Surface token.
    for run in scene["runs"]:
        if run["y"] == 1 and run["x"] == 0:
            return color(run["bg"], DEFAULT_BG)
    return color(scene["runs"][-1]["bg"], DEFAULT_BG)


def studio_palette(scene_name: str, surface_bg):
    is_light = luminance(surface_bg) > 0.45
    if "paper" in scene_name:
        return {
            "light": True,
            "mat_tl": (214, 203, 186),
            "mat_br": (235, 227, 212),
            "mat_glow": (251, 247, 238),
            "glow_pos": (0.76, 0.14),
            "title_bg": (228, 220, 205),
            "title_fg": (30, 27, 24),
            "title_muted": (104, 94, 83),
            "title_accent": (154, 49, 21),
            "win_rim": (255, 252, 246),
            "win_border": (190, 178, 160),
            "divider": (205, 194, 176),
            "shadow_alpha": 46,
        }
    if is_light:
        return {
            "light": True,
            "mat_tl": (207, 214, 226),
            "mat_br": (234, 238, 245),
            "mat_glow": (250, 252, 255),
            "glow_pos": (0.28, 0.16),
            "title_bg": (224, 228, 237),
            "title_fg": (26, 30, 38),
            "title_muted": (94, 104, 120),
            "title_accent": (30, 79, 189),
            "win_rim": (252, 253, 255),
            "win_border": (182, 190, 204),
            "divider": (198, 205, 216),
            "shadow_alpha": 44,
        }
    if "atelier" in scene_name:
        return {
            "light": False,
            "mat_tl": (15, 12, 10),
            "mat_br": (28, 21, 17),
            "mat_glow": (78, 43, 30),
            "glow_pos": (0.22, 0.18),
            "title_bg": (26, 23, 20),
            "title_fg": (250, 245, 236),
            "title_muted": (173, 163, 150),
            "title_accent": (230, 130, 92),
            "win_rim": (78, 69, 60),
            "win_border": (48, 43, 38),
            "divider": (46, 41, 36),
            "shadow_alpha": 140,
        }
    if "material" in scene_name or "query-bench" in scene_name or "sql-runbook" in scene_name:
        return {
            "light": False,
            "mat_tl": (11, 14, 20),
            "mat_br": (21, 26, 36),
            "mat_glow": (40, 62, 98),
            "glow_pos": (0.26, 0.18),
            "title_bg": (23, 26, 34),
            "title_fg": (241, 245, 251),
            "title_muted": (154, 163, 178),
            "title_accent": (138, 180, 248),
            "win_rim": (70, 79, 96),
            "win_border": (44, 50, 62),
            "divider": (42, 48, 59),
            "shadow_alpha": 145,
        }
    if "field" in scene_name or "schema-snapshot" in scene_name:
        return {
            "light": False,
            "mat_tl": (14, 15, 19),
            "mat_br": (27, 30, 38),
            "mat_glow": (82, 46, 22),
            "glow_pos": (0.78, 0.22),
            "title_bg": (28, 31, 39),
            "title_fg": (244, 243, 239),
            "title_muted": (155, 163, 180),
            "title_accent": (255, 106, 0),
            "win_rim": (74, 82, 100),
            "win_border": (46, 52, 66),
            "divider": (46, 52, 64),
            "shadow_alpha": 145,
        }
    # Midnight Dark (overview, palette, completion, plan, error, update)
    return {
        "light": False,
        "mat_tl": (10, 12, 20),
        "mat_br": (20, 25, 42),
        "mat_glow": (42, 56, 104),
        "glow_pos": (0.24, 0.18),
        "title_bg": (22, 25, 38),
        "title_fg": (230, 233, 239),
        "title_muted": (154, 164, 178),
        "title_accent": (130, 170, 255),
        "win_rim": (68, 76, 106),
        "win_border": (42, 48, 70),
        "divider": (42, 47, 66),
        "shadow_alpha": 150,
    }


def make_studio_backdrop(width: int, height: int, spec: dict) -> Image.Image:
    gw, gh = 140, 90
    tiny = Image.new("RGB", (gw, gh))
    tl = spec["mat_tl"]
    br = spec["mat_br"]
    glow = spec["mat_glow"]
    gx, gy = spec["glow_pos"]
    pixels = []
    for y in range(gh):
        ny = y / max(1, gh - 1)
        for x in range(gw):
            nx = x / max(1, gw - 1)
            diag = 0.55 * nx + 0.45 * ny
            base = lerp_rgb(tl, br, diag)
            dist2 = ((nx - gx) * 1.15) ** 2 + ((ny - gy) * 1.45) ** 2
            halo = math.exp(-dist2 * 3.8) * 0.65
            pixels.append(lerp_rgb(base, glow, halo))
    tiny.putdata(pixels)
    return tiny.resize((width, height), Image.Resampling.BICUBIC)


def draw_buffer(image, scene, mask, buf_x0, buf_y0):
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
            # Smooth editorial attenuation instead of harsh 50% dim.
            fg = lerp_rgb(bg, fg, 0.72)
        x = buf_x0 + col * CELL_WIDTH
        y = buf_y0 + row * CELL_HEIGHT
        draw.rectangle(
            (x, y, x + len(symbols) * CELL_WIDTH - 1, y + CELL_HEIGHT - 1),
            fill=bg,
        )
        spans.append((x, y, symbols, fg, styles))
    if expected_index != width * height:
        raise ValueError(f"{scene['scene']}: capture does not cover the whole buffer")

    # Draw foreground vectors and text after backgrounds so wide glyphs never get clipped.
    for x, y, symbols, fg, styles in spans:
        if styles & HIDDEN:
            continue
        for offset, symbol in enumerate(symbols):
            cell_x = x + offset * CELL_WIDTH
            if symbol in BOX_GLYPHS:
                draw_vector_box(draw, symbol, cell_x, y, fg)
            elif symbol in BLOCK_BARS:
                frac = BLOCK_BARS[symbol]
                bar_w = max(2, round(CELL_WIDTH * frac))
                draw.rectangle((cell_x, y, cell_x + bar_w - 1, y + CELL_HEIGHT - 1), fill=fg)
            elif symbol in LOWER_BLOCKS:
                frac = LOWER_BLOCKS[symbol]
                bar_h = max(2, round((CELL_HEIGHT - 6) * frac))
                pad_x = SCALE
                by1 = y + BASELINE + 2
                by0 = max(y + 2, by1 - bar_h + 1)
                draw.rectangle((cell_x + pad_x, by0, cell_x + CELL_WIDTH - 1 - pad_x, by1), fill=fg)
            elif symbol.strip():
                glyph, x_off = mask(symbol, bool(styles & BOLD), bool(styles & ITALIC))
                box = cell_x + x_off, y, cell_x + x_off + glyph.width, y + glyph.height
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


def render_scene(scene, fonts, chrome_fonts, mask):
    cols, rows = scene["width"], scene["height"]
    surface_bg = scene_surface_bg(scene)
    spec = studio_palette(scene["scene"], surface_bg)

    buf_w = cols * CELL_WIDTH
    buf_h = rows * CELL_HEIGHT
    win_w = buf_w + WIN_PAD_X * 2
    win_h = TITLEBAR_H + WIN_PAD_Y + buf_h + WIN_PAD_Y
    canvas_w = win_w + MAT_PAD_X * 2
    canvas_h = win_h + MAT_PAD_TOP + MAT_PAD_BOTTOM

    wx0, wy0 = MAT_PAD_X, MAT_PAD_TOP
    wx1, wy1 = wx0 + win_w - 1, wy0 + win_h - 1

    # 1. Studio ambient mat
    backdrop = make_studio_backdrop(canvas_w, canvas_h, spec).convert("RGBA")

    # 2. Multi-stage macOS ambient + contact drop shadow
    ambient = Image.new("RGBA", (canvas_w, canvas_h), (0, 0, 0, 0))
    a_draw = ImageDraw.Draw(ambient)
    a_draw.rounded_rectangle(
        (wx0, wy0 + 14 * SCALE, wx1, wy1 + 14 * SCALE),
        radius=WINDOW_RADIUS,
        fill=(0, 0, 0, spec["shadow_alpha"]),
    )
    ambient = ambient.filter(ImageFilter.GaussianBlur(radius=22 * SCALE))
    backdrop = Image.alpha_composite(backdrop, ambient)

    contact = Image.new("RGBA", (canvas_w, canvas_h), (0, 0, 0, 0))
    c_draw = ImageDraw.Draw(contact)
    c_draw.rounded_rectangle(
        (wx0, wy0 + 3 * SCALE, wx1, wy1 + 3 * SCALE),
        radius=WINDOW_RADIUS,
        fill=(0, 0, 0, spec["shadow_alpha"] // 2),
    )
    contact = contact.filter(ImageFilter.GaussianBlur(radius=5 * SCALE))
    backdrop = Image.alpha_composite(backdrop, contact)

    # 3. Rounded floating terminal window interior
    win_layer = Image.new("RGB", (canvas_w, canvas_h), surface_bg)
    w_draw = ImageDraw.Draw(win_layer)
    # Titlebar header background (upper strip, masked to window radius below)
    w_draw.rectangle((wx0, wy0, wx1, wy0 + TITLEBAR_H - 1), fill=spec["title_bg"])
    w_draw.line((wx0, wy0 + TITLEBAR_H, wx1, wy0 + TITLEBAR_H), fill=spec["divider"], width=SCALE)

    # Subtle traffic lights (tastefully harmonized saturation)
    dot_r = 6 * SCALE
    dot_y = wy0 + TITLEBAR_H // 2
    dots = [
        (wx0 + 20 * SCALE, (226, 101, 89)),
        (wx0 + 40 * SCALE, (224, 174, 78)),
        (wx0 + 60 * SCALE, (101, 179, 122)),
    ]
    for dx, dc in dots:
        w_draw.ellipse((dx - dot_r, dot_y - dot_r, dx + dot_r, dot_y + dot_r), fill=dc)

    # Editorial serif "Ignatius" + humanist title separator + scene title
    brand_font, ui_font = chrome_fonts
    brand_text = "Ignatius"
    sub_text = f"  \u00b7  {scene['title']}"
    bw = w_draw.textlength(brand_text, font=brand_font)
    sw = w_draw.textlength(sub_text, font=ui_font)
    tx0 = wx0 + round((win_w - (bw + sw)) / 2)
    w_draw.text((tx0, dot_y), brand_text, font=brand_font, fill=spec["title_fg"], anchor="lm")
    w_draw.text(
        (tx0 + bw, dot_y + 1),
        sub_text,
        font=ui_font,
        fill=spec["title_muted"],
        anchor="lm",
    )

    # Paint live Ratatui buffer inside the padded window viewport
    buf_x0 = wx0 + WIN_PAD_X
    buf_y0 = wy0 + TITLEBAR_H + WIN_PAD_Y
    draw_buffer(win_layer, scene, mask, buf_x0, buf_y0)

    # Composite rounded window onto studio mat
    win_mask = Image.new("L", (canvas_w, canvas_h), 0)
    m_draw = ImageDraw.Draw(win_mask)
    m_draw.rounded_rectangle((wx0, wy0, wx1, wy1), radius=WINDOW_RADIUS, fill=255)
    composed = Image.composite(win_layer, backdrop.convert("RGB"), win_mask)

    # Crisp 1px outer window border and subtle inner glass highlight
    rim_draw = ImageDraw.Draw(composed)
    rim_draw.rounded_rectangle(
        (wx0 + 1, wy0 + 1, wx1 - 1, wy1 - 1),
        radius=WINDOW_RADIUS - 1,
        outline=spec["win_rim"],
        width=SCALE // 2 or 1,
    )
    rim_draw.rounded_rectangle(
        (wx0, wy0, wx1, wy1),
        radius=WINDOW_RADIUS,
        outline=spec["win_border"],
        width=SCALE // 2 or 1,
    )

    meta = PngImagePlugin.PngInfo()
    meta.add_text("Title", scene["title"])
    meta.add_text(
        "Description",
        f"{scene['subtitle']}. Synthetic Ratatui buffer ({cols}x{rows}) at 2x Retina studio resolution.",
    )
    return composed, meta


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("capture", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--font-dir", type=Path, required=True)
    parser.add_argument("--fallback-font", type=Path, required=True)
    parser.add_argument("--font-family", default="CommitMonoNerdFontMono")
    args = parser.parse_args()

    fonts = {
        (bold, italic): ImageFont.truetype(
            resolve_mono_font(args.font_dir, args.font_family, style), FONT_SIZE
        )
        for bold, italic, style in [
            (False, False, "Regular"),
            (True, False, "Bold"),
            (False, True, "Italic"),
            (True, True, "BoldItalic"),
        ]
    }
    chrome_fonts = load_chrome_fonts(fonts[False, False])
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
            image, meta = render_scene(scene, fonts, chrome_fonts, mask)
            image.save(args.output / f"{name}.png", optimize=True, pnginfo=meta)


if __name__ == "__main__":
    main()
