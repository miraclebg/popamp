"""Generates Popamp Steel: SVG pictures, pixel fonts and skin.toml.

Usage: python3 -I -B tools/gen_steel_skin.py skins/steel
Needs python3-fonttools and fonts-dejavu-core.
The layout and colours follow the silver-and-navy "modern" players of the
2000s; the artwork, fonts and name are Popamp's own. The window title is
DejaVu Sans Condensed Bold outlines; display texts are own pixel fonts.
"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import pixelfont as pf  # noqa: E402
import skinsvg  # noqa: E402
from skinsvg import Outlines, Svg, Toml, look2, look4, part, rrect  # noqa: E402

skinsvg.OUT = sys.argv[1]
BOLD = Outlines("/usr/share/fonts/truetype/dejavu/DejaVuSansCondensed-Bold.ttf")
T = Toml()

# --- palette -------------------------------------------------------------------
OUTLINE = "#000000"
LCD_BG = "#284a89"
LCD_DOT = "#002263"
LCD_EDGE = "#141822"
LCD_TEXT = "#ffffff"
LCD_DIM = "#6981ad"
INK = "#000000"
LINK = "#406aba"
# Rows of a title bar from its top, without the grip grooves.
TITLE_ROWS = ("#0b1427", "#9ba3b5", "#b3b9c9", "#6b768e", "#596580", "#414f6e", "#364565", "#283859", "#1b2c4c",
              "#172647", "#16264b", "#16274d", "#16284e", "#1c2c51", "#233458", "#354566", "#354566", "#0d1119")
# The silver under the main window's seek bar, from y = 80 down.
LOW_ROWS = ("#bec4cf", "#979eab", "#a8afbb", "#bbc0cb", "#c9cdd7", "#ccd0d9", "#d0d3db", "#ced1da", "#cbcfd8",
            "#c4c9d3", "#b9bfcb", "#b1b8c4", "#a3aab8", "#9198a6", "#838a98", "#808796", "#808896", "#808896",
            "#7e8694", "#777e8d", "#727b8a", "#707988", "#6c7482", "#666e7c", "#666e7c", "#636b79", "#5d6572",
            "#5d6572", "#59616e", "#575f6c", "#5b6370", "#626a78", "#6d7483", "#7c8392", "#636b79")
# The lighter panel at the main window's lower right, from y = 86 down.
POD_ROWS = ("#ffffff", "#ececef", "#efeff2", "#f2f2f5", "#f2f2f5", "#efeff2", "#e8e9ed", "#e2e3e9", "#dbdde4",
            "#d7d9e0", "#d2d6de", "#d4d6de", "#d5d7df", "#d5d8e0", "#d2d6de", "#d1d4dc", "#cfd2db", "#c9cdd7",
            "#c4c9d3", "#c1c6d1", "#bec4cf", "#bac0cc", "#b9bfcb", "#c3c8d3", "#d2d5dd", "#e2e3e9", "#f4f4f6",
            "#f3f3f6", "#9ea5b2")
FACE = ("#ffffff", "#f4f5f8", "#d3d7df", "#9aa1ae")
FACE_DOWN = ("#9aa1ae", "#c3c8d1", "#e6e8ed", "#f4f5f8")
RING = "#3b414c"


# --- drawing helpers ----------------------------------------------------------------
def bands(svg, x, y, w, colors):
    """One-pixel rows of `colors` from y down."""
    for i, c in enumerate(colors):
        svg.add(f'<rect x="{x}" y="{y + i}" width="{w}" height="1" fill="{c}"/>')


def columns(svg, x, y, h, colors):
    for i, c in enumerate(colors):
        svg.add(f'<rect x="{x + i}" y="{y}" width="1" height="{h}" fill="{c}"/>')


def clip(svg, d):
    svg.n += 1
    svg.defs.append(f'<clipPath id="c{svg.n}"><path d="{d}"/></clipPath>')
    return f'clip-path="url(#c{svg.n})"'


def dots(svg, ox=0, oy=0):
    """The display's dot grid: a dark dot every second pixel both ways."""
    svg.n += 1
    svg.defs.append(f'<pattern id="p{svg.n}" x="{ox}" y="{oy}" width="2" height="2" patternUnits="userSpaceOnUse">'
                    f'<rect width="1" height="1" fill="{LCD_DOT}"/></pattern>')
    return f"url(#p{svg.n})"


def lcd(svg, d, ox=0, oy=0):
    """A dotted blue display inside the outline `d`, lit from its edges."""
    svg.add(f'<path d="{d}" fill="none" stroke="{LCD_EDGE}" stroke-width="2"/>')
    svg.add(f'<path d="{d}" fill="{LCD_BG}"/>')
    svg.add(f'<path d="{d}" fill="{dots(svg, ox, oy)}"/>')
    inside = clip(svg, d)
    svg.add(f"<g {inside}>")
    for width, color, opacity in ((16, "#7d95c8", 0.18), (10, "#7d95c8", 0.25), (6, "#8aa0cc", 0.45),
                                  (4, "#8095c0", 0.7), (2, "#c6d4f0", 0.9)):
        svg.add(f'<path d="{d}" fill="none" stroke="{color}" stroke-width="{width}" opacity="{opacity}"/>')
    svg.add("</g>")


def grip(svg, x0, x1, y):
    """A grip groove across a title bar, five pixels high."""
    svg.add(rrect(x0, y, x1 - x0, 5, 2.5, OUTLINE))
    svg.add(rrect(x0 + 1, y + 1, x1 - x0 - 2, 3, 1.5, svg.grad(("#f6f6f8", "#9ca3b0", "#c2c7d2"))))


def title_text(svg, text, cx, base, size):
    """Embossed silver capitals, baseline at `base`."""
    svg.add(BOLD.label(text, cx + 1, base + 1, size, OUTLINE))
    svg.add(BOLD.label(text, cx, base, size, svg.grad(("#ffffff", "#e4e6ec", "#9ca3b0", "#c9cdd6"))))


def disc(svg, cx, cy, rx, ry, face, ring=RING):
    """A domed button face in a dark ring with a soft shadow round it."""
    svg.add(f'<ellipse cx="{cx}" cy="{cy + 0.5}" rx="{rx + 1.8}" ry="{ry + 1.8}" fill="#5c6472" opacity="0.3"/>')
    svg.add(f'<ellipse cx="{cx}" cy="{cy}" rx="{rx + 0.5}" ry="{ry + 0.5}" fill="{svg.grad(face)}" stroke="{ring}" '
            f'stroke-width="1.3"/>')
    svg.add(f'<ellipse cx="{cx}" cy="{cy - ry * 0.35}" rx="{rx * 0.62}" ry="{ry * 0.4}" fill="#ffffff" opacity="0.55"/>')


# --- icons, drawn round (cx, cy) --------------------------------------------------------
def tri(cx, cy, s, left=False):
    k = -1 if left else 1
    return f"M{cx - k * s * 0.45} {cy - s * 0.55} L{cx + k * s * 0.55} {cy} L{cx - k * s * 0.45} {cy + s * 0.55} Z"


ICONS = {
    "prev": lambda c, d, s: f'<path d="{tri(c - s * .28, d, s * .6, True)} {tri(c + s * .28, d, s * .6, True)}" fill="{INK}"/>',
    "play": lambda c, d, s: f'<path d="{tri(c + s * .08, d, s)}" fill="{INK}"/>',
    "pause": lambda c, d, s: f'<path d="M{c - s * .4} {d - s * .5} h{s * .28} v{s} h{-s * .28} Z '
                             f'M{c + s * .12} {d - s * .5} h{s * .28} v{s} h{-s * .28} Z" fill="{INK}"/>',
    "stop": lambda c, d, s: f'<rect x="{c - s * .42}" y="{d - s * .42}" width="{s * .84}" height="{s * .84}" fill="{INK}"/>',
    "next": lambda c, d, s: f'<path d="{tri(c - s * .28, d, s * .6)} {tri(c + s * .28, d, s * .6)}" fill="{INK}"/>',
    "eject": lambda c, d, s: f'<path d="M{c - s * .45} {d + s * .05} L{c} {d - s * .45} L{c + s * .45} {d + s * .05} Z '
                             f'M{c - s * .45} {d + s * .22} h{s * .9} v{s * .2} h{-s * .9} Z" fill="{INK}"/>',
}


# --- buttons --------------------------------------------------------------------------------
def round_button(name, d, icon, s):
    """normal, pressed and the hit disc side by side, each d x d."""
    svg = Svg(3 * d, d)
    c, r = d / 2, d / 2 - 2.5
    for i, face in enumerate((FACE, FACE_DOWN)):
        svg.add(f'<g transform="translate({i * d},0)">')
        disc(svg, c, c, r, r, face)
        svg.add(ICONS[icon](c + 0.5 * i, c + 0.5 * i, s))
        svg.add("</g>")
    svg.add(f'<circle cx="{2 * d + c}" cy="{c}" r="{r + 1}" fill="#000000"/>')
    svg.save(name)


def box_button(name, glyph, w=8, h=8, faces=("#f1f1f5", "#b5bbc5"), border=OUTLINE):
    """Small square title bar buttons, normal and pressed."""
    svg = Svg(2 * w, h)
    for i, face in enumerate((faces, faces[::-1])):
        svg.add(f'<g transform="translate({i * w},0)">')
        svg.add(f'<rect x="0.5" y="0.5" width="{w - 1}" height="{h - 1}" fill="{svg.grad(face)}" stroke="{border}"/>')
        svg.add(glyph)
        svg.add("</g>")
    svg.save(name)


def pill(name, w, h, text, faces, text_colors, on_faces=None, states=4, border=OUTLINE):
    """A rounded caption button: normal, pressed, on, on_pressed."""
    svg = Svg(w * states, h)
    for i in range(states):
        on, down = i >= 2, i in (1, 3)
        face = (on_faces if on else faces)
        svg.add(f'<g transform="translate({i * w},0)">')
        svg.add(rrect(0.5, 0.5, w - 1, h - 1, (h - 1) / 2, svg.grad(face[::-1] if down else face), border))
        svg.add(rrect(2, 1.5, w - 4, h / 2 - 1.5, h / 4, "#ffffff", extra=' opacity="0.25"'))
        svg.add(pf.svg_text(pf.DISPLAY, text, w / 2 + (0.5 if down else 0), (h - 6) // 2 + (1 if down else 0),
                            text_colors[1 if on else 0], anchor="middle"))
        svg.add("</g>")
    svg.save(name)


def thumb(name, w, h):
    svg = Svg(2 * w, h)
    for i, face in enumerate((FACE, FACE_DOWN)):
        svg.add(f'<g transform="translate({i * w},0)">')
        svg.add(rrect(0.75, 0.75, w - 1.5, h - 1.5, min(w, h) / 2 - 0.75, svg.grad(face), OUTLINE, 1.5))
        svg.add("</g>")
    svg.save(name)


GLYPH_MENU = f'<path d="M2 2h4v1h-4z M2 4h4v1h-4z M2 6h4v1h-4z" fill="{INK}"/>'
GLYPH_MIN = f'<rect x="2" y="4" width="4" height="2" fill="{INK}"/>'
GLYPH_SHADE = f'<rect x="2" y="2" width="4" height="2" fill="{INK}"/><rect x="2" y="5" width="4" height="1" fill="{INK}"/>'
GLYPH_CLOSE = f'<path d="M2 2 L6 6 M6 2 L2 6" stroke="{INK}" stroke-width="1.2"/>'


# --- main window ------------------------------------------------------------------------------
W, H = 275, 116
# The display, its edge between the outline and the dots.
MAIN_LCD = ("M5 28 A6 6 0 0 1 11 22 H264 A6 6 0 0 1 270 28 V49 A5 5 0 0 1 265 54 H115 "
            "C109 54 106.5 56.5 105 60.5 L103.8 64 C102.8 67 101 68 98 68 H9 A4 4 0 0 1 5 64 Z")
WINDOW = f"M0 4 A4 4 0 0 1 4 0 H{W - 4} A4 4 0 0 1 {W} 4 V{H - 4} A4 4 0 0 1 {W - 4} {H} H4 A4 4 0 0 1 0 {H - 4} Z"
# Where the transport buttons' centres are, and their size.
TRANSPORT_Y, TRANSPORT_D = 95, 23
TRANSPORT = (
    ("prev", 28, 6, {"click": "previous", "shift": "back5s", "ctrl": "start-of-list"}, "previous", "Previous"),
    ("play", 50, 7, {"click": "play", "shift": "open-file", "ctrl": "open-url"}, "play", "Play"),
    ("pause", 73, 6, {"click": "pause"}, "pause", "Pause"),
    ("stop", 96, 5.5, {"click": "stop", "shift": "stop-fadeout", "ctrl": "stop-after-current"}, "stop", "Stop"),
    ("next", 118, 6, {"click": "next", "shift": "fwd5s", "ctrl": "end-of-list"}, "next", "Next"),
    ("eject", 142, 6.5, {"click": "open-file", "shift": "open-folder", "ctrl": "open-url"}, "eject", "Open"),
)


def titled_grips(svg, text, cx, x0, x1, size, base=13):
    """The title lettering centred on cx with grips from x0 and to x1."""
    half = BOLD.text_path(text, size)[1] / 2
    grip(svg, x0, cx - half - 7, 6)
    grip(svg, cx + half + 7, x1, 6)
    title_text(svg, text, cx, base, size)


def main_art():
    svg = Svg(W, H)
    svg.add(f"<g {clip(svg, WINDOW)}>")
    bands(svg, 0, 0, W, TITLE_ROWS)
    titled_grips(svg, "POPAMP", 134, 19, 239, 13)
    # Silver round the display, the seek bar and the transport.
    bands(svg, 0, 18, W, ("#d4d7df", "#afb6c2", "#6d7685"))
    svg.add(f'<rect x="0" y="21" width="{W}" height="59" fill="{svg.grad(("#e8e9ee", "#f1f1f4", "#d6d9e0", "#c0c5d0"))}"/>')
    bands(svg, 0, 80, W, LOW_ROWS)
    lcd(svg, MAIN_LCD)
    # Seek groove.
    svg.add(rrect(14, 73, 252, 7, 3, svg.grad(("#7a8290", "#aab0bc", "#000000", "#616976", "#8d93a2", "#cfd2db"))))
    svg.add(f'<rect x="16" y="79" width="248" height="1" fill="#ffffff"/>')
    # The lighter panel under the toggles, its top left corner sloping in.
    pod = "M275 85 H170 Q162 85 160 92 L153 116 H275 Z"
    svg.add(f"<g {clip(svg, pod)}>")
    bands(svg, 150, 85, 125, ("#0f151e",) + POD_ROWS)
    svg.add("</g>")
    svg.add(f'<path d="M275 85.5 H170 Q162.5 85.5 160.5 92 L153.5 116" fill="none" stroke="#0f151e"/>')
    svg.add('<path d="M276 86.5 H170 Q163.5 86.5 161.5 93 L154.5 116" fill="none" stroke="#ffffff" opacity="0.8"/>')
    # Volume and balance grooves; the volume's fill comes with its track.
    svg.add(rrect(176, 63, 39, 4, 2, "#000000"))
    svg.add(rrect(177, 65, 37, 2, 1, "#aab1be"))
    # The raised oval holding EQ and PL.
    svg.add(rrect(221.5, 57.5, 42, 16, 8, svg.grad(("#ffffff", "#e0e2e7", "#9ea5b2")), "#727b8a"))
    # Edges of the window body.
    columns(svg, 1, 18, H - 19, ("#9ca3b0", "#bbc1cc", "#959daa"))
    columns(svg, W - 4, 18, H - 19, ("#8b93a0", "#b9bfcb", "#dfe0e6"))
    # Display furniture: level ticks, captions.
    for y in (27, 35, 43, 51, 59):
        svg.add(f'<rect x="13" y="{y}" width="3" height="1" fill="{LCD_TEXT}"/>')
    svg.add(pf.svg_text(pf.TINY, "KBPS", 131, 44, LCD_TEXT))
    svg.add(pf.svg_text(pf.TINY, "KHZ", 171, 44, LCD_TEXT))
    # Resize-style grip at the lower right corner.
    for k in range(4):
        svg.add(f'<path d="M{259 + 3 * k} 113 L{271} {101 + 3 * k}" stroke="#4c5361" stroke-width="1.2"/>')
    svg.add("</g>")
    svg.add(f'<path d="M0.5 4.5 A4 4 0 0 1 4.5 0.5 H{W - 4.5} A4 4 0 0 1 {W - 0.5} 4.5 V{H - 4.5} A4 4 0 0 1 '
            f'{W - 4.5} {H - 0.5} H4.5 A4 4 0 0 1 0.5 {H - 4.5} Z" fill="none" stroke="{OUTLINE}"/>')
    svg.save("main/bg.svg")

    for name, _, s, _, _, _ in TRANSPORT:
        round_button(f"main/{name}.svg", TRANSPORT_D, name, s)
    for name, glyph in (("menu", GLYPH_MENU), ("min", GLYPH_MIN), ("shade", GLYPH_SHADE), ("close", GLYPH_CLOSE)):
        box_button(f"common/{name}.svg", glyph)
    blue = ("#ffffff", "#eef0f4", "#c9ced8")
    lit = ("#e6efff", "#c4d6f7", "#93acdc")
    for name, text in (("eq", "EQ"), ("pl", "PL")):
        pill(f"main/{name}.svg", 15, 11, text, blue, (LINK, LINK), lit, border=RING)
    for name, text in (("stereo", "STEREO"), ("mono", "MONO")):
        w = pf.width(pf.TINY, text)
        s = Svg(2 * w, 5)
        for i, color in enumerate((LCD_DIM, LCD_TEXT)):
            s.add(pf.svg_text(pf.TINY, text, i * w, 0, color))
        s.save(f"main/{name}.svg")
    status_art()
    volume_art()
    thumb("main/seek-thumb.svg", 28, 9)
    thumb("main/vol-thumb.svg", 13, 8)
    toggle_lamp("main/shuffle.svg", 33, "shuffle")
    toggle_lamp("main/repeat.svg", 30, "repeat")


def status_art():
    """Playing, paused and stopped, 12 x 9 each: a lamp and the state."""
    s = Svg(36, 9)
    for i, glyph in enumerate((
        "M8 3 h1 v5 h-1 z M9 4 h1 v3 h-1 z M10 5 h1 v1 h-1 z",
        "M8 3 h1 v5 h-1 z M10 3 h1 v5 h-1 z",
        "M8 4 h3 v3 h-3 z",
    )):
        s.add(f'<g transform="translate({i * 12},0)"><rect x="2" y="1" width="3" height="3" fill="{LCD_TEXT}"/>'
              f'<path d="{glyph}" fill="{LCD_TEXT}"/></g>')
    s.save("main/status.svg")


VOLUME_W, VOLUME_H, VOLUME_FRAMES = 66, 8, 28


def volume_art():
    """The volume groove, filled blue up to the thumb, one frame per step."""
    s = Svg(VOLUME_W, VOLUME_H * VOLUME_FRAMES)
    fill = s.grad(("#2a416f", "#aac4f5", "#5681d4"))
    for k in range(VOLUME_FRAMES):
        y = k * VOLUME_H
        s.add(rrect(0, y + 2, VOLUME_W, 4, 2, "#3c424e"))
        s.add(rrect(1, y + 4, VOLUME_W - 2, 2, 1, "#aab1be"))
        f = 6.5 + (VOLUME_W - 13) * k / (VOLUME_FRAMES - 1)
        s.add(f'<rect x="0" y="{y + 2}" width="{f:.2f}" height="4" rx="2" fill="{fill}"/>')
    s.save("main/volume.svg")


def toggle_lamp(name, w, icon):
    """A round button with a blue lamp to its right: four states."""
    s = Svg(4 * w, 22)
    for i in range(4):
        on, down = i >= 2, i in (1, 3)
        s.add(f'<g transform="translate({i * w},0)">')
        disc(s, 10.5, 11, 8, 8, FACE_DOWN if down else FACE)
        if icon == "shuffle":
            dx = 0.5 if down else 0
            for digit, x, y in (("1", 5, 8), ("2", 8.5, 11), ("3", 12.5, 8)):
                s.add(pf.svg_text(pf.DISPLAY, digit, x + dx, y, INK))
            s.add(f'<path d="M{6 + dx} 6.5 Q10.5 3.5 {14.5 + dx} 6" stroke="#000000" fill="none"/>'
                  f'<path d="M{13 + dx} 4.5 L{15.5 + dx} 6 L{13.5 + dx} 7.5" stroke="#000000" stroke-width="0.8" fill="none"/>')
        else:
            s.add('<path d="M14 9.5 A4 4 0 1 0 12 15" stroke="#000000" stroke-width="1.3" fill="none"/>'
                  '<path d="M10 13 L12.5 15.5 L10 17.5" stroke="#000000" stroke-width="1.2" fill="none"/>')
        lx = w - 6
        if on:
            s.add(f'<circle cx="{lx}" cy="11" r="5" fill="#6e9cf5" opacity="0.35"/>')
            s.add(f'<circle cx="{lx}" cy="11" r="3.2" fill="{s.grad(("#b0d6ff", "#4573d6", "#3753d2"))}"/>')
            s.add(f'<circle cx="{lx - 0.8}" cy="10" r="1.1" fill="#ffffff" opacity="0.9"/>')
        else:
            s.add(f'<circle cx="{lx}" cy="11" r="3.2" fill="{s.grad(("#c7cfe0", "#8c99b5"))}" stroke="#7d879c" '
                  f'stroke-width="0.6"/>')
        s.add("</g>")
    s.save(name)


def main_toml():
    T.lines.append("\n# --- main window ---")
    T.table("[[window]]", key="main", placement="free", empty_press="move")
    T.table("[window.normal]", size=[W, H], see_through=True)
    n = "normal"
    T.el(n, type="image", rect=[0, 0, W, H], normal="main/bg.svg")
    T.el(n, type="drag", rect=[0, 0, W, 18], double="toggle-windowshade", cursor="titlebar")
    T.el(n, type="button", rect=[7, 5, 8, 8], **look2("common/menu.svg", 8, 8), menu_press="main", cursor="mainmenu",
         tip="Main Menu")
    for x, name, cmd, tip in ((245, "min", "minimize", "Minimize"), (255, "shade", "toggle-windowshade",
                               "Toggle Windowshade Mode"), (265, "close", "exit", "Close")):
        T.el(n, type="button", rect=[x, 5, 8, 8], **look2(f"common/{name}.svg", 8, 8), click=cmd, tip=tip)
    # Display.
    for i, flag in enumerate(("playing", "paused", "stopped")):
        T.el(n, type="image", rect=[22, 26, 12, 9], normal=part("main/status.svg", 12 * i, 0, 12, 9), show=flag)
    T.el(n, type="clock", rect=[36, 26, 64, 13], hide="stopped")
    T.el(n, type="button", rect=[36, 26, 64, 13], press="toggle-time-mode", menu="time")
    T.el(n, type="vis", rect=[23, 43, 77, 16])
    T.el(n, type="button", rect=[23, 43, 77, 16], press="cycle-vis-mode", menu="vis")
    T.el(n, type="marquee", rect=[111, 25, 152, 11], area=[2, 2, 148, 7], double="file-info", cursor="songname",
         tip="Song Title (right-click for options)", menu="song-title")
    T.el(n, type="text", rect=[113, 43, 15, 7], text="kbps")
    T.el(n, type="text", rect=[155, 43, 10, 7], text="khz")
    for name, x in (("mono", 217), ("stereo", 241)):
        w = pf.width(pf.TINY, name.upper())
        T.el(n, type="image", rect=[x, 45, w, 5], normal=part(f"main/{name}.svg", 0, 0, w, 5),
             on=part(f"main/{name}.svg", w, 0, w, 5), lit=name)
    # Volume, balance, EQ and PL.
    T.el(n, type="slider", rect=[108, 61, VOLUME_W, VOLUME_H], value="volume",
         track={"file": "main/volume.svg", "first": [0, 0, VOLUME_W, VOLUME_H], "step": [0, VOLUME_H],
                "count": VOLUME_FRAMES},
         thumb=look2("main/vol-thumb.svg", 13, 8), thumb_len=13, reset=200, wheel_step=8, cursor="volbal",
         tip="Volume")
    T.el(n, type="slider", rect=[176, 61, 39, 8], value="balance", thumb=look2("main/vol-thumb.svg", 13, 8),
         thumb_len=13, reset=0, detent=16, cursor="volbal", tip="Balance")
    for name, x, flag, cmd, tip in (("eq", 226, "eq-window", "toggle-eq-window", "Toggle Equalizer"),
                                    ("pl", 245, "playlist-window", "toggle-playlist-window", "Toggle Playlist Editor")):
        T.el(n, type="toggle", rect=[x, 60, 15, 11], **look4(f"main/{name}.svg", 15, 11), flag=flag, click=cmd, tip=tip)
    # Seek bar.
    T.el(n, type="slider", rect=[15, 74, 250, 9], value="position", thumb=look2("main/seek-thumb.svg", 28, 9),
         thumb_len=28, live=False, show="seekable", cursor="posbar", menu="seek")
    # Transport.
    d = TRANSPORT_D
    for name, cx, _, act, menu, tip in TRANSPORT:
        x, y = cx - d // 2, TRANSPORT_Y - d // 2
        T.el(n, type="button", rect=[x, y, d, d], **look2(f"main/{name}.svg", d, d),
             hit=part(f"main/{name}.svg", 2 * d, 0, d, d), **act, menu=menu, tip=tip)
    for name, x, w, flag, cmd, tip in (("shuffle", 171, 33, "shuffle", "toggle-shuffle", "Toggle Shuffle"),
                                       ("repeat", 209, 30, "repeat", "toggle-repeat", "Toggle Repeat")):
        T.el(n, type="toggle", rect=[x, 85, w, 22], **look4(f"main/{name}.svg", w, 22), flag=flag, click=cmd,
             menu=name, tip=tip)


SHADE_H = 14


def shade_strip(name, lcd_span=None, caption=None, transport=False):
    """A window's shade mode: a navy strip with grips, and a small display
    from lcd_span[0] to lcd_span[1] or a caption."""
    s = Svg(W, SHADE_H)
    d = f"M0 3 A3 3 0 0 1 3 0 H{W - 3} A3 3 0 0 1 {W} 3 V{SHADE_H - 3} A3 3 0 0 1 {W - 3} {SHADE_H} H3 A3 3 0 0 1 0 {SHADE_H - 3} Z"
    s.add(f"<g {clip(s, d)}>")
    bands(s, 0, 0, W, TITLE_ROWS[:4] + TITLE_ROWS[-10:])
    if lcd_span:
        x0, x1 = lcd_span
        if x0 > 40:
            grip(s, 19, x0 - 4, 5)
        lcd(s, f"M{x0} 2.5 H{x1} V11.5 H{x0} Z")
    else:
        titled_grips(s, caption, 130, 6, 237, 10, base=11)
    if transport:
        for i, icon in enumerate(("prev", "play", "pause", "stop", "next")):
            s.add(ICONS[icon](206 + i * 7.5, 7, 5).replace(INK, "#d6dbe6"))
    s.add("</g>")
    s.add(f'<path d="{d}" fill="none" stroke="{OUTLINE}"/>')
    s.save(name)


def main_shade():
    shade_strip("main/shade-strip.svg", (70, 200), transport=True)
    T.table("[window.shade]", size=[W, SHADE_H], see_through=True)
    m = "shade"
    T.el(m, type="image", rect=[0, 0, W, SHADE_H], normal="main/shade-strip.svg")
    T.el(m, type="drag", rect=[0, 0, W, SHADE_H], double="toggle-windowshade", cursor="wsnormal")
    T.el(m, type="text", rect=[73, 4, 124, 7], text="title")
    for i, cmd in enumerate(("previous", "play", "pause", "stop", "next")):
        T.el(m, type="button", rect=[203 + i * 7, 3, 7, 8], press=cmd)
    T.el(m, type="button", rect=[7, 3, 8, 8], **look2("common/menu.svg", 8, 8), menu_press="main", tip="Main Menu")
    for x, name, cmd in ((245, "min", "minimize"), (255, "shade", "toggle-windowshade"), (265, "close", "exit")):
        T.el(m, type="button", rect=[x, 3, 8, 8], **look2(f"common/{name}.svg", 8, 8), click=cmd)


# --- equalizer ---------------------------------------------------------------------------------
BANDS = ("60", "170", "310", "600", "1K", "3K", "6K", "12K", "14K", "16K")
PREAMP_X = 26
BAND_X = [84 + 18 * i for i in range(10)]
# The sliders' thumbs run between these centres: +12 dB, 0 dB, -12 dB.
EQ_TOP, EQ_MID, EQ_LOW = 42, 68, 95
EQ_PANEL = ("M21.5 17.5 L28.5 5.5 H87.5 L94.5 17.5 H258.5 A4 4 0 0 1 262.5 21.5 V108.5 A3 3 0 0 1 259.5 111.5 "
            "H12.5 A4 4 0 0 1 8.5 107.5 V24 A6.5 6.5 0 0 1 15 17.5 Z")
GRAPH = (82, 18)


def eq_art():
    svg = Svg(W, H)
    svg.add(f"<g {clip(svg, WINDOW)}>")
    svg.add(f'<rect width="{W}" height="{H}" fill="{svg.grad(("#e4e4ea", "#c2c7d2", "#bbc1cc", "#c8ccd6"))}"/>')
    columns(svg, 1, 0, H, ("#aab1bd", "#f0f0f3", "#d9dbe2"))
    columns(svg, W - 4, 0, H, ("#ced1da", "#f0f0f3", "#b2b8c4"))
    bands(svg, 0, 1, W, ("#989eac", "#f6f6f8"))
    svg.add(f'<path d="{EQ_PANEL}" fill="{svg.grad(("#f6f6f8", "#eeeef1", "#c8ccd6", "#bdc3ce"))}" stroke="{OUTLINE}"/>')
    # Tab caption.
    svg.add(pf.svg_text(pf.DISPLAY, "EQUALIZER", 58, 9, "#1e232d", gap=2, anchor="middle"))
    # Shading below 0 dB, the scale and the sliders' grooves.
    svg.add(f'<rect x="10" y="{EQ_MID}" width="252" height="{101 - EQ_MID}" fill="#8a929f" opacity="0.12"/>')
    db_scale(svg)
    for i, cx in enumerate([PREAMP_X] + BAND_X):
        groove(svg, cx)
        if i >= 1 and i < 10:
            dotted(svg, cx + 9)
    # Captions under the sliders.
    bar = "#858d9b"
    svg.add(rrect(12, 103, 30, 8, 0, bar))
    svg.add(rrect(43, 103, 33, 8, 0, bar))
    svg.add(pf.svg_text(pf.DISPLAY, "PREAMP", 27, 104, "#ffffff", anchor="middle"))
    for cx, text in zip(BAND_X, BANDS):
        svg.add(rrect(cx - 7, 103, 16, 8, 0, bar))
        svg.add(pf.svg_text(pf.DISPLAY, text, cx + 1, 104, "#ffffff", anchor="middle"))
    svg.add("</g>")
    svg.add(f'<path d="M0.5 4.5 A4 4 0 0 1 4.5 0.5 H{W - 4.5} A4 4 0 0 1 {W - 0.5} 4.5 V{H - 4.5} A4 4 0 0 1 '
            f'{W - 4.5} {H - 0.5} H4.5 A4 4 0 0 1 0.5 {H - 4.5} Z" fill="none" stroke="{OUTLINE}"/>')
    svg.save("eq/bg.svg")

    navy = ("#5b74a6", "#3d5588", "#284173")
    bright = ("#7d9ad4", "#4f6fb6", "#2f4f96")
    pill("eq/on.svg", 25, 13, "ON", navy, ("#c5cede", "#ffffff"), bright)
    pill("eq/auto.svg", 34, 13, "AUTO", navy, ("#c5cede", "#ffffff"), bright)
    pill("eq/presets.svg", 44, 13, "PRESETS", ("#ffffff", "#f2f2f5", "#d3d6de"), (INK, INK), states=2)
    knob("eq/thumb.svg")
    for name, glyph in (("close", GLYPH_CLOSE), ("shade", GLYPH_MIN)):
        box_button(f"eq/{name}.svg", glyph.replace(INK, "#7f8794"), faces=("#ffffff00", "#ffffff00"), border="none")
    shade_strip("eq/shade-strip.svg", caption="EQUALIZER")
    graph_sheet()


def db_scale(svg):
    ink = "#2c313c"
    for x in (38, 71):
        for y in range(EQ_TOP + 2, EQ_LOW, 2):
            svg.add(f'<rect x="{x}" y="{y}" width="2" height="1" fill="{ink}"/>')
    for y, text in ((EQ_TOP, "+12"), (EQ_MID, "+0"), (EQ_LOW, "-12")):
        svg.add(f'<rect x="37" y="{y}" width="6" height="1" fill="{ink}"/><rect x="69" y="{y}" width="6" height="1" fill="{ink}"/>')
        w = pf.width(pf.DISPLAY, text) + 3 + pf.width(pf.TINY, "DB")
        x = round(56.5 - w / 2)
        svg.add(pf.svg_text(pf.DISPLAY, text, x, y - 3, ink))
        svg.add(pf.svg_text(pf.TINY, "DB", x + w - pf.width(pf.TINY, "DB"), y - 2, ink))


def groove(svg, cx):
    """A slider's dark groove with a black line down its middle."""
    svg.add(rrect(cx - 4.5, EQ_TOP - 4.5, 9, 105.5 - EQ_TOP, 4.5, svg.grad(("#1e232d", "#4c5360", "#5a626f", "#4f5663", "#9ea5b2", "#a5acb9", "#d0d4dc"), 1, 0),
                  "#9ea5b2", 0.6))
    svg.add(f'<rect x="{cx + 4}" y="{EQ_TOP - 3}" width="1" height="{EQ_LOW - EQ_TOP + 6}" fill="#fefefe"/>')
    svg.add(f'<rect x="{cx}" y="{EQ_TOP}" width="1" height="{EQ_LOW - EQ_TOP}" fill="#000000"/>')


def dotted(svg, x):
    for y in range(EQ_TOP - 2, EQ_LOW + 4, 2):
        svg.add(f'<rect x="{x}" y="{y}" width="1" height="1" fill="#2c313c"/>')
    svg.add(f'<rect x="{x - 1}" y="{EQ_MID}" width="3" height="1" fill="#2c313c"/>')


def knob(name, w=11, h=10):
    s = Svg(2 * w, h)
    for i, face in enumerate((FACE, FACE_DOWN)):
        s.add(f'<g transform="translate({i * w},0)">')
        s.add(f'<ellipse cx="{w / 2}" cy="{h / 2}" rx="{w / 2 - 0.75}" ry="{h / 2 - 0.75}" fill="{s.grad(face)}" '
              f'stroke="{OUTLINE}" stroke-width="1.5"/>')
        s.add(f'<ellipse cx="{w / 2 - 1}" cy="{h / 2 - 1.5}" rx="2.5" ry="1.8" fill="#ffffff" opacity="0.8"/>')
        s.add("</g>")
    s.save(name)


def graph_sheet():
    """The equalizer sheet, only for its curve: background, colours and
    preamp line where the classic sheet keeps them."""
    s = Svg(275, 315)
    s.add('<rect x="0" y="294" width="113" height="19" fill="#eceef3"/>')
    for x in range(1, 113, 12):
        s.add(f'<rect x="{x}" y="294" width="1" height="19" fill="#d5d8e0"/>')
    for y in range(296, 313, 3):
        s.add(f'<rect x="0" y="{y}" width="113" height="1" fill="#dfe2e8"/>')
    s.add('<rect x="115" y="294" width="1" height="19" fill="#8296c0"/>')
    s.add('<rect x="0" y="314" width="113" height="1" fill="#9cb4d0"/>')
    s.save("eq/eqmain.svg")


def eq_toml():
    T.lines.append("\n# --- equalizer ---")
    T.table("[[window]]", key="eq", placement="docked", empty_press="move")
    T.table("[window.normal]", size=[W, H], see_through=True, cursor="eqnormal")
    n = "normal"
    T.el(n, type="image", rect=[0, 0, W, H], normal="eq/bg.svg")
    T.el(n, type="drag", rect=[0, 0, W, 18], cursor="eqtitle")
    T.el(n, type="button", rect=[254, 3, 8, 8], **look2("eq/shade.svg", 8, 8), click="toggle-eq-shade",
         tip="Toggle Windowshade Mode")
    T.el(n, type="button", rect=[264, 3, 8, 8], **look2("eq/close.svg", 8, 8), click="toggle-eq-window", cursor="eqclose",
         tip="Close")
    T.el(n, type="toggle", rect=[16, 21, 25, 13], **look4("eq/on.svg", 25, 13), flag="eq-on", click="toggle-eq",
         tip="Toggle Equalizer")
    T.el(n, type="toggle", rect=[41, 21, 34, 13], **look4("eq/auto.svg", 34, 13), flag="eq-autoload",
         click="toggle-eq-autoload", tip="Toggle Auto-Load Presets")
    T.el(n, type="eq-graph", rect=[*GRAPH, 113, 19])
    T.el(n, type="button", rect=[219, 21, 44, 13], **look2("eq/presets.svg", 44, 13), menu_press="eq-presets",
         menu="eq-presets")
    for value, cx in [("preamp", PREAMP_X)] + [(f"eq-band-{i + 1}", x) for i, x in enumerate(BAND_X)]:
        T.el(n, type="slider", rect=[cx - 5, EQ_TOP - 5, 11, EQ_LOW - EQ_TOP + 10], value=value, vertical=True,
             thumb=look2("eq/thumb.svg", 11, 10), thumb_len=10, reset=31, wheel_step=-2, cursor="eqslid",
             menu="eq-presets")
    T.table("[window.shade]", size=[W, SHADE_H], see_through=True)
    m = "shade"
    T.el(m, type="image", rect=[0, 0, W, SHADE_H], normal="eq/shade-strip.svg")
    T.el(m, type="drag", rect=[0, 0, W, SHADE_H], double="toggle-eq-shade", cursor="eqtitle")
    shade_buttons(m, "toggle-eq-shade", "toggle-eq-window")


def shade_buttons(mode, shade_cmd, close_cmd):
    """The shade and close buttons at a shade strip's right end."""
    T.el(mode, type="button", rect=[255, 3, 8, 8], **look2("common/shade.svg", 8, 8), click=shade_cmd,
         tip="Toggle Windowshade Mode")
    T.el(mode, type="button", rect=[265, 3, 8, 8], **look2("common/close.svg", 8, 8), click=close_cmd, tip="Close")


# --- playlist -------------------------------------------------------------------------------------
TOP_H, BOTTOM_H, LEFT_W, RIGHT_W = 19, 38, 7, 17
LIST_BG, LIST_SEL = "#1c3d7d", "#6788c9"
LEFT_COLS = ("#000000", "#989fac", "#e5e5eb", "#b6bcc8", "#ccd0d9", "#000000", "#7f8795")
RIGHT_COLS = ("#7f8795", "#ccd0d9") + ("#000000",) * 6 + ("#ccd0d9", "#7f8795", "#7f8795", "#000000", "#c4c9d3",
                                                          "#b6bcc8", "#e5e5eb", "#989fac", "#000000")
BOTTOM_ROWS = ("#181e27", "#d2d5dd") + ("#b2b8c4",) * 7
BOTTOM_LOW = ("#5b6370", "#6b7381", "#808896", "#5f6775", "#000000")
# Oval buttons along the bottom left: (left, label, popup items).
PL_MENUS = (
    ("add", 13, "+", [("file", "FILE", {"click": "pl-add-files"}), ("dir", "DIR", {"click": "pl-add-folder"}),
                      ("url", "URL", {"click": "pl-add-url"})]),
    ("rem", 42, "-", [("rmisc", "MISC", {"menu_release": "playlist-remove-misc"}),
                      ("rsel", "SEL", {"click": "pl-remove-selected"}),
                      ("crop", "CROP", {"click": "pl-crop-selected"}), ("rall", "ALL", {"click": "clear-playlist"})]),
    ("sel", 71, "list", [("sall", "ALL", {"click": "select-all"}), ("snone", "NONE", {"click": "select-none"}),
                         ("sinv", "INV", {"click": "invert-selection"})]),
    ("misc", 100, "dots", [("mopt", "OPTS", {"menu_release": "playlist-misc"}), ("minfo", "INFO", {"click": "file-info"}),
                           ("msort", "SORT", {"menu_release": "playlist-sort"})]),
)
LIST_MENU = [("lnew", "NEW", {"click": "clear-playlist"}), ("lsave", "SAVE", {"click": "playlist-save"}),
             ("lload", "LOAD", {"click": "playlist-open"})]
OVAL_W, OVAL_H, FOLDER_W = 23, 18, 21


def top_piece(name, w, text=None, grips=True, corner=None):
    s = Svg(w, TOP_H)
    shape = {"tl": f"M0 {TOP_H} V4 A4 4 0 0 1 4 0 H{w} V{TOP_H} Z",
             "tr": f"M0 0 H{w - 4} A4 4 0 0 1 {w} 4 V{TOP_H} H0 Z"}.get(corner, f"M0 0 H{w} V{TOP_H} H0 Z")
    s.add(f"<g {clip(s, shape)}>")
    bands(s, 0, 0, w, TITLE_ROWS[:6] + ("#181f2d", "#98a0b0", "#626f86", "#757f96", "#0a1223") + TITLE_ROWS[11:17]
          + ("#050911", "#ffffff"))
    if corner == "tl":
        s.add('<rect x="0" y="0" width="1" height="19" fill="#000000"/>')
        grip(s, 5, w + 3, 6)
    elif corner == "tr":
        s.add(f'<rect x="{w - 1}" y="0" width="1" height="19" fill="#000000"/>')
        grip(s, -3, w - 22, 6)
    elif text:
        titled_grips(s, text, w / 2, -3, w + 3, 12)
    elif grips:
        grip(s, -3, w + 3, 6)
    s.add("</g>")
    s.save(f"pl/{name}.svg")


def side(name, cols):
    s = Svg(len(cols), 29)
    columns(s, 0, 0, 29, cols)
    s.save(f"pl/{name}.svg")


def bottom(name, w, draw=None, corner=None):
    s = Svg(w, BOTTOM_H)
    shape = {"bl": f"M0 0 H{w} V{BOTTOM_H} H4 A4 4 0 0 1 0 {BOTTOM_H - 4} Z",
             "br": f"M0 0 H{w} V{BOTTOM_H - 4} A4 4 0 0 1 {w - 4} {BOTTOM_H} H0 Z"}.get(corner, f"M0 0 H{w} V{BOTTOM_H} H0 Z")
    s.add(f"<g {clip(s, shape)}>")
    bands(s, 0, 0, w, BOTTOM_ROWS)
    mid_h = BOTTOM_H - len(BOTTOM_ROWS) - len(BOTTOM_LOW)
    s.add(f'<rect x="0" y="{len(BOTTOM_ROWS)}" width="{w}" height="{mid_h}" '
          f'fill="{s.grad(("#aeb5c1", "#c9cdd7", "#b9bfcb", "#8d94a2", "#6d7483", "#5e6673"))}"/>')
    bands(s, 0, BOTTOM_H - len(BOTTOM_LOW), w, BOTTOM_LOW)
    if corner == "bl":
        columns(s, 0, 0, BOTTOM_H, LEFT_COLS[:5])
    if corner == "br":
        columns(s, w - 5, 0, BOTTOM_H, RIGHT_COLS[-5:])
    if draw:
        draw(s)
    s.add("</g>")
    if corner:
        edge = (f"M0.5 0 V{BOTTOM_H - 4} A3.5 3.5 0 0 0 4 {BOTTOM_H - 0.5} H{w}" if corner == "bl" else
                f"M{w - 0.5} 0 V{BOTTOM_H - 4} A3.5 3.5 0 0 1 {w - 4} {BOTTOM_H - 0.5} H0")
        s.add(f'<path d="{edge}" fill="none" stroke="{OUTLINE}"/>')
    s.save(f"pl/{name}.svg")


def oval_icon(s, kind, cx, cy):
    if kind == "+":
        s.add(rrect(cx - 3.5, cy - 3.5, 7, 7, 1, INK))
        s.add(f'<path d="M{cx - 2} {cy} h4 M{cx} {cy - 2} v4" stroke="#ffffff"/>')
    elif kind == "-":
        s.add(rrect(cx - 3.5, cy - 3.5, 7, 7, 1, INK))
        s.add(f'<path d="M{cx - 2} {cy} h4" stroke="#ffffff"/>')
    elif kind == "list":
        for k in range(3):
            y = cy - 3 + 2.5 * k
            s.add(f'<rect x="{cx - 3}" y="{y}" width="1" height="1.2" fill="{INK}"/>')
            s.add(f'<rect x="{cx - 1}" y="{y}" width="5" height="1.2" fill="{INK}"/>')
    else:
        for k in range(8):
            a = k * math.pi / 4
            s.add(f'<rect x="{cx - 0.5 + 2.6 * math.cos(a):.2f}" y="{cy - 0.5 + 2.6 * math.sin(a):.2f}" '
                  f'width="1" height="1" fill="{INK}"/>')


def bottom_left(s):
    for _, x, icon, _ in PL_MENUS:
        disc(s, x + OVAL_W / 2, 16.5, OVAL_W / 2 - 2, OVAL_H / 2 - 2.5, FACE)
        oval_icon(s, icon, x + OVAL_W / 2, 16.5)


# The info display at the bottom right, inside its piece.
PL_LCD = "M10 3 H91 A6 6 0 0 1 97 9 V29 A6 6 0 0 1 91 35 H10 A6 6 0 0 1 4 29 V9 A6 6 0 0 1 10 3 Z"


def bottom_right(s):
    lcd(s, PL_LCD, 1, 1)
    for i, name in enumerate(("prev", "play", "pause", "stop", "next", "eject")):
        s.add(ICONS[name](12 + i * 9, 26, 6).replace(INK, LCD_TEXT))
    # Folder button for the list menu.
    disc(s, 116.5, 18, FOLDER_W / 2 - 2, OVAL_H / 2 - 2.5, FACE)
    s.add(f'<path d="M112.5 15.5 h3 l1 1 h4.5 v5 h-8.5 Z" fill="none" stroke="{INK}"/>')
    # Arrows for the shade mode, and the resize grip.
    s.add(f'<path d="M138 4 L142 8 H134 Z M134 9.5 H142 M134 11 H142 L138 15 Z" fill="#102d66" stroke="#102d66" '
          f'stroke-width="0.6"/>')
    for k in range(4):
        s.add(f'<path d="M{135 + 3 * k} 35 L146 {24 + 3 * k}" stroke="#3c424e" stroke-width="1.2"/>')


def item_art(key, text, w):
    s = Svg(2 * w, OVAL_H)
    for i, face in enumerate((FACE, FACE_DOWN)):
        s.add(f'<g transform="translate({i * w},0)">')
        s.add(rrect(0.5, 0.5, w - 1, OVAL_H - 1, (OVAL_H - 1) / 2, s.grad(face), RING))
        s.add(pf.svg_text(pf.DISPLAY, text, w / 2 + 0.5 * i, 6 + i, INK, anchor="middle"))
        s.add("</g>")
    s.save(f"pl/item-{key}.svg")


def scroll_thumb():
    s = Svg(12, 18)
    for i, face in enumerate((("#ffffff", "#c4c9d3"), ("#c4c9d3", "#ffffff"))):
        s.add(f'<g transform="translate({i * 6},0)">')
        s.add(f'<rect x="0" y="0" width="6" height="18" fill="{s.grad(face, 1, 0)}"/>')
        for y in range(3, 16, 3):
            s.add(f'<rect x="1" y="{y}" width="4" height="1" fill="#8b93a0"/>')
        s.add("</g>")
    s.save("pl/scroll-thumb.svg")


def playlist_art():
    top_piece("top-left", 25, corner="tl")
    top_piece("top-tile", 25)
    top_piece("title", 100, text="PLAYLIST")
    top_piece("top-right", 25, corner="tr")
    side("left", LEFT_COLS)
    side("right", RIGHT_COLS)
    bottom("bottom-left", 125, bottom_left, "bl")
    bottom("bottom-tile", 25)
    bottom("bottom-right", 150, bottom_right, "br")
    scroll_thumb()
    shade_strip("pl/shade-strip.svg", (8, 248))
    navy = ("#3a4a6d", "#1c2c51")
    box_button("pl/shade.svg", f'<rect x="2" y="3" width="4" height="2" fill="#b8c0d0"/>', 8, 12, navy, "#7d879c")
    box_button("pl/close.svg", '<path d="M2 4 L6 9 M6 4 L2 9" stroke="#b8c0d0" stroke-width="1.4"/>', 8, 12, navy,
               "#7d879c")
    for _, _, _, items in PL_MENUS:
        for key, text, _ in items:
            item_art(key, text, OVAL_W)
    for key, text, _ in LIST_MENU:
        item_art(key, text, FOLDER_W)


def popup(left, right, top, bottom_, w, items):
    T.el("normal", type="popup", edges=[left, top, right, bottom_], above=True)
    for key, _, act in items:
        T.lines.append("[[window.normal.element.item]]")
        T.lines.append(f'normal = {skinsvg.val(part(f"pl/item-{key}.svg", 0, 0, w, OVAL_H))}')
        T.lines.append(f'pressed = {skinsvg.val(part(f"pl/item-{key}.svg", w, 0, w, OVAL_H))}')
        for k, v in act.items():
            T.lines.append(f"{k} = {skinsvg.val(v)}")


def playlist_toml():
    T.lines.append("\n# --- playlist ---")
    T.table("[[window]]", key="playlist", placement="docked", empty_press="nothing", wheel_scrolls_list=True)
    T.table("[window.normal]", size=[W, H], resize_step=[25, 29], see_through=True, cursor="pnormal")
    n = "normal"
    T.el(n, type="image", edges=[0, 0, 25, TOP_H], normal="pl/top-left.svg")
    T.el(n, type="image", edges=[25, 0, "end-25", TOP_H], normal="pl/top-tile.svg", fill="tile")
    T.el(n, type="image", edges=["center-50", 0, "center+50", TOP_H], normal="pl/title.svg")
    T.el(n, type="image", edges=["end-25", 0, "end", TOP_H], normal="pl/top-right.svg")
    T.el(n, type="image", edges=[0, TOP_H, LEFT_W, f"end-{BOTTOM_H}"], normal="pl/left.svg", fill="tile")
    T.el(n, type="image", edges=[f"end-{RIGHT_W}", TOP_H, "end", f"end-{BOTTOM_H}"], normal="pl/right.svg", fill="tile")
    T.el(n, type="image", edges=[125, f"end-{BOTTOM_H}", "end-150", "end"], normal="pl/bottom-tile.svg", fill="tile")
    T.el(n, type="image", edges=[0, f"end-{BOTTOM_H}", 125, "end"], normal="pl/bottom-left.svg")
    T.el(n, type="image", edges=["end-150", f"end-{BOTTOM_H}", "end", "end"], normal="pl/bottom-right.svg")
    T.el(n, type="drag", edges=[0, 0, "end", 18], double="toggle-pl-shade", cursor="ptbar")
    T.el(n, type="button", edges=["end-20", 2, "end-12", 14], **look2("pl/shade.svg", 8, 12), click="toggle-pl-shade",
         tip="Toggle Windowshade Mode")
    T.el(n, type="button", edges=["end-10", 2, "end-2", 14], **look2("pl/close.svg", 8, 12),
         click="toggle-playlist-window", cursor="pclose", tip="Close")
    T.el(n, type="list", edges=[LEFT_W, TOP_H, f"end-{RIGHT_W}", f"end-{BOTTOM_H}"], menu="playlist-row")
    T.el(n, type="list-scroll", edges=["end-15", TOP_H, "end-9", f"end-{BOTTOM_H}"],
         thumb=look2("pl/scroll-thumb.svg", 6, 18), cursor="pvscroll")
    T.el(n, type="text", edges=["end-141", "end-28", "end-60", "end-21"], text="playlist-time")
    T.el(n, type="text", edges=["end-76", "end-15", "end-50", "end-8"], text="playlist-clock", hide="stopped")
    for i, cmd in enumerate(("previous", "play", "pause", "stop", "next", "open-file")):
        T.el(n, type="button", edges=[f"end-{142 - i * 9}", "end-16", f"end-{133 - i * 9}", "end-7"], click=cmd)
    T.el(n, type="button", edges=["end-16", "end-35", "end-8", "end-24"], click="toggle-pl-shade",
         tip="Toggle Windowshade Mode")
    T.el(n, type="resize", edges=["end-16", "end-14", "end", "end"], cursor="psize")
    for _, left, _, items in PL_MENUS:
        popup(left, left + OVAL_W, f"end-{H - 86}", f"end-{H - 86 - OVAL_H}", OVAL_W, items)
    popup("end-44", f"end-{44 - FOLDER_W}", "end-29", "end-11", FOLDER_W, LIST_MENU)
    T.table("[window.shade]", size=[W, SHADE_H], see_through=True)
    m = "shade"
    T.el(m, type="image", rect=[0, 0, W, SHADE_H], normal="pl/shade-strip.svg")
    T.el(m, type="drag", rect=[0, 0, W, SHADE_H], double="toggle-pl-shade", cursor="ptbar")
    T.el(m, type="text", rect=[11, 4, 190, 7], text="title")
    T.el(m, type="text", rect=[205, 4, 40, 7], text="playlist-clock", hide="stopped")
    shade_buttons(m, "toggle-pl-shade", "toggle-playlist-window")


# --- skin ----------------------------------------------------------------------------------------
def vis_colors():
    """The analyzer's bars lit only on the display's dot rows, so they look
    segmented: colour 2 + r paints row r of the 16 from the top."""
    rows = [LCD_TEXT if r % 2 else LCD_BG for r in range(16)]
    return [LCD_BG, LCD_DOT] + rows + ["#ffffff", "#e6ecf8", "#c6d4f0", "#9fb4df", "#7d95c8", "#ffffff"]


def main():
    T.lines.append("# Popamp Steel: silver and navy, after the modern players of the 2000s.")
    T.lines.append("# Generated by tools/gen_steel_skin.py; own artwork and pixel fonts.")
    T.table("[meta]", name="Popamp Steel", author="Popamp", format_version=1)
    T.table("[sheets]", eqmain="eq/eqmain.svg")
    T.table("[text]", font="fonts/display.ttf", size=7, color=LCD_TEXT)
    T.table("[clock]", font="fonts/clock.ttf", size=13, color=LCD_TEXT)
    T.table("[playlist]", normal="#d4dbe8", current="#9fb6e6", normal_bg=LIST_BG, selected_bg=LIST_SEL)
    T.table("[vis]", colors=vis_colors())
    os.makedirs(os.path.join(skinsvg.OUT, "fonts"), exist_ok=True)
    pf.save_ttf(pf.DISPLAY, os.path.join(skinsvg.OUT, "fonts/display.ttf"), "Popamp Steel Display", rise=1)
    pf.save_ttf(pf.CLOCK, os.path.join(skinsvg.OUT, "fonts/clock.ttf"), "Popamp Steel Clock", gap=3)
    main_art()
    main_toml()
    main_shade()
    eq_art()
    eq_toml()
    playlist_art()
    playlist_toml()
    T.save()


main()
