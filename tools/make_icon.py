"""Draw Next Launcher's original app icon (no third-party artwork).

Usage: python tools/make_icon.py <out.png>
Then:  npx tauri icon <out.png>   (generates every size into src-tauri/icons)
"""
import sys

from PIL import Image, ImageDraw

SIZE = 1024
SS = 4  # supersampling factor for smooth edges


def main(out: str) -> None:
    s = SIZE * SS
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))

    # Rounded square with a vertical gradient between two shades of the default accent.
    top, bottom = (24, 132, 240), (8, 92, 184)
    grad = Image.new("RGBA", (s, s))
    gd = ImageDraw.Draw(grad)
    for y in range(s):
        t = y / (s - 1)
        c = tuple(round(top[i] + (bottom[i] - top[i]) * t) for i in range(3)) + (255,)
        gd.line([(0, y), (s, y)], fill=c)
    mask = Image.new("L", (s, s), 0)
    margin = round(s * 0.06)
    ImageDraw.Draw(mask).rounded_rectangle(
        [margin, margin, s - margin, s - margin], radius=round(s * 0.22), fill=255
    )
    img.paste(grad, (0, 0), mask)

    # Bold forward chevron ("next").
    d = ImageDraw.Draw(img)
    w = round(s * 0.11)
    pts = [(s * 0.40, s * 0.28), (s * 0.62, s * 0.50), (s * 0.40, s * 0.72)]
    d.line(pts, fill=(255, 255, 255, 255), width=w, joint="curve")
    r = w // 2
    for x, y in (pts[0], pts[2]):
        d.ellipse([x - r, y - r, x + r, y + r], fill=(255, 255, 255, 255))

    img.resize((SIZE, SIZE), Image.LANCZOS).save(out)


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "icon-source.png")
