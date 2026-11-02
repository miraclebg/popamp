"""Helpers the skin generators share: SVG files, text outlines, skin.toml.

Set `OUT` to the skin folder before saving pictures.
"""
import os

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

OUT = None


# --- svg -------------------------------------------------------------------------
class Svg:
    def __init__(self, w, h):
        self.w, self.h, self.defs, self.body, self.n = w, h, [], [], 0

    def grad(self, stops, x2=0, y2=1):
        self.n += 1
        gid = f"g{self.n}"
        s = "".join(f'<stop offset="{o}" stop-color="{c}"/>' for o, c in spread(stops))
        self.defs.append(f'<linearGradient id="{gid}" x1="0" y1="0" x2="{x2}" y2="{y2}">{s}</linearGradient>')
        return f"url(#{gid})"

    def add(self, s):
        self.body.append(s)

    def save(self, name):
        text = (
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{self.w}" height="{self.h}" viewBox="0 0 {self.w} {self.h}">\n'
            f"<defs>\n" + "\n".join(self.defs) + "\n</defs>\n" + "\n".join(self.body) + "\n</svg>\n"
        )
        path = os.path.join(OUT, name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write(text)


def spread(colors):
    if isinstance(colors, str):
        colors = (colors, colors)
    n = len(colors) - 1
    return [(i / n, c) for i, c in enumerate(colors)]


def rrect(x, y, w, h, r, fill, stroke=None, sw=1, extra=""):
    st = f' stroke="{stroke}" stroke-width="{sw}"' if stroke else ""
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{r}" fill="{fill}"{st}{extra}/>'


class Outlines:
    """Texts of a TrueType font as SVG paths (skins do not draw <text>)."""

    def __init__(self, path):
        font = TTFont(path)
        self.glyphs, self.cmap, self.upm = font.getGlyphSet(), font.getBestCmap(), font["head"].unitsPerEm

    def text_path(self, s, size):
        pen = SVGPathPen(self.glyphs)
        scale, x = size / self.upm, 0.0
        for ch in s:
            g = self.cmap[ord(ch)]
            self.glyphs[g].draw(TransformPen(pen, (scale, 0, 0, -scale, x, 0)))
            x += self.glyphs[g].width * scale
        return pen.getCommands(), x

    def label(self, s, x, y, size, fill, anchor="middle"):
        """`s` with its baseline at y; anchored at x."""
        d, w = self.text_path(s, size)
        dx = {"start": x, "middle": x - w / 2, "end": x - w}[anchor]
        return f'<path transform="translate({dx:.2f},{y:.2f})" d="{d}" fill="{fill}"/>'


# --- toml ------------------------------------------------------------------------


def val(v):
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, str):
        return f'"{v}"'
    if isinstance(v, dict):
        return "{ " + ", ".join(f"{k} = {val(x)}" for k, x in v.items()) + " }"
    if isinstance(v, (list, tuple)):
        return "[" + ", ".join(val(x) for x in v) + "]"
    return str(v)


def part(file, x, y, w, h):
    return {"file": file, "area": [x, y, w, h]}


class Toml:
    def __init__(self):
        self.lines = []

    def table(self, header, **kv):
        self.lines.append(f"\n{header}")
        for k, v in kv.items():
            self.lines.append(f"{k} = {val(v)}")

    def el(self, mode, **kv):
        self.table(f"[[window.{mode}.element]]", **kv)

    def save(self):
        with open(os.path.join(OUT, "skin.toml"), "w") as f:
            f.write("\n".join(self.lines).lstrip() + "\n")


def look2(file, w, h):
    """normal/pressed from a two-frame file."""
    return {"normal": part(file, 0, 0, w, h), "pressed": part(file, w, 0, w, h)}


def look4(file, w, h):
    return {
        "normal": part(file, 0, 0, w, h),
        "pressed": part(file, w, 0, w, h),
        "on": part(file, 2 * w, 0, w, h),
        "on_pressed": part(file, 3 * w, 0, w, h),
    }
