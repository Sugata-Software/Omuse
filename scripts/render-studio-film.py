#!/usr/bin/env python3
"""Render the 0.7.0 film from real captures and native exports.

Requires Pillow, NumPy, FFmpeg and an explicitly supplied production asset
directory. See docs/media/omuse-0.7.0-studio-film.md. Camera moves, typography
and comparison wipes are editorial animation, never simulated app gestures.
The existing credited AAC soundtrack is remuxed without changing its samples.
"""
from __future__ import annotations

import argparse
from functools import lru_cache
import json
import math
from pathlib import Path
import subprocess
import time

import numpy as np
from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

REPO = Path(__file__).resolve().parents[1]
W, H, FPS = 1920, 1080, 30
PAPER, INK = (247, 235, 214), (43, 22, 32)
DARK = (22, 15, 25)
ORANGE, CORAL, GOLD, PINK = (226, 98, 42), (240, 136, 90), (242, 179, 61), (255, 46, 136)
CHAPTERS = [
    (0, 7, "Photos. Vectors. Possibility."),
    (7, 18, "Find the light"),
    (18, 26, "Shape the contrast"),
    (26, 34, "Choose the focus"),
    (34, 42, "Change the palette"),
    (42, 51, "Draw and refine"),
    (51, 60, "Trace into editable shapes"),
    (60, 66, "Make colour deliberate"),
    (66, 78, "Build a collection"),
    (78, 87, "Create with a chosen connection"),
    (87, 96, "Give it motion"),
    (96, 105, "Export the whole set"),
    (105, 112, "Keep your hands on the keyboard"),
    (112, 120, "Make the next thing"),
]
ASSETS: Path
OUTPUT: Path


def clamp(x):
    return min(1.0, max(0.0, x))


def ease(x):
    return 1 - (1 - clamp(x)) ** 3


def smooth(x):
    x = clamp(x)
    return x * x * (3 - 2 * x)


def mix(a, b, t):
    return tuple(round(x * (1 - t) + y * t) for x, y in zip(a, b))


@lru_cache(maxsize=64)
def font(size, weight=500):
    f = ImageFont.truetype(str(REPO / "rust/assets/fonts/Outfit.ttf"), size)
    f.set_variation_by_axes([weight])
    return f


@lru_cache(maxsize=64)
def asset(name):
    path = REPO / name if name.startswith("docs/") else ASSETS / name
    return Image.open(path).convert("RGBA")


def paste(canvas, im, x, y, opacity=1):
    if opacity < 0.999:
        im = im.copy()
        im.putalpha(im.getchannel("A").point(lambda a: round(a * clamp(opacity))))
    canvas.alpha_composite(im, (round(x), round(y)))


def text(canvas, value, xy, size=26, color=INK, weight=500, opacity=1):
    if opacity <= 0:
        return
    f = font(size, weight)
    box = f.getbbox(value, anchor="lt")
    assert xy[0] + box[2] < W - 35, f"Text exceeds safe area: {value}"
    layer = Image.new("RGBA", (max(1, box[2] + 8), max(1, box[3] + 8)))
    ImageDraw.Draw(layer).text((0, 0), value, font=f, fill=(*color, round(255 * opacity)), anchor="lt")
    paste(canvas, layer, *xy)


@lru_cache(maxsize=100)
def label(value, size, color):
    f = font(size, 550)
    bounds = f.getbbox(value)
    im = Image.new("RGBA", (bounds[2] - bounds[0] + 10, bounds[3] - bounds[1] + 10))
    ImageDraw.Draw(im).text((5 - bounds[0], 5 - bounds[1]), value, font=f, fill=color)
    return im.crop(im.getchannel("A").getbbox())


def chip(canvas, value, x, y, dark=False, accent=True, size=21):
    ink = label(value, size, PAPER if dark or accent else INK)
    width, height = ink.width + 40, ink.height + 28
    ImageDraw.Draw(canvas).rounded_rectangle((x, y, x + width, y + height), 3,
        fill=ORANGE if accent else ((65, 41, 48) if dark else (234, 219, 197)))
    # Centre visible glyphs, accounting for their bearings and antialias fringes.
    l, t, r, b = ink.getchannel("A").point(lambda a: 255 if a >= 160 else 0).getbbox()
    paste(canvas, ink, x + (width - l - r) / 2, y + (height - t - b) / 2)
    return width


@lru_cache(maxsize=2)
def background(dark=False):
    yy, xx = np.mgrid[0:H, 0:W].astype(np.float32)
    glow = np.exp(-(((xx - 1500) / 1100) ** 2 + ((yy - 350) / 850) ** 2))
    base, tint = (DARK, (34, 12, 9)) if dark else (PAPER, (4, -10, -21))
    pixels = np.clip(np.array(base) + glow[:, :, None] * np.array(tint), 0, 255).astype("uint8")
    return Image.fromarray(pixels).convert("RGBA")


def rings(canvas, t, dark=False):
    layer = Image.new("RGBA", (W, H))
    d = ImageDraw.Draw(layer)
    for i in range(6):
        r = 290 + 26 * i + 5 * math.sin(t / 3)
        d.arc((1760 - r, 145 - r, 1760 + r, 145 + r), 20 + t, 280 + t,
              fill=(*(CORAL if dark else ORANGE), 30), width=2)
    canvas.alpha_composite(layer)


def base(index, t, dark):
    im = background(dark).copy()
    rings(im, t, dark)
    colour = PAPER if dark else INK
    text(im, "O M U S E", (90, 64), 24, colour, 650)
    text(im, "0.7.0  /  PHOTOS + VECTORS + CONTENT", (1250, 70), 19, colour, 450)
    ImageDraw.Draw(im).line((90, 115, 1830, 115), fill=(*colour, 60))
    text(im, f"{index:02} / {CHAPTERS[index][2].upper()}", (90, 154), 19, colour, 450)
    return im


def footer(im, t, note="NATIVE ON LINUX  /  BUILT FOR OMARCHY", dark=False):
    col = mix(PAPER, INK, 0.35) if dark else mix(INK, PAPER, 0.25)
    text(im, note, (90, 1016), 17, col, 450)
    text(im, "OMUSE 0.7.0", (1685, 1016), 17, col, 550)
    ImageDraw.Draw(im).rectangle((0, 1076, round(W * clamp(t / 120)), 1079), fill=ORANGE)


def heading(im, lines, u, dark, x=90, y=245, size=74):
    for i, line in enumerate(lines):
        a = ease((u - i * 0.10) / 0.6)
        text(im, line, (x, y + size * 1.07 * i + 20 * (1 - a)), size,
             PAPER if dark else INK, 550, a)


@lru_cache(maxsize=64)
def panel(name, width, radius=3):
    im = asset(name)
    im = im.resize((width, round(im.height * width / im.width)), Image.Resampling.LANCZOS)
    mask = Image.new("L", im.size)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, im.width - 1, im.height - 1), radius, fill=255)
    im.putalpha(ImageChops.multiply(im.getchannel("A"), mask))
    return im


@lru_cache(maxsize=32)
def shadow(width, height):
    im = Image.new("RGBA", (width + 80, height + 80))
    ImageDraw.Draw(im).rounded_rectangle((40, 40, width + 40, height + 40), 6, fill=(8, 2, 5, 75))
    return im.filter(ImageFilter.GaussianBlur(17))


def card(im, name, x, y, width, opacity=1):
    p = panel(name, width)
    paste(im, shadow(*p.size), x - 40, y - 26, opacity)
    paste(im, p, x, y, opacity)


def screenshot(im, name, u, x=610, y=230, width=1220):
    a = ease(u / 0.75)
    card(im, "docs/releases/images/v0.7.0/" + name, x + 36 * (1 - a), y + 20 * (1 - a), width, a)


PHOTO = {
    1: ("grade", ["Find", "the light."], ["Live exposure.", "Editable colour balance."], "LIVE ADJUSTMENTS", "WILLIAN JUSTEN DE VASCONCELLOS / PEXELS"),
    2: ("mono", ["Shape", "the contrast."], ["Channel mixing.", "Curves with character."], "MONOCHROME + CURVES", "ENGIN AKYURT / UNSPLASH"),
    3: ("focus", ["Choose", "the focus."], ["A softer background.", "A retained layer mask."], "BLUR + MASK", "MARKO MILIVOJEVIC / PIXNIO"),
    4: ("colour", ["Change", "the feeling."], ["Map a new palette.", "Keep the natural detail."], "GRADIENT MAP", "ENGIN AKYURT / UNSPLASH"),
}


def photo_scene(index, u, t):
    key, title, lines, tag, credit = PHOTO[index]
    im = base(index, t, True)
    heading(im, title, u, True)
    for j, line in enumerate(lines):
        text(im, line, (95, 460 + j * 43), 28, mix(PAPER, CORAL, 0.18), 400)
    chip(im, tag, 95, 600, True)
    text(im, "Rendered by Omuse 0.7.0", (95, 685), 23, mix(PAPER, CORAL, 0.28), 400)
    before = panel(f"photo-{key}-before.png", 1090)
    after = panel(f"photo-{key}-after.png", 1090)
    if before.height > 775:
        width = round(1090 * 775 / before.height)
        before, after = panel(f"photo-{key}-before.png", width), panel(f"photo-{key}-after.png", width)
    assert before.size == after.size
    width, height = before.size
    x, y = 1280 - width / 2, 576 - height / 2
    amount = smooth((u - 0.9) / 3.1)
    if 5.0 < u < 6.0:
        amount = 1 - 0.5 * smooth(u - 5.0)
    elif 6.0 <= u < 7.0:
        amount = 0.5 + 0.5 * smooth(u - 6.0)
    picture = before.copy()
    reveal = round(width * amount)
    if reveal:
        picture.paste(after.crop((0, 0, reveal, height)), (0, 0))
    paste(im, shadow(width, height), x - 40, y - 26)
    paste(im, picture, x, y)
    if 0 < reveal < width:
        ImageDraw.Draw(im).line((x + reveal, y, x + reveal, y + height), fill=PAPER, width=3)
    chip(im, "ORIGINAL" if amount < 0.01 else "OMUSE EDIT", x + 18, y + 18, True, amount > 0.01, 19)
    if 0.1 < amount < 0.9:
        chip(im, "ORIGINAL", x + width - label("ORIGINAL", 19, PAPER).width - 58, y + 18, True, False, 19)
    footer(im, t, "PHOTO: " + credit, True)
    return im


@lru_cache(maxsize=1)
def motion():
    raw = subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(ASSETS / "native-motion.mp4"),
        "-an", "-vf", "scale=520:650:flags=lanczos,fps=30", "-pix_fmt", "rgb24", "-f", "rawvideo", "-"])
    return np.frombuffer(raw, dtype=np.uint8).reshape((-1, 650, 520, 3))


def scene(index, u, t):
    if index in PHOTO:
        return photo_scene(index, u, t)
    dark = index in (5, 6, 9, 10, 12)
    im = base(index, t, dark)
    fg = PAPER if dark else INK
    note = "NATIVE ON LINUX  /  BUILT FOR OMARCHY"
    if index in (0, 13):
        im = background(False).copy()
        rings(im, t)
        a = ease(u / 0.85)
        paste(im, panel("splash-muse.png", 710), 1070 + 35 * (1 - a), 190, a)
        text(im, "O M U S E   /   0.7.0", (95, 122), 25, INK, 600)
        heading(im, ["Photos. Vectors.", "Possibility."] if index == 0 else ["Make the", "next thing."], u, False, 90, 288, 86)
        text(im, "One native creative studio for Linux.", (96, 538), 31, INK, 400)
        chip(im, "BUILT FOR OMARCHY", 96, 623)
        for j, colour in enumerate((GOLD, CORAL, ORANGE, PINK)):
            ImageDraw.Draw(im).rectangle((96 + 145 * j, 725, 216 + 145 * j, 731), fill=colour)
        if index == 0:
            text(im, "Real artwork. Real app captures. Room to create.", (96, 793), 28, INK, 400)
        else:
            text(im, "github.com/Sugata-Software/Omuse", (96, 787), 32, INK, 550)
            text(im, "Install · Learn · Create", (96, 842), 25, INK, 400)
            text(im, "Music: Night Owl — Broke For Free · edited for Omuse", (96, 921), 20, INK, 450)
            text(im, "brokeforfree.bandcamp.com · CC BY 3.0 · creativecommons.org/licenses/by/3.0", (96, 955), 18, INK, 400)
        note = "OMUSE 0.7.0  /  SOURCE PRE-RELEASE  /  ARCH + OMARCHY"
    elif index == 5:
        heading(im, ["Shape", "every idea."], u, dark, size=70)
        text(im, "Pen. Nodes. Move.", (95, 459), 28, fg, 400)
        text(im, "One shared canvas.", (95, 502), 28, fg, 400)
        chip(im, "P  /  A  /  V", 95, 610, dark)
        screenshot(im, "01-vector-pen.png" if u < 5.2 else "02-vector-artwork.png", u)
    elif index == 6:
        heading(im, ["Pixels to", "possibilities."], u, dark, size=67)
        text(im, "Trace locally.", (95, 459), 28, fg, 400)
        text(im, "Refine editable curves.", (95, 502), 28, fg, 400)
        chip(im, "DETAIL, ON YOUR TERMS", 95, 610, dark, size=19)
        screenshot(im, "03-image-trace.png", u)
        note = "LOCAL IMAGE TRACE  /  SOLID-COLOUR APPROXIMATIONS  /  KEEP THE ORIGINAL"
    elif index == 7:
        heading(im, ["Make colour", "deliberate."], u, dark, size=65)
        text(im, "Targeted colour control.", (95, 459), 27, fg, 400)
        text(im, "Visible fill and stroke.", (95, 502), 27, fg, 400)
        screenshot(im, "05-target-colour.png" if u < 3.7 else "04-photo-curves.png", u)
    elif index == 8:
        heading(im, ["One idea. A whole collection."], u, dark, 90, 218, 72)
        text(im, "Editable pages. Shared brand. Your content, together.", (96, 324), 29, fg, 400)
        if u < 7.3:
            for j in range(6):
                a = ease((u - 0.08 * j) / 0.75)
                x, y = 95 + j * 318 - u * 38, 430 + 18 * math.sin(j * 0.8 + u / 4)
                card(im, f"page-{j + 1}.png", x, y + 30 * (1 - a), 282, a)
        else:
            screenshot(im, "06-create.png", u - 7.3, 440, 380, 1010)
        note = "NATIVE PAGES EXPORTED BY OMUSE 0.7.0  /  SAVE THE EDITABLE .OMUSE PROJECT"
    elif index == 9:
        heading(im, ["Your brief.", "Your choice."], u, dark, size=68)
        text(im, "Choose a connection.", (95, 459), 27, fg, 400)
        text(im, "Review what you keep.", (95, 502), 27, fg, 400)
        chip(im, "ASK OMUSE", 95, 610, dark)
        screenshot(im, "07-ai-assistant.png", u)
        note = "OFFLINE INTERFACE DEMO  /  NO NEW AI REQUEST  /  CAPABILITIES VARY BY CONNECTION"
    elif index == 10:
        heading(im, ["Give it", "motion."], u, dark, size=77)
        text(im, "Pages. Transitions. Rhythm.", (95, 459), 28, fg, 400)
        text(im, "Export MP4 or GIF.", (95, 502), 28, fg, 400)
        chip(im, "ACTUAL OMUSE EXPORT", 95, 610, dark, size=19)
        if u < 4.1:
            frames = motion()
            frame = Image.fromarray(frames[min(len(frames) - 1, round(u * FPS))]).convert("RGBA")
            paste(im, shadow(520, 650), 1130, 225)
            paste(im, frame, 1170, 251)
        else:
            screenshot(im, "09-motion.png", u - 4.1)
        note = "CURRENT RELEASE MOTION EXPORT  /  FOLLOWED BY THE MOTION WORKSPACE"
    elif index == 11:
        heading(im, ["Ready", "to share."], u, dark, size=76)
        text(im, "Export the whole set.", (95, 459), 28, fg, 400)
        text(im, "Keep the editable project.", (95, 502), 28, fg, 400)
        chip(im, "PNG  /  PDF  /  MP4", 95, 610, size=20)
        screenshot(im, "08-content-export.png", u)
    elif index == 12:
        heading(im, ["Stay", "in flow."], u, dark, size=79)
        text(im, "Search. Find. Execute.", (95, 459), 28, fg, 400)
        text(im, "Make shortcuts your own.", (95, 502), 27, fg, 400)
        chip(im, "CTRL + K", 95, 610, dark)
        screenshot(im, "10-command-search.png", u)
    footer(im, t, note, dark)
    return im


def frame(t):
    index = next(i for i, (start, end, _) in enumerate(CHAPTERS) if start <= t < end)
    u = t - CHAPTERS[index][0]
    im = scene(index, u, t)
    if index and u < 0.27:
        start, end, _ = CHAPTERS[index - 1]
        old = scene(index - 1, end - start - 0.001, end - 0.001)
        im = Image.blend(old, im, smooth(u / 0.27))
    return im.convert("RGB")


def proof():
    samples = [(s + min(3.3, (e - s) / 2), name) for s, e, name in CHAPTERS]
    samples += [(49.0, "Vector artwork"), (65.1, "Camera Raw"), (76.0, "Create"), (94.0, "Motion workspace")]
    sheet = Image.new("RGB", (1440, math.ceil(len(samples) / 3) * 298), DARK)
    for i, (t, title) in enumerate(samples):
        im = frame(t)
        im.save(OUTPUT / "review" / f"frame-{t:05.1f}.png")
        x, y = (i % 3) * 480, (i // 3) * 298
        sheet.paste(im.resize((480, 270), Image.Resampling.LANCZOS), (x, y))
        ImageDraw.Draw(sheet).text((x + 10, y + 276), f"{t:05.1f}s · {title}", font=font(14, 450), fill=PAPER)
    sheet.save(OUTPUT / "review/storyboard.jpg", quality=94)
    frame(49).save(OUTPUT / "review/poster-design.png")
    (OUTPUT / "edit-plan.json").write_text(json.dumps({"width": W, "height": H, "fps": FPS,
        "duration": 120, "chapters": [{"start": s, "end": e, "title": n} for s, e, n in CHAPTERS]}, indent=2) + "\n")


def render():
    path = OUTPUT / "renders/picture.mp4"
    stage = path.with_name("picture.work.mp4")
    cmd = ["ffmpeg", "-v", "warning", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}",
        "-r", str(FPS), "-i", "-", "-an", "-c:v", "libx264", "-preset", "medium", "-crf", "19",
        "-threads", "3", "-pix_fmt", "yuv420p", "-color_primaries", "bt709", "-color_trc", "bt709",
        "-colorspace", "bt709", "-movflags", "+faststart", str(stage)]
    with (OUTPUT / "renders/encoder.log").open("w") as log:
        p = subprocess.Popen(cmd, stdin=subprocess.PIPE, stderr=log)
        started = time.monotonic()
        try:
            for n in range(FPS * 120):
                p.stdin.write(frame(n / FPS).tobytes())
                if n % 150 == 0:
                    print(f"{n}/3600 frames; {time.monotonic() - started:.1f}s", flush=True)
            p.stdin.close()
            if p.wait() != 0:
                raise RuntimeError("FFmpeg failed; see renders/encoder.log")
            stage.replace(path)
        finally:
            if p.poll() is None:
                p.kill()
                p.wait()
    metadata = OUTPUT / "renders/chapters.ffmeta"
    metadata.write_text(";FFMETADATA1\n" + "".join(
        f"[CHAPTER]\nTIMEBASE=1/1000\nSTART={s * 1000}\nEND={e * 1000}\ntitle={title}\n"
        for s, e, title in CHAPTERS))
    credit = ("Music: Night Owl by Broke For Free, Directionless EP (2011). "
        "https://brokeforfree.bandcamp.com/track/night-owl CC BY 3.0: "
        "https://creativecommons.org/licenses/by/3.0/ Recording excerpted, edited, "
        "crossfaded, faded and level-adjusted in the retained Sunset Muse mix. No endorsement implied.")
    destination = OUTPUT / "omuse-0.7.0-studio-film.mp4"
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", str(path), "-i",
        str(REPO / "docs/media/omuse-sunset-muse.mp4"), "-i", str(metadata), "-map", "0:v:0", "-map", "1:a:0",
        "-map_metadata", "2", "-map_chapters", "2", "-c", "copy", "-movflags", "+faststart", "-t", "120",
        "-metadata", "title=Omuse 0.7.0 — Photos. Vectors. Possibility.", "-metadata", "comment=" + credit,
        "-metadata", "artist=Music: Broke For Free", "-metadata", "copyright=Music: Broke For Free, CC BY 3.0",
        str(destination)], check=True)
    print(destination, flush=True)


def main():
    global ASSETS, OUTPUT
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assets", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--render", action="store_true")
    args = parser.parse_args()
    ASSETS, OUTPUT = args.assets.resolve(), args.output.resolve()
    for folder in ("review", "renders"):
        (OUTPUT / folder).mkdir(parents=True, exist_ok=True)
    proof()
    if args.render:
        render()


if __name__ == "__main__":
    main()
