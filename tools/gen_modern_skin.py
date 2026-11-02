"""Generates the Modern-style test skins: SVG pictures and skin.toml.

Usage: python3 -I tools/gen_modern_skin.py skins/modern
       python3 -I tools/gen_modern_skin.py skins/bar bar
The second is the same skin with a bar across the screen for a main window.
Needs python3-fonttools and fonts-dejavu-core.
Own artwork in the spirit of the silver "modern" players; labels are
DejaVu Sans Condensed Bold outlines.
"""
import os
import sys

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

OUT = sys.argv[1]
BAR = sys.argv[2:] == ["bar"]
FONT = TTFont("/usr/share/fonts/truetype/dejavu/DejaVuSansCondensed-Bold.ttf")
GLYPHS = FONT.getGlyphSet()
CMAP = FONT.getBestCmap()
UPM = FONT["head"].unitsPerEm

# --- palette -----------------------------------------------------------------
BODY = ("#f1f3f6", "#d6dbe1", "#b3bac3")
EDGE = "#56606c"
TITLE = ("#cfd5dc", "#a7afb9")
LCD = ("#0a131b", "#15222d")
LCD_TEXT = "#8fd6ff"
LCD_DIM = "#29475a"
INK = "#2b333d"
ACCENT = "#2f9bff"
FACE = ("#fdfdfe", "#c3c9d1")
FACE_DOWN = ("#aab2bc", "#e2e6ea")
RING = "#66707c"


# --- svg helpers ---------------------------------------------------------------
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


def text_path(s, size):
    pen = SVGPathPen(GLYPHS)
    scale, x = size / UPM, 0.0
    for ch in s:
        g = CMAP[ord(ch)]
        GLYPHS[g].draw(TransformPen(pen, (scale, 0, 0, -scale, x, 0)))
        x += GLYPHS[g].width * scale
    return pen.getCommands(), x


def label(s, x, y, size, fill, anchor="middle"):
    """`s` with its baseline at y; anchored at x."""
    d, w = text_path(s, size)
    dx = {"start": x, "middle": x - w / 2, "end": x - w}[anchor]
    return f'<path transform="translate({dx:.2f},{y:.2f})" d="{d}" fill="{fill}"/>'


def panel(svg, x, y, w, h, r=3):
    """A dark display set into the body."""
    svg.add(rrect(x - 0.5, y - 0.5, w + 1, h + 1, r + 0.5, "none", "#ffffff", 1, ' opacity="0.8"'))
    svg.add(rrect(x, y, w, h, r, svg.grad(LCD), "#3a4450"))


def body(svg, w, h, r=7):
    svg.add(rrect(0.5, 0.5, w - 1, h - 1, r, svg.grad(BODY), EDGE))
    svg.add(rrect(1.5, 1.5, w - 3, h - 3, r - 1, "none", "#ffffff", 1, ' opacity="0.7"'))


def title_strip(svg, w, text, y=1, h=13):
    svg.add(f'<path d="M1.5 {y + h} V{y + 6} a6 6 0 0 1 6 -6 H{w - 7.5} a6 6 0 0 1 6 6 V{y + h} Z" fill="{svg.grad(TITLE)}"/>')
    svg.add(f'<line x1="2" y1="{y + h + 0.5}" x2="{w - 2}" y2="{y + h + 0.5}" stroke="#8a929c"/>')
    _, tw = text_path(text, 7.5)
    for side in (-1, 1):
        x0 = w / 2 + side * (tw / 2 + 6)
        x1 = 20 if side < 0 else w - 46
        for k in (0, 3):
            svg.add(f'<line x1="{x0}" y1="{y + 5 + k}" x2="{x1}" y2="{y + 5 + k}" stroke="#ffffff" opacity="0.8"/>')
            svg.add(f'<line x1="{x0}" y1="{y + 6 + k}" x2="{x1}" y2="{y + 6 + k}" stroke="#8a929c"/>')
    svg.add(label(text, w / 2, y + 9.5, 7.5, INK))


# --- buttons -------------------------------------------------------------------
ICONS = {
    "prev": lambda c, s: f'<path d="M{c - s * .55} {c - s * .5} h{s * .2} v{s} h{-s * .2} Z M{c + s * .5} {c - s * .5} v{s} L{c - s * .3} {c} Z" fill="{INK}"/>',
    "play": lambda c, s: f'<path d="M{c - s * .35} {c - s * .55} L{c + s * .6} {c} L{c - s * .35} {c + s * .55} Z" fill="{ACCENT}"/>',
    "pause": lambda c, s: f'<path d="M{c - s * .45} {c - s * .5} h{s * .3} v{s} h{-s * .3} Z M{c + s * .15} {c - s * .5} h{s * .3} v{s} h{-s * .3} Z" fill="{INK}"/>',
    "stop": lambda c, s: f'<rect x="{c - s * .45}" y="{c - s * .45}" width="{s * .9}" height="{s * .9}" fill="{INK}"/>',
    "next": lambda c, s: f'<path d="M{c + s * .55} {c - s * .5} h{-s * .2} v{s} h{s * .2} Z M{c - s * .5} {c - s * .5} v{s} L{c + s * .3} {c} Z" fill="{INK}"/>',
    "eject": lambda c, s: f'<path d="M{c - s * .5} {c + s * .1} L{c} {c - s * .5} L{c + s * .5} {c + s * .1} Z M{c - s * .5} {c + s * .3} h{s} v{s * .2} h{-s} Z" fill="{INK}"/>',
}


def round_button(name, d, icon, icon_size):
    """Normal and pressed side by side, each d x d."""
    svg = Svg(2 * d, d)
    for i, face in enumerate((FACE, FACE_DOWN)):
        ox, c, r = i * d, d / 2, d / 2 - 1
        svg.add(f'<g transform="translate({ox},0)">')
        svg.add(f'<circle cx="{c}" cy="{c + 0.6}" r="{r}" fill="#000000" opacity="0.25"/>')
        svg.add(f'<circle cx="{c}" cy="{c}" r="{r}" fill="{svg.grad(face)}" stroke="{RING}"/>')
        svg.add(f'<circle cx="{c}" cy="{c}" r="{r - 1.5}" fill="none" stroke="#ffffff" opacity="{0.8 if i == 0 else 0.3}"/>')
        svg.add(ICONS[icon](c + (0.5 if i else 0), icon_size))
        svg.add("</g>")
    svg.save(name)


def small_button(name, glyph):
    """Title bar buttons, 10 x 10, normal and pressed."""
    svg = Svg(20, 10)
    for i, face in enumerate((FACE, FACE_DOWN)):
        svg.add(f'<g transform="translate({i * 10},0)">')
        svg.add(rrect(0.5, 0.5, 9, 9, 2.5, svg.grad(face), RING))
        svg.add(glyph)
        svg.add("</g>")
    svg.save(name)


def pill(name, w, h, text, states=4):
    """normal, pressed, on, on_pressed side by side."""
    svg = Svg(w * states, h)
    for i in range(states):
        down, on = i in (1, 3), i >= 2
        face = FACE_DOWN if down else FACE
        svg.add(f'<g transform="translate({i * w},0)">')
        svg.add(rrect(0.5, 0.5, w - 1, h - 1, h / 2 - 0.5, svg.grad(face), RING))
        led = ACCENT if on else "#7d8792"
        svg.add(f'<circle cx="{h / 2 + 1}" cy="{h / 2}" r="1.6" fill="{led}"/>')
        if on:
            svg.add(f'<circle cx="{h / 2 + 1}" cy="{h / 2}" r="3" fill="{ACCENT}" opacity="0.35"/>')
        # Between the lamp and the right end, smaller if it would not fit.
        left, right = h / 2 + 4, w - 3
        size = min(7, 7 * (right - left) / text_path(text, 7)[1])
        svg.add(label(text, (left + right) / 2, h / 2 + size * 0.37, size, INK))
        svg.add("</g>")
    svg.save(name)


def thumb(name, w, h):
    svg = Svg(2 * w, h)
    for i, face in enumerate((FACE, FACE_DOWN)):
        svg.add(f'<g transform="translate({i * w},0)">')
        svg.add(rrect(0.5, 0.5, w - 1, h - 1, min(w, h) / 2 - 0.5, svg.grad(face), RING))
        svg.add(f'<line x1="{w / 2}" y1="2.5" x2="{w / 2}" y2="{h - 2.5}" stroke="{RING}" opacity="0.6"/>' if w > h else
                f'<line x1="2.5" y1="{h / 2}" x2="{w - 2.5}" y2="{h / 2}" stroke="{RING}" opacity="0.6"/>')
        svg.add("</g>")
    svg.save(name)


def fill_frames(name, w, h, count, vertical):
    """A groove filled to k/(count-1), frames stacked down (or across)."""
    fw, fh = (w * count, h) if vertical else (w, h * count)
    svg = Svg(fw, fh)
    glow = svg.grad(("#7fd0ff", ACCENT, "#1f62b8"), *((1, 0) if vertical else (0, 1)))
    for k in range(count):
        ox, oy = (k * w, 0) if vertical else (0, k * h)
        svg.add(f'<g transform="translate({ox},{oy})">')
        if vertical:
            gx, gw = w / 2 - 2, 4
            svg.add(rrect(gx, 0.5, gw, h - 1, 2, "#202a34", "#ffffff", 0.6))
            f = (h - 2) * k / max(count - 1, 1)
            if f > 0:
                svg.add(rrect(gx + 0.5, h - 1 - f, gw - 1, f, 1.5, glow))
        else:
            gy, gh = h / 2 - 2, 4
            svg.add(rrect(0.5, gy, w - 1, gh, 2, "#202a34", "#ffffff", 0.6))
            f = (w - 2) * k / max(count - 1, 1)
            if f > 0:
                svg.add(rrect(1, gy + 0.5, f, gh - 1, 1.5, glow))
        svg.add("</g>")
    svg.save(name)


def cornered(w, h, corner, r=7):
    """Path of a w x h rectangle with one rounded corner: tl, tr, bl or br."""
    return {
        "tl": f"M0 {h} V{r} A{r} {r} 0 0 1 {r} 0 H{w} V{h} Z",
        "tr": f"M0 0 H{w - r} A{r} {r} 0 0 1 {w} {r} V{h} H0 Z",
        "bl": f"M0 0 H{w} V{h} H{r} A{r} {r} 0 0 1 0 {h - r} Z",
        "br": f"M0 0 H{w} V{h - r} A{r} {r} 0 0 1 {w - r} {h} H0 Z",
        None: f"M0 0 H{w} V{h} H0 Z",
    }[corner]


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


T = Toml()


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


GLYPH_MENU = f'<path d="M2.5 3.5 h5 M2.5 5 h5 M2.5 6.5 h5" stroke="{INK}" stroke-width="0.9"/>'
GLYPH_MIN = f'<path d="M2.5 6.8 h5" stroke="{INK}" stroke-width="1.3"/>'
GLYPH_SHADE = f'<path d="M2.5 3.2 h5 v3.6 h-5 Z M2.5 4.4 h5" stroke="{INK}" stroke-width="0.9" fill="none"/>'
GLYPH_CLOSE = f'<path d="M3 3 L7 7 M7 3 L3 7" stroke="{INK}" stroke-width="1.3"/>'


def title_buttons(mode, w, close_cmd, shade_cmd=None, menu=True, minimize=False):
    if menu:
        T.el(mode, type="button", rect=[6, 3, 10, 10], **look2("common/menu.svg", 10, 10), menu_press="main",
             cursor="mainmenu", tip="Main Menu")
    x = w - 16
    T.el(mode, type="button", rect=[x, 3, 10, 10], **look2("common/close.svg", 10, 10), click=close_cmd, tip="Close")
    if shade_cmd:
        x -= 12
        T.el(mode, type="button", rect=[x, 3, 10, 10], **look2("common/shade.svg", 10, 10), click=shade_cmd,
             tip="Toggle Windowshade Mode")
    if minimize:
        x -= 12
        T.el(mode, type="button", rect=[x, 3, 10, 10], **look2("common/min.svg", 10, 10), click="minimize", tip="Minimize")


# --- main window -----------------------------------------------------------------
MAIN_W, MAIN_H = 275, 116


def main_window():
    main_art()
    main_toml()
    main_shade()


def main_art():
    W, H = MAIN_W, MAIN_H
    svg = Svg(W, H)
    body(svg, W, H)
    title_strip(svg, W, "POPAMP MODERN")
    panel(svg, 8, 19, 196, 40)
    # Display furniture.
    svg.add(f'<line x1="92" y1="23" x2="92" y2="55" stroke="{LCD_DIM}"/>')
    svg.add(label("KBPS", 113, 49.5, 5.5, LCD_DIM, "start"))
    svg.add(label("KHZ", 152, 49.5, 5.5, LCD_DIM, "start"))
    # Seek groove.
    svg.add(rrect(8.5, 64.5, 258, 7, 3.5, "#28323c", "#ffffff", 0.8))
    # Volume and balance captions.
    svg.add(label("VOLUME", 197, 85, 6, "#59636f"))
    svg.add(label("BAL", 249, 85, 6, "#59636f"))
    svg.add(rrect(158.5, 76.5, 108, 32, 5, "none", "#9aa2ac", 1, ' opacity="0.6"'))
    svg.save("main/bg.svg")

    for name, d, s in (("prev", 22, 9), ("play", 28, 12), ("pause", 22, 9), ("stop", 22, 8), ("next", 22, 9), ("eject", 18, 8)):
        round_button(f"main/{name}.svg", d, name, s)
    pill("main/ontop.svg", 58, 11, "ON TOP")
    fill_frames("main/balance.svg", 18, 10, 1, False)
    for name, text in (("stereo", "STEREO"), ("mono", "MONO")):
        s = Svg(56, 7)
        for i, color in enumerate((LCD_DIM, LCD_TEXT)):
            s.add(label(text, i * 28 + 14, 6, 6.2, color))
        s.save(f"main/{name}.svg")
    controls_art()


def controls_art():
    """Pictures of the main window's controls."""
    common_art()
    for name, text in (("eq", "EQ"), ("pl", "PL"), ("shuffle", "SHUF"), ("repeat", "REP")):
        pill(f"main/{name}.svg", 28, 11, text)
    fill_frames("main/volume.svg", 76, 10, 28, False)
    thumb("main/seek-thumb.svg", 22, 7)
    thumb("main/vol-thumb.svg", 10, 10)
    status_art()


def common_art():
    """Title bar buttons every window uses."""
    for name, glyph in (("menu", GLYPH_MENU), ("min", GLYPH_MIN), ("shade", GLYPH_SHADE), ("close", GLYPH_CLOSE)):
        small_button(f"common/{name}.svg", glyph)


def status_art():
    """Status icons 9 x 9 and stereo/mono lamps."""
    st = Svg(27, 9)
    st.add(f'<path d="M1.5 0.5 L8.5 4.5 L1.5 8.5 Z" fill="{LCD_TEXT}"/>')
    st.add(f'<path d="M10 1 h2.5 v7 h-2.5 Z M14 1 h2.5 v7 h-2.5 Z" fill="{LCD_TEXT}"/>')
    st.add(f'<rect x="19.5" y="1.5" width="6" height="6" fill="{LCD_TEXT}"/>')
    st.save("main/status.svg")


def main_toml():
    W, H = MAIN_W, MAIN_H
    T.lines.append("\n# --- main window ---")
    T.table("[[window]]", key="main", placement="free", empty_press="move")
    T.table("[window.normal]", size=[W, H], see_through=True)
    n = "normal"
    T.el(n, type="image", rect=[0, 0, W, H], normal="main/bg.svg")
    T.el(n, type="drag", rect=[0, 0, W, 14], double="toggle-windowshade", cursor="titlebar")
    title_buttons(n, W, "exit", "toggle-windowshade", minimize=True)
    # Display.
    for i, (flag, x) in enumerate((("playing", 0), ("paused", 9), ("stopped", 18))):
        T.el(n, type="image", rect=[14, 25, 9, 9], normal=part("main/status.svg", x, 0, 9, 9), show=flag)
    T.el(n, type="clock", rect=[26, 21, 62, 18], hide="stopped")
    T.el(n, type="button", rect=[26, 21, 62, 18], press="toggle-time-mode", menu="time")
    T.el(n, type="vis", rect=[13, 40, 76, 16])
    T.el(n, type="button", rect=[13, 40, 76, 16], press="cycle-vis-mode", menu="vis")
    T.el(n, type="marquee", rect=[96, 22, 104, 12], area=[2, 1, 100, 10], double="file-info", cursor="songname",
         tip="Song Title (right-click for options)", menu="song-title")
    T.el(n, type="text", rect=[97, 43, 15, 8], text="kbps")
    T.el(n, type="text", rect=[139, 43, 12, 8], text="khz")
    T.el(n, type="image", rect=[96, 51, 28, 7], normal=part("main/stereo.svg", 0, 0, 28, 7),
         on=part("main/stereo.svg", 28, 0, 28, 7), lit="stereo")
    T.el(n, type="image", rect=[126, 51, 28, 7], normal=part("main/mono.svg", 0, 0, 28, 7),
         on=part("main/mono.svg", 28, 0, 28, 7), lit="mono")
    # Toggles beside the display.
    for (name, flag, cmd, menu), (x, y) in zip(
        (("eq", "eq-window", "toggle-eq-window", None), ("pl", "playlist-window", "toggle-playlist-window", None),
         ("shuffle", "shuffle", "toggle-shuffle", "shuffle"), ("repeat", "repeat", "toggle-repeat", "repeat")),
        ((209, 20), (239, 20), (209, 34), (239, 34)),
    ):
        kv = dict(type="toggle", rect=[x, y, 28, 11], **look4(f"main/{name}.svg", 28, 11), flag=flag, click=cmd)
        if menu:
            kv["menu"] = menu
        T.el(n, **kv)
    T.el(n, type="toggle", rect=[209, 48, 58, 11],
         normal=part("main/ontop.svg", 0, 0, 58, 11), pressed=part("main/ontop.svg", 58, 0, 58, 11),
         on=part("main/ontop.svg", 116, 0, 58, 11), on_pressed=part("main/ontop.svg", 174, 0, 58, 11),
         flag="always-on-top", click="toggle-always-on-top", tip="Toggle Always-On-Top")
    # Seek bar.
    T.el(n, type="slider", rect=[9, 64, 257, 8], value="position", thumb={
        "normal": part("main/seek-thumb.svg", 0, 0, 22, 7), "pressed": part("main/seek-thumb.svg", 22, 0, 22, 7)},
         thumb_len=22, live=False, show="seekable", cursor="posbar", menu="seek")
    # Transport.
    transport = (
        ("prev", 22, 8, {"click": "previous", "shift": "back5s", "ctrl": "start-of-list"}, "previous", "Previous"),
        ("play", 28, 33, {"click": "play", "shift": "open-file", "ctrl": "open-url"}, "play", "Play"),
        ("pause", 22, 64, {"click": "pause"}, "pause", "Pause"),
        ("stop", 22, 89, {"click": "stop", "shift": "stop-fadeout", "ctrl": "stop-after-current"}, "stop", "Stop"),
        ("next", 22, 114, {"click": "next", "shift": "fwd5s", "ctrl": "end-of-list"}, "next", "Next"),
        ("eject", 18, 139, {"click": "open-file", "shift": "open-folder", "ctrl": "open-url"}, "eject", "Open"),
    )
    for name, d, x, act, menu, tip in transport:
        y = 92 - d // 2
        lk = look2(f"main/{name}.svg", d, d)
        T.el(n, type="button", rect=[x, y, d, d], **lk, hit=lk["normal"], **act, menu=menu, tip=tip)
    # Volume and balance.
    T.el(n, type="slider", rect=[164, 89, 76, 10], value="volume",
         track={"file": "main/volume.svg", "first": [0, 0, 76, 10], "step": [0, 10], "count": 28},
         thumb={"normal": part("main/vol-thumb.svg", 0, 0, 10, 10), "pressed": part("main/vol-thumb.svg", 10, 0, 10, 10)},
         thumb_len=10, reset=200, wheel_step=8, cursor="volbal", tip="Volume")
    T.el(n, type="image", rect=[244, 89, 18, 10], normal=part("main/balance.svg", 0, 0, 18, 10))
    T.el(n, type="slider", rect=[244, 89, 18, 10], value="balance",
         thumb={"normal": part("main/vol-thumb.svg", 0, 0, 10, 10), "pressed": part("main/vol-thumb.svg", 10, 0, 10, 10)},
         thumb_len=10, reset=0, detent=16, cursor="volbal", tip="Balance")


def main_shade():
    W = MAIN_W
    s = Svg(W, 16)
    s.add(rrect(0.5, 0.5, W - 1, 15, 6, s.grad(BODY), EDGE))
    s.add(rrect(70, 3, 132, 10, 2, s.grad(LCD), "#3a4450"))
    s.save("main/shade.svg")
    T.table("[window.shade]", size=[W, 16], see_through=True)
    m = "shade"
    T.el(m, type="image", rect=[0, 0, W, 16], normal="main/shade.svg")
    T.el(m, type="drag", rect=[0, 0, W, 16], double="toggle-windowshade", cursor="wsnormal")
    T.el(m, type="text", rect=[73, 4, 126, 8], text="title")
    for i, (name, cmd) in enumerate((("prev", "previous"), ("play", "play"), ("pause", "pause"), ("stop", "stop"), ("next", "next"))):
        T.el(m, type="button", rect=[20 + i * 9, 4, 8, 8], press=cmd)
    T.el(m, type="button", rect=[6, 3, 10, 10], **look2("common/menu.svg", 10, 10), menu_press="main", tip="Main Menu")
    T.el(m, type="button", rect=[W - 40, 3, 10, 10], **look2("common/min.svg", 10, 10), click="minimize", tip="Minimize")
    T.el(m, type="button", rect=[W - 28, 3, 10, 10], **look2("common/shade.svg", 10, 10), click="toggle-windowshade")
    T.el(m, type="button", rect=[W - 16, 3, 10, 10], **look2("common/close.svg", 10, 10), click="exit", tip="Close")
    # Mini transport glyphs on the shade strip.
    for i, name in enumerate(("prev", "play", "pause", "stop", "next")):
        s.add(f'<g transform="translate({20 + i * 9},4)">' + ICONS[name](4, 6).replace(ACCENT, INK) + "</g>")
    s.save("main/shade.svg")


# --- bar ---------------------------------------------------------------------------
# A thin strip across the screen, as high as a classic window shade: the
# transport on the left, the display stretching in the middle, volume and
# toggles on the right. Placed at the bottom of the screen by default.
BAR_H = 14
# The display inside the bar, from its top to its bottom.
LCD_TOP, LCD_BOTTOM = 1, 13
# The stretching part of the display, from its left end to its right end
# counted from the bar's right end.
BAR_SPAN = (210, 180)


def bar_window():
    bar_art()
    bar_toml()


def lcd_piece(name, w, cap=None, groove=False):
    """A slice of the display: the left or right end, or the middle."""
    h = LCD_BOTTOM - LCD_TOP
    s = Svg(w, h)
    r = 2
    path = {
        "left": f"M{w} 0.5 H{r + 0.5} a{r} {r} 0 0 0 -{r} {r} V{h - 0.5 - r} a{r} {r} 0 0 0 {r} {r} H{w}",
        "right": f"M0 0.5 H{w - r - 0.5} a{r} {r} 0 0 1 {r} {r} V{h - 0.5 - r} a{r} {r} 0 0 1 -{r} {r} H0",
        None: f"M0 0.5 H{w} M0 {h - 0.5} H{w}",
    }[cap]
    fill = {"left": f"M{w} 0 H{r} a{r} {r} 0 0 0 -{r} {r} V{h - r} a{r} {r} 0 0 0 {r} {r} H{w} Z",
            "right": f"M0 0 H{w - r} a{r} {r} 0 0 1 {r} {r} V{h - r} a{r} {r} 0 0 1 -{r} {r} H0 Z",
            None: f"M0 0 H{w} V{h} H0 Z"}[cap]
    s.add(f'<path d="{fill}" fill="{s.grad(LCD)}"/>')
    s.add(f'<path d="{path}" fill="none" stroke="#3a4450"/>')
    if groove:
        s.add(f'<rect x="0" y="{h - 3.5}" width="{w}" height="2" fill="#28323c"/>')
    s.save(f"bar/{name}.svg")


def bar_art():
    body = Svg(8, BAR_H)
    body.add(f'<rect width="8" height="{BAR_H}" fill="{body.grad(BODY)}"/>')
    for y, color, opacity in ((0.5, EDGE, 1), (BAR_H - 0.5, EDGE, 1)):
        body.add(f'<line x1="0" y1="{y}" x2="8" y2="{y}" stroke="{color}" opacity="{opacity}"/>')
    body.save("bar/body.svg")
    # The fixed display: play state, time and vis.
    h = LCD_BOTTOM - LCD_TOP
    d = Svg(112, h)
    panel(d, 0.5, 0.5, 111, h - 1, 2)
    d.add(f'<line x1="56.5" y1="2" x2="56.5" y2="{h - 2}" stroke="{LCD_DIM}"/>')
    d.save("bar/display.svg")
    lcd_piece("lcd-left", 4, "left")
    lcd_piece("lcd-tile", 8, groove=True)
    lcd_piece("lcd-right", 4, "right")
    for name, icon in (("prev", 6), ("play", 7), ("pause", 6), ("stop", 5), ("next", 6), ("eject", 6)):
        round_button(f"bar/{name}.svg", 12, name, icon)
    for name, text in (("eq", "EQ"), ("pl", "PL"), ("shuffle", "SHUF"), ("repeat", "REP")):
        pill(f"bar/{name}.svg", 24, 10, text)
    fill_frames("bar/volume.svg", 52, 8, 28, False)
    thumb("bar/seek-thumb.svg", 12, 4)
    thumb("bar/vol-thumb.svg", 8, 8)
    common_art()
    status_art()


def bar_toml():
    H = BAR_H
    top, bottom = LCD_TOP, LCD_BOTTOM
    left, right = BAR_SPAN
    T.lines.append("\n# --- bar: the main window ---")
    T.table("[[window]]", key="main", placement="bar-bottom", empty_press="nothing")
    T.table("[window.normal]", size=[640, H])
    n = "normal"
    T.el(n, type="image", edges=[0, 0, "end", H], normal="bar/body.svg", fill="tile")
    T.el(n, type="button", rect=[3, 2, 10, 10], **look2("common/menu.svg", 10, 10), menu_press="main",
         cursor="mainmenu", tip="Main Menu")
    transport = (
        ("prev", {"click": "previous", "shift": "back5s", "ctrl": "start-of-list"}, "previous", "Previous"),
        ("play", {"click": "play", "shift": "open-file", "ctrl": "open-url"}, "play", "Play"),
        ("pause", {"click": "pause"}, "pause", "Pause"),
        ("stop", {"click": "stop", "shift": "stop-fadeout", "ctrl": "stop-after-current"}, "stop", "Stop"),
        ("next", {"click": "next", "shift": "fwd5s", "ctrl": "end-of-list"}, "next", "Next"),
        ("eject", {"click": "open-file", "shift": "open-folder", "ctrl": "open-url"}, "eject", "Open"),
    )
    for i, (name, act, menu, tip) in enumerate(transport):
        lk = look2(f"bar/{name}.svg", 12, 12)
        T.el(n, type="button", rect=[16 + 13 * i, 1, 12, 12], **lk, hit=lk["normal"], **act, menu=menu, tip=tip)
    # Fixed display.
    T.el(n, type="image", rect=[96, top, 112, bottom - top], normal="bar/display.svg")
    for flag, x in (("playing", 0), ("paused", 9), ("stopped", 18)):
        T.el(n, type="image", rect=[99, 3, 9, 9], normal=part("main/status.svg", x, 0, 9, 9), show=flag)
    T.el(n, type="clock", rect=[110, 2, 42, 10], hide="stopped")
    T.el(n, type="button", rect=[110, 2, 42, 10], press="toggle-time-mode", menu="time")
    T.el(n, type="vis", rect=[155, 3, 50, 8])
    T.el(n, type="button", rect=[155, 3, 50, 8], press="cycle-vis-mode", menu="vis")
    # Stretching display: the title over a thin seek bar.
    T.el(n, type="image", edges=[left, top, left + 4, bottom], normal="bar/lcd-left.svg")
    T.el(n, type="image", edges=[left + 4, top, f"end-{right + 4}", bottom], normal="bar/lcd-tile.svg", fill="tile")
    T.el(n, type="image", edges=[f"end-{right + 4}", top, f"end-{right}", bottom], normal="bar/lcd-right.svg")
    T.el(n, type="marquee", edges=[left + 4, 2, f"end-{right + 4}", 9], double="file-info", cursor="songname",
         tip="Song Title (right-click for options)", menu="song-title")
    T.el(n, type="slider", edges=[left + 3, 9, f"end-{right + 3}", 13], value="position", thumb={
        "normal": part("bar/seek-thumb.svg", 0, 0, 12, 4), "pressed": part("bar/seek-thumb.svg", 12, 0, 12, 4)},
         thumb_len=12, live=False, show="seekable", cursor="posbar", menu="seek")
    # Volume and toggles.
    T.el(n, type="slider", edges=["end-175", 3, "end-123", 11], value="volume",
         track={"file": "bar/volume.svg", "first": [0, 0, 52, 8], "step": [0, 8], "count": 28},
         thumb={"normal": part("bar/vol-thumb.svg", 0, 0, 8, 8), "pressed": part("bar/vol-thumb.svg", 8, 0, 8, 8)},
         thumb_len=8, reset=200, wheel_step=8, cursor="volbal", tip="Volume")
    toggles = (("eq", "eq-window", "toggle-eq-window", None), ("pl", "playlist-window", "toggle-playlist-window", None),
               ("shuffle", "shuffle", "toggle-shuffle", "shuffle"), ("repeat", "repeat", "toggle-repeat", "repeat"))
    for i, (name, flag, cmd, menu) in enumerate(toggles):
        x = 118 - 25 * i
        kv = dict(type="toggle", edges=[f"end-{x}", 2, f"end-{x - 24}", 12], **look4(f"bar/{name}.svg", 24, 10),
                  flag=flag, click=cmd)
        if menu:
            kv["menu"] = menu
        T.el(n, **kv)
    T.el(n, type="button", edges=["end-13", 2, "end-3", 12], **look2("common/close.svg", 10, 10), click="exit", tip="Close")


# --- equalizer -------------------------------------------------------------------
BANDS = ("60", "170", "310", "600", "1K", "3K", "6K", "12K", "14K", "16K")


def eq_window():
    W, H = 275, 116
    svg = Svg(W, H)
    body(svg, W, H)
    title_strip(svg, W, "EQUALIZER")
    panel(svg, 8, 33, 259, 77)
    # Scale and band captions.
    for y, t in ((44, "+12"), (73.5, "0"), (103, "-12")):
        svg.add(label(t, 41, y + 2, 5.5, LCD_TEXT, "end"))
        svg.add(f'<line x1="44" y1="{y}" x2="262" y2="{y}" stroke="{LCD_DIM}" stroke-dasharray="1 2"/>')
    svg.add(label("PRE", 22, 108.5, 5.5, LCD_TEXT))
    for i, b in enumerate(BANDS):
        svg.add(label(b, 58 + i * 20 + 7, 108.5, 5.5, LCD_TEXT))
    svg.save("eq/bg.svg")
    pill("eq/on.svg", 28, 11, "ON")
    pill("eq/auto.svg", 36, 11, "AUTO")
    pill("eq/presets.svg", 52, 11, "PRESETS", states=2)
    fill_frames("eq/band.svg", 14, 62, 28, True)
    thumb("eq/thumb.svg", 14, 7)

    T.lines.append("\n# --- equalizer ---")
    T.table("[[window]]", key="eq", placement="docked", empty_press="move")
    T.table("[window.normal]", size=[W, H], see_through=True, cursor="eqnormal")
    n = "normal"
    T.el(n, type="image", rect=[0, 0, W, H], normal="eq/bg.svg")
    T.el(n, type="drag", rect=[0, 0, W, 14], cursor="eqtitle")
    title_buttons(n, W, "toggle-eq-window", menu=False)
    T.el(n, type="toggle", rect=[9, 19, 28, 11], **look4("eq/on.svg", 28, 11), flag="eq-on", click="toggle-eq")
    T.el(n, type="toggle", rect=[40, 19, 36, 11], **look4("eq/auto.svg", 36, 11), flag="eq-autoload",
         click="toggle-eq-autoload")
    T.el(n, type="eq-graph", rect=[86, 16, 113, 19])
    T.el(n, type="button", rect=[214, 19, 52, 11], **look2("eq/presets.svg", 52, 11), menu_press="eq-presets",
         menu="eq-presets")
    sliders = [("preamp", 15)] + [(f"eq-band-{i + 1}", 58 + i * 20) for i in range(10)]
    for value, x in sliders:
        T.el(n, type="slider", rect=[x, 38, 14, 62], value=value, vertical=True,
             track={"file": "eq/band.svg", "first": [0, 0, 14, 62], "step": [14, 0], "count": 28, "per_row": 28,
                    "map": "falling"},
             thumb={"normal": part("eq/thumb.svg", 0, 0, 14, 7), "pressed": part("eq/thumb.svg", 14, 0, 14, 7)},
             thumb_len=7, reset=31, wheel_step=-2, cursor="eqslid", menu="eq-presets")


# --- playlist --------------------------------------------------------------------
# Top: left cap 25, tile 25, title 100 (centred), right cap 25; all 20 high.


def top_piece(name, w, text=None, left=False, right=False):
    s = Svg(w, 20)
    corner = "tl" if left else "tr" if right else None
    s.add(f'<path d="{cornered(w, 20, corner)}" fill="{s.grad(TITLE)}"/>')
    if left:
        s.add(f'<path d="M0.5 20 V7 a6.5 6.5 0 0 1 6.5 -6.5 H{w}" fill="none" stroke="{EDGE}"/>')
    elif right:
        s.add(f'<path d="M0 0.5 H{w - 7} a6.5 6.5 0 0 1 6.5 6.5 V20" fill="none" stroke="{EDGE}"/>')
    else:
        s.add(f'<line x1="0" y1="0.5" x2="{w}" y2="0.5" stroke="{EDGE}"/>')
    s.add(f'<line x1="0" y1="19.5" x2="{w}" y2="19.5" stroke="#8a929c"/>')
    if text:
        s.add(label(text, w / 2, 13.5, 7.5, INK))
    elif not (left or right):
        for k in (0, 3):
            s.add(f'<line x1="0" y1="{8 + k}" x2="{w}" y2="{8 + k}" stroke="#ffffff" opacity="0.8"/>')
            s.add(f'<line x1="0" y1="{9 + k}" x2="{w}" y2="{9 + k}" stroke="#8a929c"/>')
    s.save(f"pl/{name}.svg")


def side(name, w, groove):
    s = Svg(w, 29)
    s.add(f'<rect x="0" y="0" width="{w}" height="29" fill="{s.grad(BODY[1:], 1, 0)}"/>')
    edge = 0.5 if name == "left" else w - 0.5
    s.add(f'<line x1="{edge}" y1="0" x2="{edge}" y2="29" stroke="{EDGE}"/>')
    if groove:
        s.add(f'<rect x="5" y="0" width="8" height="29" fill="#1d2731"/>')
    s.save(f"pl/{name}.svg")


def bottom(name, w, draw=None, corner=None):
    s = Svg(w, 38)
    s.add(f'<path d="{cornered(w, 38, corner)}" fill="{s.grad(BODY[1:])}"/>')
    x0, x1 = (7 if corner == "bl" else 0), (w - 7 if corner == "br" else w)
    s.add(f'<line x1="{x0}" y1="37.5" x2="{x1}" y2="37.5" stroke="{EDGE}"/>')
    s.add(f'<line x1="0" y1="0.5" x2="{w}" y2="0.5" stroke="#ffffff" opacity="0.7"/>')
    if draw:
        draw(s)
    s.save(f"pl/{name}.svg")


def pills_on(s, items):
    for x, text in items:
        s.add(rrect(x + 0.5, 8.5, 24, 18, 4, s.grad(FACE), RING))
        s.add(label(text, x + 12.5, 20, 6.5, INK))


def bottom_left(s):
    s.add(f'<path d="M0.5 0 V31 a6.5 6.5 0 0 0 6.5 6.5" fill="none" stroke="{EDGE}"/>')
    pills_on(s, ((12, "ADD"), (40, "REM"), (68, "SEL"), (96, "MISC")))


def bottom_right(s):
    s.add(f'<path d="M149.5 0 V31 a6.5 6.5 0 0 1 -6.5 6.5" fill="none" stroke="{EDGE}"/>')
    panel(s, 7, 6, 86, 10, 2)
    panel(s, 68, 21, 30, 10, 2)
    for i, name in enumerate(("prev", "play", "pause", "stop", "next", "eject")):
        s.add(f'<g transform="translate({6 + i * 9},22)">' + ICONS[name](4, 7) + "</g>")
    pills_on(s, ((106, "LIST"),))
    for k in (1, 2, 3):
        s.add(f'<path d="M{147 - 3 * k} 35 L147 {35 - 3 * k}" stroke="{RING}"/>')


# Popup items, 22 x 18: normal and pressed side by side.


def item(name, text):
    s = Svg(44, 18)
    for i, face in enumerate((FACE, FACE_DOWN)):
        s.add(f'<g transform="translate({i * 22},0)">')
        s.add(rrect(0.5, 0.5, 21, 17, 3, s.grad(face), RING))
        s.add(label(text, 11, 12, 6, INK))
        s.add("</g>")
    s.save(f"pl/item-{name}.svg")


PL_MENUS = (
    ("add", 12, [("file", "FILE", {"click": "pl-add-files"}), ("dir", "DIR", {"click": "pl-add-folder"}),
                 ("url", "URL", {"click": "pl-add-url"})]),
    ("rem", 40, [("rmisc", "MISC", {"menu_release": "playlist-remove-misc"}), ("rsel", "SEL", {"click": "pl-remove-selected"}),
                 ("crop", "CROP", {"click": "pl-crop-selected"}), ("rall", "ALL", {"click": "clear-playlist"})]),
    ("sel", 68, [("sall", "ALL", {"click": "select-all"}), ("snone", "NONE", {"click": "select-none"}),
                 ("sinv", "INV", {"click": "invert-selection"})]),
    ("misc", 96, [("mopt", "OPTS", {"menu_release": "playlist-misc"}), ("minfo", "INFO", {"click": "file-info"}),
                   ("msort", "SORT", {"menu_release": "playlist-sort"})]),
    ("list", None, [("lnew", "NEW", {"click": "clear-playlist"}), ("lsave", "SAVE", {"click": "playlist-save"}),
                    ("lload", "LOAD", {"click": "playlist-open"})]),
)


def playlist_window():
    top_piece("top-left", 25, left=True)
    top_piece("top-tile", 25)
    top_piece("title", 100, text="PLAYLIST")
    top_piece("top-right", 25, right=True)
    side("left", 12, False)
    side("right", 20, True)
    bottom("bottom-left", 125, bottom_left, "bl")
    bottom("bottom-tile", 25)
    bottom("bottom-right", 150, bottom_right, "br")
    thumb("pl/scroll-thumb.svg", 8, 18)
    for _, _, items in PL_MENUS:
        for key, text, _ in items:
            item(key, text)
    playlist_toml()


def playlist_toml():
    T.lines.append("\n# --- playlist ---")
    T.table("[[window]]", key="playlist", placement="docked", empty_press="nothing", wheel_scrolls_list=True)
    T.table("[window.normal]", size=[275, 116], resize_step=[25, 29], see_through=True, cursor="pnormal")
    n = "normal"
    T.el(n, type="image", edges=[0, 0, 25, 20], normal="pl/top-left.svg")
    T.el(n, type="image", edges=[25, 0, "end-25", 20], normal="pl/top-tile.svg", fill="tile")
    T.el(n, type="image", edges=["center-50", 0, "center+50", 20], normal="pl/title.svg")
    T.el(n, type="image", edges=["end-25", 0, "end", 20], normal="pl/top-right.svg")
    T.el(n, type="image", edges=[0, 20, 12, "end-38"], normal="pl/left.svg", fill="tile")
    T.el(n, type="image", edges=["end-20", 20, "end", "end-38"], normal="pl/right.svg", fill="tile")
    T.el(n, type="image", edges=[125, "end-38", "end-150", "end"], normal="pl/bottom-tile.svg", fill="tile")
    T.el(n, type="image", edges=[0, "end-38", 125, "end"], normal="pl/bottom-left.svg")
    T.el(n, type="image", edges=["end-150", "end-38", "end", "end"], normal="pl/bottom-right.svg")
    T.el(n, type="drag", edges=[0, 0, "end", 20], double="toggle-pl-shade", cursor="ptbar")
    T.el(n, type="button", edges=["end-17", 5, "end-7", 15], **look2("common/close.svg", 10, 10),
         click="toggle-playlist-window", cursor="pclose", tip="Close")
    T.el(n, type="list", edges=[12, 20, "end-20", "end-38"], menu="playlist-row")
    T.el(n, type="list-scroll", edges=["end-15", 20, "end-7", "end-38"],
         thumb={"normal": part("pl/scroll-thumb.svg", 0, 0, 8, 18), "pressed": part("pl/scroll-thumb.svg", 8, 0, 8, 18)},
         cursor="pvscroll")
    T.el(n, type="text", edges=["end-141", "end-31", "end-59", "end-23"], text="playlist-time")
    T.el(n, type="text", edges=["end-80", "end-16", "end-54", "end-8"], text="playlist-clock", hide="stopped")
    T.el(n, type="resize", edges=["end-12", "end-12", "end", "end"], cursor="psize")
    for i, cmd in enumerate(("previous", "play", "pause", "stop", "next", "open-file")):
        T.el(n, type="button", edges=[f"end-{144 - i * 9}", "end-16", f"end-{136 - i * 9}", "end-8"], click=cmd)
    for name, left, items in PL_MENUS:
        l, r = ((left, left + 25) if left is not None else ("end-44", "end-19"))
        T.el(n, type="popup", edges=[l, "end-30", r, "end-12"], above=True)
        for key, _, act in items:
            T.lines.append("[[window.normal.element.item]]")
            T.lines.append(f'normal = {val(part(f"pl/item-{key}.svg", 0, 0, 22, 18))}')
            T.lines.append(f'pressed = {val(part(f"pl/item-{key}.svg", 22, 0, 22, 18))}')
            for k, v in act.items():
                T.lines.append(f"{k} = {val(v)}")


def main():
    if BAR:
        T.lines.append("# Popamp Bar: Popamp Modern with a bar across the screen for a main window.")
    else:
        T.lines.append("# Popamp Modern: a test skin with round buttons and rounded windows.")
    T.lines.append("# Own artwork; labels are DejaVu Sans Condensed Bold outlines.")
    T.table("[meta]", name="Popamp Bar" if BAR else "Popamp Modern", author="Popamp", format_version=1)
    T.table("[text]", size=7, color=LCD_TEXT)
    T.table("[clock]", size=10 if BAR else 16, color=LCD_TEXT)
    T.table("[playlist]", normal="#9fd8ff", current="#ffffff", normal_bg="#0d1720", selected_bg="#24496f")
    vis = ["#0a131b", "#1b2a36"]
    top, mid, bottom = (0xff, 0xff, 0xff), (0x2f, 0x9b, 0xff), (0x14, 0x3d, 0x73)
    for i in range(16):
        t = i / 15
        a, b, u = (top, mid, t * 2) if t < 0.5 else (mid, bottom, (t - 0.5) * 2)
        vis.append("#%02x%02x%02x" % tuple(round(a[k] + (b[k] - a[k]) * u) for k in range(3)))
    vis += ["#ffffff", "#c8e8ff", "#8fd6ff", "#4fb0f0", "#2f9bff", "#ffffff"]
    T.table("[vis]", colors=vis)
    if BAR:
        bar_window()
    else:
        main_window()
    eq_window()
    playlist_window()
    with open(os.path.join(OUT, "skin.toml"), "w") as f:
        f.write("\n".join(T.lines).lstrip() + "\n")


main()
