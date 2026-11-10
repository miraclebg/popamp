//! A tiny set of controls for Popamp's own dialogs: labels, text fields,
//! lists, buttons, check boxes and group frames. A [`Form`] holds them,
//! turns mouse and keys into [`Event`]s and draws itself in the colours of
//! the pop-up menus. Coordinates are window pixels.

use crate::font::Font;
use crate::menu::{BG, BORDER, DIM, FONT_PX, HILITE, TEXT};
use popamp_skin::bitmap::mix;
use popamp_skin::Bitmap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}

/// The keys a form understands; the window code translates real key events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    Up,
    Down,
    PageUp,
    PageDown,
    Enter,
    Escape,
    Tab,
}

/// One control. `A` names what a button or check box stands for.
#[derive(Debug, Clone)]
pub enum Widget<A> {
    Label {
        text: String,
        dim: bool,
    },
    Input {
        text: String,
        cursor: usize,
    },
    List {
        rows: Vec<String>,
        selected: Option<usize>,
        scroll: usize,
    },
    Button {
        label: String,
        action: A,
        default: bool,
    },
    Check {
        label: String,
        checked: bool,
        action: A,
    },
    Group {
        label: String,
    },
    /// A picture, or `note` in an empty frame until it arrives.
    Image {
        image: Option<Bitmap>,
        note: String,
    },
}

/// What happened to the form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event<A> {
    None,
    /// A button was clicked, or Enter pressed with a default button.
    Clicked(A),
    Toggled(A, bool),
    /// The text of a field changed.
    Edited,
    /// The list selection moved.
    Selected,
    /// A list row was double-clicked, or Enter pressed without a default button.
    Activated,
    /// Escape.
    Cancel,
}

pub const ROW_H: i32 = 18;
pub const BUTTON: (i32, i32) = (80, 24);
/// Text fields and lists are a little darker than the dialog.
const FIELD_BG: u32 = darken(BG);

const fn darken(c: u32) -> u32 {
    let (r, g, b) = ((c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff);
    ((r * 3 / 4) << 16) | ((g * 3 / 4) << 8) | (b * 3 / 4)
}

pub struct Form<A> {
    pub w: i32,
    pub h: i32,
    controls: Vec<(Rect, Widget<A>)>,
    focus: Option<usize>,
    /// The button held down by the mouse.
    pressed: Option<usize>,
}

impl<A: Copy + PartialEq> Form<A> {
    pub fn new(w: i32, h: i32) -> Form<A> {
        Form { w, h, controls: Vec::new(), focus: None, pressed: None }
    }

    /// Adds a control and returns its index. The first text field or list
    /// gets the keyboard.
    pub fn add(&mut self, rect: Rect, widget: Widget<A>) -> usize {
        let i = self.controls.len();
        if self.focus.is_none() && matches!(widget, Widget::Input { .. } | Widget::List { .. }) {
            self.focus = Some(i);
        }
        self.controls.push((rect, widget));
        i
    }

    #[cfg(test)]
    pub fn widget(&self, i: usize) -> &Widget<A> {
        &self.controls[i].1
    }

    #[cfg(all(test, feature = "plugins"))]
    pub fn count(&self) -> usize {
        self.controls.len()
    }

    #[cfg(test)]
    pub fn rect(&self, i: usize) -> Rect {
        self.controls[i].0
    }

    /// Changes the text of label or button `i`.
    pub fn set_label(&mut self, i: usize, new: String) {
        if let Widget::Label { text: old, .. } | Widget::Button { label: old, .. } = &mut self.controls[i].1 {
            *old = new;
        }
    }

    /// Shows `image` in image `i`, or `note` when there is none.
    pub fn set_image(&mut self, i: usize, new: Option<Bitmap>, new_note: &str) {
        if let Widget::Image { image, note } = &mut self.controls[i].1 {
            *image = new;
            *note = new_note.into();
        }
    }

    /// Appends rows to list `i`, keeping the selection and scroll.
    pub fn push_rows(&mut self, i: usize, more: impl IntoIterator<Item = String>) {
        if let Widget::List { rows, .. } = &mut self.controls[i].1 {
            rows.extend(more);
        }
    }

    /// The text of field `i` (empty for other controls).
    pub fn text(&self, i: usize) -> &str {
        match &self.controls[i].1 {
            Widget::Input { text, .. } => text,
            _ => "",
        }
    }

    /// Whether check box `i` is ticked.
    #[cfg(unix)]
    pub fn checked(&self, i: usize) -> bool {
        matches!(self.controls[i].1, Widget::Check { checked: true, .. })
    }

    /// The selected row of list `i`.
    pub fn selected(&self, i: usize) -> Option<usize> {
        match &self.controls[i].1 {
            Widget::List { selected, .. } => *selected,
            _ => None,
        }
    }

    /// Replaces the rows of list `i`, selecting `select` (clamped).
    pub fn set_rows(&mut self, i: usize, new_rows: Vec<String>, select: Option<usize>) {
        if let Widget::List { rows, selected, scroll } = &mut self.controls[i].1 {
            *selected = select.filter(|_| !new_rows.is_empty()).map(|s| s.min(new_rows.len() - 1));
            *rows = new_rows;
            *scroll = 0;
        }
        self.scroll_to_selection(i);
    }

    fn default_action(&self) -> Option<A> {
        self.controls.iter().find_map(|(_, w)| match w {
            Widget::Button { action, default: true, .. } => Some(*action),
            _ => None,
        })
    }

    fn list_index(&self) -> Option<usize> {
        self.controls.iter().position(|(_, w)| matches!(w, Widget::List { .. }))
    }

    /// Visible rows of the list at `i`.
    fn page(&self, i: usize) -> usize {
        ((self.controls[i].0.h - 4) / ROW_H).max(1) as usize
    }

    fn scroll_to_selection(&mut self, i: usize) {
        let page = self.page(i);
        if let Widget::List { selected: Some(s), scroll, .. } = &mut self.controls[i].1 {
            if *s < *scroll {
                *scroll = *s;
            } else if *s >= *scroll + page {
                *scroll = *s + 1 - page;
            }
        }
    }

    pub fn mouse_down(&mut self, x: i32, y: i32, double: bool) -> Event<A> {
        let Some(i) = self
            .controls
            .iter()
            .position(|(r, w)| r.contains(x, y) && !matches!(w, Widget::Label { .. } | Widget::Group { .. } | Widget::Image { .. }))
        else {
            return Event::None;
        };
        let rect = self.controls[i].0;
        match &mut self.controls[i].1 {
            Widget::Input { text, cursor } => {
                self.focus = Some(i);
                *cursor = text.chars().count();
                Event::None
            }
            Widget::List { rows, selected, scroll } => {
                self.focus = Some(i);
                let row = ((y - rect.y - 2) / ROW_H) as usize + *scroll;
                if row >= rows.len() {
                    return Event::None;
                }
                *selected = Some(row);
                if double {
                    Event::Activated
                } else {
                    Event::Selected
                }
            }
            Widget::Button { .. } => {
                self.pressed = Some(i);
                Event::None
            }
            Widget::Check { checked, action, .. } => {
                *checked = !*checked;
                Event::Toggled(*action, *checked)
            }
            Widget::Label { .. } | Widget::Group { .. } | Widget::Image { .. } => Event::None,
        }
    }

    /// A button fires when the mouse is released over it.
    pub fn mouse_up(&mut self, x: i32, y: i32) -> Event<A> {
        match self.pressed.take().map(|i| &self.controls[i]) {
            Some((r, Widget::Button { action, .. })) if r.contains(x, y) => Event::Clicked(*action),
            _ => Event::None,
        }
    }

    /// Scrolls the list by `steps` wheel notches (positive = up).
    pub fn wheel(&mut self, steps: i32) {
        let Some(i) = self.list_index() else { return };
        let page = self.page(i);
        if let Widget::List { rows, scroll, .. } = &mut self.controls[i].1 {
            let max = rows.len().saturating_sub(page) as i32;
            *scroll = (*scroll as i32 - steps * 3).clamp(0, max) as usize;
        }
    }

    pub fn key(&mut self, key: Key) -> Event<A> {
        match key {
            Key::Escape => return Event::Cancel,
            Key::Enter => return self.default_action().map_or(Event::Activated, Event::Clicked),
            Key::Tab => {
                self.focus_next();
                return Event::None;
            }
            // The arrows move the list even while typing in a search field.
            Key::Up | Key::Down | Key::PageUp | Key::PageDown => return self.move_selection(key),
            _ => {}
        }
        let Some(f) = self.focus else { return Event::None };
        let Widget::Input { text, cursor } = &mut self.controls[f].1 else { return Event::None };
        if edit(text, cursor, key) {
            Event::Edited
        } else {
            Event::None
        }
    }

    fn focus_next(&mut self) {
        let focusable: Vec<usize> =
            (0..self.controls.len()).filter(|&i| matches!(self.controls[i].1, Widget::Input { .. } | Widget::List { .. })).collect();
        if focusable.is_empty() {
            return;
        }
        let cur = self.focus.and_then(|f| focusable.iter().position(|&x| x == f));
        self.focus = Some(focusable[cur.map_or(0, |c| (c + 1) % focusable.len())]);
    }

    fn move_selection(&mut self, key: Key) -> Event<A> {
        let Some(i) = self.list_index() else { return Event::None };
        let page = self.page(i) as i64;
        let Widget::List { rows, selected, .. } = &mut self.controls[i].1 else { return Event::None };
        if rows.is_empty() {
            return Event::None;
        }
        let step = match key {
            Key::Up => -1,
            Key::Down => 1,
            Key::PageUp => -page,
            _ => page,
        };
        let cur = selected.map_or(-1, |s| s as i64);
        *selected = Some((cur + step).clamp(0, rows.len() as i64 - 1) as usize);
        self.scroll_to_selection(i);
        Event::Selected
    }

    pub fn draw(&self, b: &mut Bitmap, font: &mut Font) {
        b.fill(0, 0, b.w, b.h, BG);
        for (i, (r, w)) in self.controls.iter().enumerate() {
            let focused = self.focus == Some(i);
            match w {
                Widget::Label { text, dim } => {
                    let t = font.fit(text, FONT_PX, r.w);
                    font.draw(b, r.x, r.y + (r.h - FONT_PX as i32) / 2 - 1, &t, FONT_PX, if *dim { DIM } else { TEXT }, r.x, r.w);
                }
                Widget::Input { text, cursor } => draw_input(b, font, *r, text, *cursor, focused),
                Widget::List { rows, selected, scroll } => draw_list(b, font, *r, rows, *selected, *scroll, focused),
                Widget::Button { label, default, .. } => draw_button(b, font, *r, label, self.pressed == Some(i), *default),
                Widget::Check { label, checked, .. } => draw_check(b, font, *r, label, *checked),
                Widget::Group { label } => draw_group(b, font, *r, label),
                Widget::Image { image, note } => draw_image(b, font, *r, image.as_ref(), note),
            }
        }
    }
}

/// Applies an editing key to a one-line text field. Returns whether the
/// text changed.
fn edit(text: &mut String, cursor: &mut usize, key: Key) -> bool {
    let mut chars: Vec<char> = text.chars().collect();
    *cursor = (*cursor).min(chars.len());
    match key {
        Key::Char(c) if !c.is_control() => {
            chars.insert(*cursor, c);
            *cursor += 1;
        }
        Key::Backspace if *cursor > 0 => {
            *cursor -= 1;
            chars.remove(*cursor);
        }
        Key::Delete if *cursor < chars.len() => {
            chars.remove(*cursor);
        }
        Key::Left => *cursor = cursor.saturating_sub(1),
        Key::Right => *cursor = (*cursor + 1).min(chars.len()),
        Key::Home => *cursor = 0,
        Key::End => *cursor = chars.len(),
        _ => {}
    }
    let new: String = chars.into_iter().collect();
    let changed = new != *text;
    *text = new;
    changed
}

fn sunken(b: &mut Bitmap, r: Rect, focused: bool) {
    b.fill(r.x, r.y, r.w, r.h, FIELD_BG);
    let edge = if focused { HILITE } else { BORDER };
    b.bevel(r.x, r.y, r.w, r.h, edge, edge);
}

fn text_y(r: Rect) -> i32 {
    r.y + (r.h - FONT_PX as i32) / 2 - 1
}

fn draw_input(b: &mut Bitmap, font: &mut Font, r: Rect, text: &str, cursor: usize, focused: bool) {
    sunken(b, r, focused);
    let (clip_x, clip_w) = (r.x + 2, r.w - 4);
    // Scroll the text left when the caret would leave the field.
    let before: String = text.chars().take(cursor).collect();
    let caret = font.width(&before, FONT_PX);
    let shift = (caret - (clip_w - 8)).max(0);
    let x = r.x + 4 - shift;
    font.draw(b, x, text_y(r), text, FONT_PX, TEXT, clip_x, clip_w);
    if focused {
        b.vline(x + caret, r.y + 4, r.h - 8, TEXT);
    }
}

fn draw_list(b: &mut Bitmap, font: &mut Font, r: Rect, rows: &[String], selected: Option<usize>, scroll: usize, focused: bool) {
    sunken(b, r, focused);
    let page = ((r.h - 4) / ROW_H) as usize;
    for (n, row) in rows.iter().enumerate().skip(scroll).take(page) {
        let y = r.y + 2 + (n - scroll) as i32 * ROW_H;
        let sel = selected == Some(n);
        if sel {
            b.fill(r.x + 2, y, r.w - 4, ROW_H, if focused { HILITE } else { mix(HILITE, FIELD_BG, 0.5) });
        }
        let t = font.fit(row, FONT_PX, r.w - 12);
        font.draw(b, r.x + 5, y + (ROW_H - FONT_PX as i32) / 2 - 1, &t, FONT_PX, TEXT, r.x + 2, r.w - 4);
    }
    // A thin bar on the right shows where the visible rows are.
    if rows.len() > page && page > 0 {
        let track = r.h - 4;
        let bar_h = (track * page as i32 / rows.len() as i32).max(10);
        let bar_y = r.y + 2 + (track - bar_h) * scroll as i32 / (rows.len() - page) as i32;
        b.fill(r.x + r.w - 5, bar_y, 3, bar_h, BORDER);
    }
}

fn draw_button(b: &mut Bitmap, font: &mut Font, r: Rect, label: &str, down: bool, default: bool) {
    let face = if down { HILITE } else { mix(BG, BORDER, 0.35) };
    b.fill(r.x, r.y, r.w, r.h, face);
    let edge = if default { mix(HILITE, TEXT, 0.3) } else { BORDER };
    b.bevel(r.x, r.y, r.w, r.h, edge, edge);
    let tw = font.width(label, FONT_PX);
    font.draw(b, r.x + (r.w - tw) / 2, text_y(r), label, FONT_PX, TEXT, r.x + 2, r.w - 4);
}

fn draw_check(b: &mut Bitmap, font: &mut Font, r: Rect, label: &str, checked: bool) {
    let (bx, by) = (r.x, r.y + (r.h - 13) / 2);
    sunken(b, Rect::new(bx, by, 13, 13), false);
    if checked {
        for k in 0..3 {
            b.vline(bx + 3 + k, by + 5 + k, 2, TEXT);
        }
        for k in 0..4 {
            b.vline(bx + 6 + k, by + 6 - k, 2, TEXT);
        }
    }
    font.draw(b, r.x + 20, text_y(r), label, FONT_PX, TEXT, r.x, r.w);
}

fn draw_group(b: &mut Bitmap, font: &mut Font, r: Rect, label: &str) {
    let line = mix(BORDER, BG, 0.3);
    let top = r.y + FONT_PX as i32 / 2;
    b.bevel(r.x, top, r.w, r.h - (top - r.y), line, line);
    let tw = font.width(label, FONT_PX);
    b.fill(r.x + 8, r.y, tw + 6, FONT_PX as i32 + 2, BG);
    font.draw(b, r.x + 11, r.y - 1, label, FONT_PX, DIM, r.x, r.w);
}

fn draw_image(b: &mut Bitmap, font: &mut Font, r: Rect, image: Option<&Bitmap>, note: &str) {
    match image {
        Some(img) => b.blit(img, 0, 0, img.w.min(r.w), img.h.min(r.h), r.x, r.y),
        None => {
            sunken(b, r, false);
            let t = font.fit(note, FONT_PX, r.w - 8);
            let tw = font.width(&t, FONT_PX);
            font.draw(b, r.x + (r.w - tw) / 2, r.y + r.h / 2 - FONT_PX as i32 / 2, &t, FONT_PX, DIM, r.x, r.w);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Act {
        Ok,
        Cancel,
        Flag,
    }

    fn rows(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("row {i}")).collect()
    }

    /// A search field over a list of 20 rows (about 5 visible) and two buttons.
    fn form() -> (Form<Act>, usize, usize) {
        let mut f = Form::new(300, 200);
        let input = f.add(Rect::new(10, 10, 200, 22), Widget::Input { text: String::new(), cursor: 0 });
        let list = f.add(Rect::new(10, 40, 200, 4 + 5 * ROW_H), Widget::List { rows: rows(20), selected: Some(0), scroll: 0 });
        f.add(Rect::new(10, 150, 80, 24), Widget::Button { label: "OK".into(), action: Act::Ok, default: true });
        f.add(Rect::new(100, 150, 80, 24), Widget::Button { label: "Cancel".into(), action: Act::Cancel, default: false });
        (f, input, list)
    }

    fn type_text(f: &mut Form<Act>, s: &str) {
        for c in s.chars() {
            f.key(Key::Char(c));
        }
    }

    #[test]
    fn typing_edits_the_focused_field() {
        let (mut f, input, _) = form();
        type_text(&mut f, "abc");
        assert_eq!(f.key(Key::Left), Event::None);
        assert_eq!(f.key(Key::Backspace), Event::Edited);
        assert_eq!(f.text(input), "ac");
        f.key(Key::Home);
        assert_eq!(f.key(Key::Char('x')), Event::Edited);
        f.key(Key::End);
        assert_eq!(f.key(Key::Delete), Event::None);
        assert_eq!(f.text(input), "xac");
        assert_eq!(f.key(Key::Char('й')), Event::Edited);
        assert_eq!(f.text(input), "xacй");
    }

    #[test]
    fn arrows_move_the_list_while_typing() {
        let (mut f, _, list) = form();
        type_text(&mut f, "q");
        assert_eq!(f.key(Key::Down), Event::Selected);
        assert_eq!(f.selected(list), Some(1));
        f.key(Key::PageDown);
        assert_eq!(f.selected(list), Some(6));
        f.key(Key::Up);
        f.key(Key::Up);
        assert_eq!(f.selected(list), Some(4));
        for _ in 0..30 {
            f.key(Key::Down);
        }
        assert_eq!(f.selected(list), Some(19));
    }

    #[test]
    fn keeps_the_selection_in_view() {
        let (mut f, _, list) = form();
        f.key(Key::PageDown);
        f.key(Key::PageDown);
        // Row 10 selected, 5 rows visible: the first visible row is 6.
        assert_eq!(f.mouse_down(15, 40 + 2, false), Event::Selected);
        assert_eq!(f.selected(list), Some(6));
    }

    #[test]
    fn enter_clicks_the_default_button_and_escape_cancels() {
        let (mut f, _, _) = form();
        assert_eq!(f.key(Key::Enter), Event::Clicked(Act::Ok));
        assert_eq!(f.key(Key::Escape), Event::Cancel);
        let mut plain: Form<Act> = Form::new(100, 100);
        plain.add(Rect::new(0, 0, 100, 100), Widget::List { rows: rows(2), selected: Some(0), scroll: 0 });
        assert_eq!(plain.key(Key::Enter), Event::Activated);
    }

    #[test]
    fn buttons_fire_on_release_inside() {
        let (mut f, _, _) = form();
        assert_eq!(f.mouse_down(110, 160, false), Event::None);
        assert_eq!(f.mouse_up(110, 160), Event::Clicked(Act::Cancel));
        f.mouse_down(110, 160, false);
        assert_eq!(f.mouse_up(5, 5), Event::None, "released outside");
        assert_eq!(f.mouse_up(110, 160), Event::None, "not pressed");
    }

    #[test]
    fn clicking_rows_selects_and_double_click_activates() {
        let (mut f, _, list) = form();
        let row2 = 40 + 2 + 2 * ROW_H + 1;
        assert_eq!(f.mouse_down(20, row2, false), Event::Selected);
        assert_eq!(f.selected(list), Some(2));
        assert_eq!(f.mouse_down(20, row2, true), Event::Activated);
        f.set_rows(list, rows(1), Some(0));
        assert_eq!(f.mouse_down(20, row2, false), Event::None, "below the last row");
    }

    #[test]
    fn check_boxes_toggle() {
        let mut f: Form<Act> = Form::new(100, 40);
        let c = f.add(Rect::new(0, 0, 100, 20), Widget::Check { label: "Flag".into(), checked: false, action: Act::Flag });
        assert_eq!(f.mouse_down(5, 5, false), Event::Toggled(Act::Flag, true));
        assert_eq!(f.mouse_down(5, 5, false), Event::Toggled(Act::Flag, false));
        assert!(matches!(f.widget(c), Widget::Check { checked: false, .. }));
    }

    #[test]
    fn tab_moves_focus_between_field_and_list() {
        let (mut f, input, _) = form();
        f.key(Key::Tab);
        assert_eq!(f.key(Key::Char('a')), Event::None, "the list has the keyboard");
        f.key(Key::Tab);
        f.key(Key::Char('a'));
        assert_eq!(f.text(input), "a");
    }

    #[test]
    fn wheel_scrolls_within_bounds() {
        let (mut f, _, list) = form();
        f.wheel(-100);
        // 20 rows, 5 visible: the last page starts at row 15.
        assert_eq!(f.mouse_down(15, 40 + 2, false), Event::Selected);
        assert_eq!(f.selected(list), Some(15));
        f.wheel(100);
        f.mouse_down(15, 40 + 2, false);
        assert_eq!(f.selected(list), Some(0));
    }

    #[test]
    fn new_rows_clamp_the_selection() {
        let (mut f, _, list) = form();
        f.set_rows(list, rows(3), Some(10));
        assert_eq!(f.selected(list), Some(2));
        f.set_rows(list, vec![], Some(0));
        assert_eq!(f.selected(list), None);
        assert_eq!(f.key(Key::Down), Event::None);
    }
}
