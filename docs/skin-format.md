# Vector skin format

A vector skin is drawn in SVG, so it stays sharp at every window size,
including fractional ones such as x1.25 or x1.5. Popamp's own default skin
is a vector skin: `skins/default/` in Popamp's source is a complete example
that draws every classic sheet, and `skins/modern/` one that describes its
own windows.

A vector skin is a folder, or a `.psz` archive (a zip file under another
name), with a file named `skin.toml` at its top. Opening a `.psz` file in
Popamp installs and applies it; a folder or `.zip` placed in the skins
folder works too. Everything else in it (SVG pictures, TrueType
fonts) is referred to from `skin.toml` by its path inside the skin, for
example `"buttons/play.svg"`.

Coordinates are in **skin pixels**: the classic main window is 275 x 116 of
them, whatever size the window is shown at. An SVG drawn 23 units wide
covers 23 skin pixels.

While a skin is in use, Popamp checks its files twice a second and loads
it again after you save a change. If the change breaks the skin, the
current look stays and the title display shows the error.

## Two ways to build a skin

1. **Draw the classic sheets in SVG** (`[sheets]`). You get the classic
   layout of all three windows and their shade modes, with every button,
   slider and text, and only draw the pictures. This is the easy way.
2. **Describe your own windows** (`[[window]]`). You place every element
   yourself and can give windows any size and set of controls.

Both can be mixed: windows you describe replace the classic ones, the rest
keep the classic layout.

## A minimal skin

```toml
[meta]
name = "My Skin"
author = "Me"
format_version = 1

[sheets]
main = "main.svg"
cbuttons = "cbuttons.svg"
```

Sheets you leave out, and anything else missing, come from Popamp's
default skin.

## `[meta]`

| Key | | |
|---|---|---|
| `name` | required | Shown in the skins menu. |
| `author` | optional | |
| `format_version` | required | `1`. |

## `[sheets]`: SVG drawings of the classic sheets

Each key is a classic skin bitmap without `.bmp`; the value is an SVG
drawing of it at the same size and with the same sprite positions as the
classic bitmap:

`main`, `cbuttons`, `titlebar`, `shufrep`, `text`, `volume`, `balance`,
`monoster`, `playpaus`, `numbers`, `nums_ex`, `posbar`, `eqmain`, `eq_ex`,
`pledit`.

`nums_ex` (digits with a minus sign) is optional: when a skin has it, the
play time uses it instead of `numbers`.

For example `cbuttons.svg` is 136 x 36 units with the Previous button's
normal picture at 0, 0, 23 x 18 and its pressed picture at 0, 18. Any
reference on classic Winamp 2 skins describes where each sprite is.

## `[shapes]`: window shapes for the classic layout

The opaque part of the picture is the visible window; transparent parts
let the desktop show through. Keys are `main`, `eq`, `playlist`, and the
same with `_shade` for the shade modes:

```toml
[shapes]
main = "shape-main.svg"
```

## `[text]` and `[clock]`: fonts

Without these, the small texts (song title, kbps, kHz...) use the `text`
sheet and the play time uses the `numbers` sheet. With them, they are
written in a TrueType font:

```toml
[text]
font = "fonts/display.ttf"   # optional; the system's sans-serif font otherwise
size = 7                     # height in skin pixels
color = "#00e000"

[clock]
font = "fonts/digits.ttf"
size = 13
color = "#00e000"
```

Characters the font does not have (for example Japanese in a song title)
are taken from a system font that has them.

## `[playlist]`

```toml
[playlist]
font = "fonts/list.ttf"      # optional
normal = "#00ff00"           # text
current = "#ffffff"          # the playing entry
normal_bg = "#000000"
selected_bg = "#0000c6"
```

Colours left out keep the classic ones. The text size is the user's
playlist font size setting.

## `[vis]`

```toml
[vis]
colors = ["#000000", "#181818", ...]   # 24 colours
```

The order is the classic `viscolor.txt` one: background, background dots,
16 spectrum colours from the top of a bar down, 5 oscilloscope colours,
the peak dots.

## `[[window]]`: your own windows

```toml
[[window]]
key = "main"                 # main, eq or playlist
placement = "free"           # free (moves the docked group) or docked
empty_press = "move"         # nothing, move or move-if-easy
wheel_volume = 8             # volume change per mouse wheel step
# wheel_scrolls_list = true  # the wheel scrolls the list instead

[window.normal]
size = [275, 116]
# resize_step = [25, 29]     # makes the window resizable in these steps
cursor = "normal"            # the default mouse cursor (see Words)
shape = "shape.svg"          # optional window shape

[[window.normal.element]]
type = "image"
rect = [0, 0, 275, 116]
normal = "main/background.svg"

[[window.normal.element]]
type = "button"
rect = [39, 88, 23, 18]
normal = "main/play.svg"
pressed = "main/play-down.svg"
click = "play"
shift = "open-file"
tip = "Play"
```

`[window.shade]` describes the shade mode the same way. Elements are drawn
in the order they are listed; the mouse goes to the last one under it.

### Where an element is

- `rect = [x, y, width, height]` for an element that stays put, or
- `edges = [left, top, right, bottom]` for one that follows a resizable
  window. Each edge is a number (from the window's left or top), `"end-20"`
  (20 from its right or bottom) or `"center+5"` (5 past its middle).

### Pictures

A picture is an SVG file, drawn at its own size from the element's
top-left corner:

```toml
normal = "play.svg"
```

or an area of a bigger SVG, optionally shifted:

```toml
normal = { file = "buttons.svg", area = [23, 0, 23, 18], offset = [0, 1] }
```

### Element types

Every element can also have `show`, `hide` or `min_width` (when it is
visible: while a player state holds, while it does not, or while the
window is at least that wide), `cursor`, `tip` (tooltip text), `menu`
(its right-click menu) and `hit`: a picture whose opaque part is where the
element takes the mouse, for round buttons. Without `hit` the whole
rectangle does:

```toml
normal = "play.svg"
hit = "play.svg"       # only the round part is the button
```

| `type` | What it is | Its keys |
|---|---|---|
| `image` | A picture | `normal`, `on` (while `lit` holds), `inactive` (window not focused), `fill` (`once` or `tile`), `lit` |
| `drag` | Moves the window | `double` (command on double click) |
| `button` | A push button | `normal`, `pressed`, `inactive`, and what it does |
| `toggle` | A button showing a state | `normal`, `pressed`, `on`, `on_pressed`, `flag`, and what it does |
| `slider` | A slider | `value`, `vertical`, `track`, `thumb`, `thumb_len`, `live`, `reset`, `wheel_step`, `detent` |
| `text` | A text | `text` |
| `marquee` | The scrolling song title | `area` (the text's place inside the element), `double` |
| `clock` | The play time | |
| `vis` | Spectrum analyzer or oscilloscope | |
| `eq-graph` | The equalizer curve | |
| `list` | The playlist's track list | |
| `list-scroll` | The track list's scroll bar | `thumb` |
| `popup` | A button opening a column of picture buttons | `item` (each with `normal`, `pressed` and what it does), `bar` |
| `resize` | Resizes the window | |

What a button does is one of:

- `click = "command"`, run when the mouse button is released over it;
  `shift` and `ctrl` give commands for Shift+click and Ctrl+click;
- `press = "command"`, run as soon as the button goes down;
- `menu_press = "menu"` or `menu_release = "menu"`, opening a menu.

A slider:

```toml
[[window.normal.element]]
type = "slider"
rect = [107, 57, 68, 13]
value = "volume"
track = { file = "volume.svg", first = [0, 0, 68, 13], step = [0, 15], count = 28 }
thumb = { normal = "thumb.svg", pressed = "thumb-down.svg" }
reset = 200        # value a double click sets
wheel_step = 8     # change per wheel step over the slider
```

Values run from 0 to 255 for `volume`, -127 to 127 for `balance`, 0 to 63
for `preamp` and the EQ bands (31 is the middle), and over the track's
length in milliseconds for `position`.

`track` is one picture per value step in a grid: the first at `first`,
the next `step` further, `count` of them, `per_row` to a row, with `map`
`rising` (first at the minimum), `falling` or `from-middle`. `live = false`
changes the value only when the slider is let go (as the position bar
does). `detent` snaps values that close to the middle to it.

### Words

Commands, states, values, texts, menus and cursors are written as words.
A wrong word is reported with the list of right ones.

- **Commands:** the player's commands in lower case with dashes, for
  example `play`, `pause`, `stop`, `next`, `previous`, `open-file`,
  `toggle-shuffle`, `toggle-repeat`, `toggle-eq-window`,
  `toggle-playlist-window`, `toggle-windowshade`, `minimize`, `exit`.
- **Player states** (`flag`, `lit`, `show`, `hide`): `playing`, `paused`,
  `stopped`, `seekable`, `stereo`, `mono`, `shuffle`, `repeat`,
  `eq-window`, `playlist-window`, `always-on-top`, `double-size`, `eq-on`,
  `eq-autoload`.
- **Values:** `volume`, `balance`, `position`, `preamp`, `eq-band-1` to
  `eq-band-10`.
- **Texts:** `kbps`, `khz`, `short-time`, `title`, `playlist-time`,
  `playlist-clock`.
- **Menus:** `main`, `options`, `vis`, `eq-presets`, `playlist-row`,
  `playlist-misc`, `playlist-sort`, `playlist-remove-misc`, and the
  button menus `previous`, `play`, `pause`, `stop`, `next`, `eject`,
  `seek`, `repeat`, `shuffle`, `time`, `song-title`.
- **Cursors:** the classic cursor file names without `.cur`: `normal`,
  `close`, `titlebar`, `volbal`, `posbar`, `songname`, `psize`...

## Mistakes

`skin.toml` is checked strictly. An unknown key, a key that does not apply
to an element type, a missing file or a file that is not an SVG or a font
stops the skin from loading, with a message such as:

```
skin.toml, line 12: unknown command `plya`; it can be one of: exit, ...
```

## Tips

- Draw on whole skin pixels where you want sharp edges at x1; at larger
  sizes everything is smooth anyway.
- SVG `<text>` is not drawn. Convert texts in pictures to outlines.
- Keep each SVG's `width` and `height` equal to the area it covers.
