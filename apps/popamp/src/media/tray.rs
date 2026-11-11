//! The tray icon on Unix: a StatusNotifierItem, which KDE, Xfce, Cinnamon,
//! MATE and GNOME's AppIndicator extension show. A click brings Popamp up,
//! the wheel sets the volume, and the menu has the transport buttons.

use super::Snapshot;
use crate::cli::Remote;
use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::{CheckmarkItem, MenuItem, StandardItem};
use popamp_core::Command;
use std::sync::Arc;

/// Sends remote commands to the UI thread.
pub type Send = Arc<dyn Fn(Vec<Remote>) + std::marker::Send + Sync>;

/// How far one notch of the wheel moves the volume, in percent.
const WHEEL_STEP: i32 = 2;

struct Item {
    send: Send,
    snap: Snapshot,
    icons: Arc<Vec<ksni::Icon>>,
    /// KDE sends the wheel turned up as a positive delta, GNOME as negative.
    up_is_positive: bool,
}

impl Item {
    fn on(&self, remote: Remote) -> Box<dyn Fn(&mut Item) + std::marker::Send> {
        Box::new(move |item: &mut Item| (item.send)(vec![remote.clone()]))
    }

    fn button(&self, label: &str, command: Command) -> MenuItem<Item> {
        StandardItem { label: label.into(), activate: self.on(Remote::Cmd(command)), ..Default::default() }.into()
    }

    fn check(&self, label: &str, checked: bool, remote: Remote) -> MenuItem<Item> {
        CheckmarkItem { label: label.into(), checked, activate: self.on(remote), ..Default::default() }.into()
    }
}

impl ksni::Tray for Item {
    fn id(&self) -> String {
        "popamp".into()
    }

    fn title(&self) -> String {
        crate::edition::NAME.into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icons.to_vec()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip { title: crate::edition::NAME.into(), description: now_playing(&self.snap), ..Default::default() }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        (self.send)(vec![Remote::Show]);
    }

    fn secondary_activate(&mut self, _x: i32, _y: i32) {
        (self.send)(vec![Remote::Cmd(Command::PlayPause)]);
    }

    fn scroll(&mut self, delta: i32, orientation: ksni::Orientation) {
        if orientation == ksni::Orientation::Vertical && delta != 0 {
            let up = (delta > 0) == self.up_is_positive;
            (self.send)(vec![Remote::VolumeStep(if up { WHEEL_STEP } else { -WHEEL_STEP })]);
        }
    }

    fn menu(&self) -> Vec<MenuItem<Item>> {
        let s = &self.snap;
        vec![
            StandardItem { label: "Show Popamp".into(), activate: self.on(Remote::Show), ..Default::default() }.into(),
            MenuItem::Separator,
            self.button("Previous", Command::Previous),
            self.button("Play", Command::Play),
            self.button("Pause", Command::Pause),
            self.button("Stop", Command::Stop),
            self.button("Next", Command::Next),
            MenuItem::Separator,
            self.check("Shuffle", s.shuffle, Remote::Shuffle(Some(!s.shuffle))),
            self.check("Repeat", s.repeat, Remote::Repeat(Some(!s.repeat))),
            MenuItem::Separator,
            StandardItem { label: "Exit".into(), activate: self.on(Remote::Quit), ..Default::default() }.into(),
        ]
    }
}

/// What the tooltip says under the name.
fn now_playing(snap: &Snapshot) -> String {
    let Some(track) = &snap.track else { return "Stopped".into() };
    let line = match super::byline(track) {
        by if by.is_empty() => track.title.clone(),
        by => format!("{}\n{by}", track.title),
    };
    match snap.status {
        "Playing" => line,
        status => format!("{line}\n({status})"),
    }
}

/// The logo, drawn for each size the tray may want: a host picks the
/// nearest, so small trays get the small logo, not the big one shrunk.
fn icons() -> Vec<ksni::Icon> {
    let pngs: [&[u8]; 5] = [
        include_bytes!("../../../../assets/logo/png/popamp-16.png"),
        include_bytes!("../../../../assets/logo/png/popamp-24.png"),
        include_bytes!("../../../../assets/logo/png/popamp-32.png"),
        include_bytes!("../../../../assets/logo/png/popamp-48.png"),
        include_bytes!("../../../../assets/logo/png/popamp-64.png"),
    ];
    pngs.iter().filter_map(|png| image::load_from_memory(png).ok()).map(|img| argb(&img.to_rgba8())).collect()
}

/// RGBA pixels as the ARGB32 in network byte order the spec asks for.
fn argb(img: &image::RgbaImage) -> ksni::Icon {
    let data = img.pixels().flat_map(|p| [p[3], p[0], p[1], p[2]]).collect();
    ksni::Icon { width: img.width() as i32, height: img.height() as i32, data }
}

/// The tray icon while the user wants one.
pub struct Tray {
    send: Send,
    shown: Shown,
    icons: Arc<Vec<ksni::Icon>>,
}

enum Shown {
    No,
    Yes(Handle<Item>),
    /// No tray on this desktop: not asked again until turned off and on.
    Refused,
}

impl Tray {
    pub fn new(send: Send) -> Tray {
        Tray { send, shown: Shown::No, icons: Arc::new(icons()) }
    }

    /// Shows, updates or takes down the icon to match `snap`.
    pub fn update(&mut self, snap: &Snapshot) {
        match (&self.shown, snap.tray) {
            (Shown::Yes(handle), true) => {
                handle.update(|item| item.snap = snap.clone());
            }
            (Shown::No, true) => self.shown = self.spawn(snap),
            (Shown::Yes(handle), false) => {
                handle.shutdown().wait();
                self.shown = Shown::No;
            }
            (Shown::Refused, false) => self.shown = Shown::No,
            (Shown::No, false) | (Shown::Refused, true) => {}
        }
    }

    fn spawn(&self, snap: &Snapshot) -> Shown {
        let kde = std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.split(':').any(|d| d.eq_ignore_ascii_case("KDE")));
        let item = Item { send: self.send.clone(), snap: snap.clone(), icons: self.icons.clone(), up_is_positive: kde };
        match item.spawn() {
            Ok(handle) => Shown::Yes(handle),
            Err(e) => {
                log::info!("no tray icon: {e}");
                Shown::Refused
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::Track;
    use ksni::Tray as _;
    use parking_lot::Mutex;

    fn item(snap: Snapshot) -> (Item, Arc<Mutex<Vec<Remote>>>) {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let log = sent.clone();
        let send: Send = Arc::new(move |r| log.lock().extend(r));
        (Item { send, snap, icons: Arc::new(Vec::new()), up_is_positive: false }, sent)
    }

    /// Clicks the menu entry labelled `label`.
    fn click(item: &mut Item, label: &str) {
        let entry = item.menu().into_iter().find_map(|m| match m {
            MenuItem::Standard(s) if s.label == label => Some(s.activate),
            MenuItem::Checkmark(c) if c.label == label => Some(c.activate),
            _ => None,
        });
        entry.expect(label)(item);
    }

    #[test]
    fn the_menu_and_the_icon_drive_the_player() {
        let (mut item, sent) = item(Snapshot { shuffle: true, ..Snapshot::default() });
        click(&mut item, "Next");
        click(&mut item, "Shuffle");
        click(&mut item, "Repeat");
        click(&mut item, "Exit");
        item.activate(0, 0);
        item.secondary_activate(0, 0);
        item.scroll(-1, ksni::Orientation::Vertical);
        item.scroll(3, ksni::Orientation::Vertical);
        item.scroll(1, ksni::Orientation::Horizontal);
        let want = [
            Remote::Cmd(Command::Next),
            Remote::Shuffle(Some(false)),
            Remote::Repeat(Some(true)),
            Remote::Quit,
            Remote::Show,
            Remote::Cmd(Command::PlayPause),
            Remote::VolumeStep(2),
            Remote::VolumeStep(-2),
        ];
        assert_eq!(*sent.lock(), want);
        sent.lock().clear();
        item.up_is_positive = true;
        item.scroll(5, ksni::Orientation::Vertical);
        assert_eq!(*sent.lock(), [Remote::VolumeStep(2)], "KDE's way up");
    }

    #[test]
    fn the_tooltip_says_what_plays() {
        let track = Track { title: "Song".into(), artist: Some("Band".into()), ..Track::default() };
        let snap = |status| Snapshot { status, track: Some(track.clone()), ..Snapshot::default() };
        assert_eq!(now_playing(&Snapshot::default()), "Stopped");
        assert_eq!(now_playing(&snap("Playing")), "Song\nBand");
        assert_eq!(now_playing(&snap("Paused")), "Song\nBand\n(Paused)");
        let bare = Snapshot { track: Some(Track { title: "Stream".into(), ..Track::default() }), status: "Playing", ..Snapshot::default() };
        assert_eq!(now_playing(&bare), "Stream");
    }

    #[test]
    fn every_logo_size_is_an_icon() {
        let icons = icons();
        assert_eq!(icons.iter().map(|i| i.width).collect::<Vec<_>>(), [16, 24, 32, 48, 64]);
        let one = argb(&image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 4])));
        assert_eq!(one.data, [4, 1, 2, 3], "alpha first");
    }
}
