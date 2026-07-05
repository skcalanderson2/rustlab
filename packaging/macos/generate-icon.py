#!/usr/bin/env python3
"""Generate the RustLab app icon (macOS style).

Motif: a ringed planet — Jupyter's namesake — rendered in Rust's warm
orange on a deep-space navy squircle, following the Big Sur icon grid
(824/1024 squircle, ~22.4% corner radius).

Outputs RustLab-1024.png next to this script; build the .icns with
make-icns.sh (sips + iconutil).
"""

from PIL import Image, ImageDraw, ImageFilter, ImageChops
import math
import os
import random

S = 2  # supersampling factor
SIZE = 1024 * S
HERE = os.path.dirname(os.path.abspath(__file__))


def lerp(a, b, t):
    return tuple(round(a[i] + (b[i] - a[i]) * t) for i in range(len(a)))


def vertical_gradient(size, top, bottom):
    strip = Image.new("RGB", (1, 256))
    for y in range(256):
        strip.putpixel((0, y), lerp(top, bottom, y / 255))
    return strip.resize(size, Image.BICUBIC)


def radial_gradient(size, inner, outer, center, radius):
    img = Image.new("RGB", size, outer)
    draw = ImageDraw.Draw(img)
    steps = 256
    for i in range(steps, 0, -1):
        t = i / steps
        r = radius * t
        color = lerp(inner, outer, t)
        draw.ellipse(
            [center[0] - r, center[1] - r, center[0] + r, center[1] + r],
            fill=color,
        )
    return img


def squircle_mask(size, box, radius):
    mask = Image.new("L", size, 0)
    ImageDraw.Draw(mask).rounded_rectangle(box, radius=radius, fill=255)
    return mask


def main():
    random.seed(7)
    canvas = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))

    # --- Big Sur grid: 824x824 squircle centered on 1024 canvas ---
    margin = round(100 * S)
    box = [margin, margin, SIZE - margin, SIZE - margin]
    corner = round(185 * S)
    mask = squircle_mask((SIZE, SIZE), box, corner)

    # --- background: deep space, vertical + soft top glow ---
    bg = vertical_gradient((SIZE, SIZE), (30, 38, 61), (10, 13, 22)).convert("RGBA")
    glow = radial_gradient(
        (SIZE, SIZE),
        (52, 63, 94),
        (10, 13, 22),
        (SIZE * 0.5, SIZE * 0.28),
        SIZE * 0.75,
    ).convert("RGBA")
    bg = Image.blend(bg, glow, 0.45)

    # faint stars
    star_draw = ImageDraw.Draw(bg)
    for _ in range(90):
        x = random.uniform(box[0], box[2])
        y = random.uniform(box[1], box[3])
        r = random.uniform(0.8, 2.4) * S
        a = random.randint(40, 130)
        star_draw.ellipse([x - r, y - r, x + r, y + r], fill=(230, 236, 250, a))

    canvas.paste(bg, (0, 0), mask)

    # --- planet geometry ---
    cx, cy = SIZE * 0.5, SIZE * 0.52
    pr = SIZE * 0.235  # planet radius

    # --- ring layer (drawn tilted, split into back/front halves) ---
    ring_w, ring_h = SIZE * 0.78, SIZE * 0.27
    tilt = -16  # degrees
    ring_layer = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    rd = ImageDraw.Draw(ring_layer)
    rbox = [cx - ring_w / 2, cy - ring_h / 2, cx + ring_w / 2, cy + ring_h / 2]
    # two concentric strokes: broad translucent band + bright edge
    rd.ellipse(rbox, outline=(247, 206, 147, 110), width=round(26 * S))
    rd.ellipse(
        [c + d for c, d in zip(rbox, (18 * S, 7 * S, -18 * S, -7 * S))],
        outline=(255, 227, 181, 220),
        width=round(7 * S),
    )
    ring_layer = ring_layer.rotate(tilt, resample=Image.BICUBIC, center=(cx, cy))

    # half-plane mask rotated with the ring: front half = below ring plane
    half = Image.new("L", (SIZE, SIZE), 0)
    ImageDraw.Draw(half).rectangle([0, cy, SIZE, SIZE], fill=255)
    half = half.rotate(tilt, resample=Image.BICUBIC, center=(cx, cy))

    ring_alpha = ring_layer.split()[3]
    back_alpha = ImageChops.subtract(ring_alpha, half)
    front_alpha = ImageChops.multiply(ring_alpha, half.point(lambda v: 255 if v > 127 else 0))

    ring_back = ring_layer.copy()
    ring_back.putalpha(ImageChops.multiply(ring_alpha, back_alpha))
    ring_front = ring_layer.copy()
    ring_front.putalpha(front_alpha)

    # clip everything to the squircle
    def paste_clipped(layer):
        clipped = ImageChops.multiply(layer.split()[3], mask)
        layer = layer.copy()
        layer.putalpha(clipped)
        canvas.alpha_composite(layer)

    paste_clipped(ring_back)

    # --- planet: rust-orange radial gradient, banded, lit from top-left ---
    planet = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    grad = radial_gradient(
        (SIZE, SIZE),
        (255, 158, 92),
        (186, 52, 26),
        (cx - pr * 0.42, cy - pr * 0.48),
        pr * 1.85,
    ).convert("RGBA")
    pmask = Image.new("L", (SIZE, SIZE), 0)
    ImageDraw.Draw(pmask).ellipse([cx - pr, cy - pr, cx + pr, cy + pr], fill=255)
    planet.paste(grad, (0, 0), pmask)

    # jovian bands: translucent darker stripes clipped to the disk
    bands = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    bd = ImageDraw.Draw(bands)
    band_specs = [
        (-0.52, 0.10, 60), (-0.18, 0.13, 78), (0.22, 0.11, 70), (0.58, 0.09, 55),
    ]
    for off, h, alpha in band_specs:
        y0 = cy + off * pr - h * pr
        y1 = cy + off * pr + h * pr
        bd.rounded_rectangle(
            [cx - pr, y0, cx + pr, y1], radius=h * pr, fill=(96, 26, 16, alpha)
        )
    bands = bands.filter(ImageFilter.GaussianBlur(10 * S))
    bands.putalpha(ImageChops.multiply(bands.split()[3], pmask))
    planet.alpha_composite(bands)

    # terminator shading bottom-right + rim light top-left
    shade = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    sd = ImageDraw.Draw(shade)
    sd.ellipse(
        [cx - pr * 1.15, cy - pr * 1.15, cx + pr * 1.35, cy + pr * 1.35],
        fill=(20, 8, 8, 90),
    )
    shade = shade.filter(ImageFilter.GaussianBlur(38 * S))
    shade.putalpha(ImageChops.multiply(shade.split()[3], pmask))
    planet.alpha_composite(shade)

    hl = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    hd = ImageDraw.Draw(hl)
    hd.ellipse(
        [cx - pr * 0.95, cy - pr * 0.98, cx + pr * 0.35, cy + pr * 0.1],
        fill=(255, 214, 170, 60),
    )
    hl = hl.filter(ImageFilter.GaussianBlur(30 * S))
    hl.putalpha(ImageChops.multiply(hl.split()[3], pmask))
    planet.alpha_composite(hl)

    # soft planet shadow cast onto the ring/space behind
    drop = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    ImageDraw.Draw(drop).ellipse(
        [cx - pr * 1.06, cy - pr * 1.02, cx + pr * 1.10, cy + pr * 1.14],
        fill=(5, 6, 12, 120),
    )
    drop = drop.filter(ImageFilter.GaussianBlur(22 * S))
    paste_clipped(drop)

    paste_clipped(planet)
    paste_clipped(ring_front)

    # --- moons: two Jupyter-ish companion dots on the ring line ---
    moons = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    md = ImageDraw.Draw(moons)
    for (mx, my, mr, color, shade_color) in [
        (cx - ring_w * 0.335, cy + ring_h * 0.29, 16 * S, (226, 232, 246, 255), (148, 158, 184, 255)),
        (cx + ring_w * 0.365, cy - ring_h * 0.315, 12 * S, (247, 206, 147, 255), (196, 152, 96, 255)),
    ]:
        # small sphere: base + crescent shadow on the lower-right
        md.ellipse([mx - mr, my - mr, mx + mr, my + mr], fill=shade_color)
        md.ellipse(
            [mx - mr, my - mr, mx + mr * 0.72, my + mr * 0.72],
            fill=color,
        )
    paste_clipped(moons)

    # --- subtle top inner highlight on the squircle (Big Sur gloss) ---
    edge = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    ed = ImageDraw.Draw(edge)
    ed.rounded_rectangle(
        [box[0] + 2 * S, box[1] + 2 * S, box[2] - 2 * S, box[3] - 2 * S],
        radius=corner - 2 * S,
        outline=(255, 255, 255, 36),
        width=round(3 * S),
    )
    grad_fade = vertical_gradient((SIZE, SIZE), (255, 255, 255), (0, 0, 0)).convert("L")
    edge.putalpha(ImageChops.multiply(edge.split()[3], grad_fade))
    canvas.alpha_composite(edge)

    out = canvas.resize((1024, 1024), Image.LANCZOS)
    path = os.path.join(HERE, "RustLab-1024.png")
    out.save(path)
    print(f"wrote {path}")


if __name__ == "__main__":
    main()
