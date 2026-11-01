//! What the names in a skin layout mean in the player: the flags, values,
//! texts and menus that elements show or change.

use crate::menu::MenuItem;
use crate::model::Model;
use linamp_audio::PlayState;
use linamp_core::text::format_duration_ms;
use linamp_core::Command;
use linamp_skin::layout::{Cond, Dim, Flag, MenuRef, Text, Value, WindowKey};

pub fn flag(model: &Model, f: Flag) -> bool {
    let s = model.settings();
    let sh = model.engine.shared();
    let state = sh.state();
    let running = state != PlayState::Stopped;
    match f {
        Flag::Playing => state == PlayState::Playing,
        Flag::Paused => state == PlayState::Paused,
        Flag::Stopped => !running,
        Flag::Seekable => running && sh.length_ms().is_some_and(|l| l > 0),
        Flag::Stereo => running && sh.channels() >= 2,
        Flag::Mono => running && sh.channels() < 2,
        Flag::Shuffle => s.shuffle,
        Flag::Repeat => s.repeat,
        Flag::EqWindow => s.eq_open,
        Flag::PlaylistWindow => s.pe_open,
        Flag::AlwaysOnTop => s.aot,
        Flag::DoubleSize => s.dsize,
        Flag::EqOn => s.use_eq,
        Flag::EqAutoload => s.autoload_eq,
    }
}

pub fn holds(model: &Model, c: Cond, width: i32) -> bool {
    match c {
        Cond::Always => true,
        Cond::If(f) => flag(model, f),
        Cond::Unless(f) => !flag(model, f),
        Cond::MinWidth(w) => width >= w,
    }
}

/// A value with its range, or `None` while it has none (position of a
/// stream of unknown length).
pub fn value(model: &Model, v: Value) -> Option<(i64, i64, i64)> {
    let s = model.settings();
    Some(match v {
        Value::Volume => (s.volume as i64, 0, 255),
        Value::Balance => (s.balance as i64, -127, 127),
        Value::Position => {
            let sh = model.engine.shared();
            let len = sh.length_ms().filter(|&l| l > 0)? as i64;
            (sh.position_ms() as i64, 0, len)
        }
        Value::Preamp => (s.preamp as i64, 0, 63),
        Value::EqBand(i) => (s.eq_bands()[i as usize] as i64, 0, 63),
    })
}

pub fn set_value(model: &mut Model, v: Value, x: i64) {
    match v {
        Value::Volume => model.set_volume(x as i32),
        Value::Balance => model.set_balance(x as i32),
        Value::Position => model.seek_to(x.max(0) as u64),
        Value::Preamp => model.set_preamp(x as i32),
        Value::EqBand(i) => model.set_eq_band(i as usize, x as i32),
    }
}

/// The text to show, or `None` to leave the background as it is.
pub fn text(model: &Model, t: Text) -> Option<String> {
    let sh = model.engine.shared();
    let running = sh.state() != PlayState::Stopped;
    match t {
        Text::Kbps => Some(sh.bitrate_kbps()).filter(|&k| running && k > 0).map(|k| format!("{:>3}", k.min(999))),
        Text::Khz => Some(sh.sample_rate()).filter(|&r| running && r > 0).map(|r| format!("{:>2}", (r / 1000).min(99))),
        Text::ShortTime => running.then(|| {
            let secs = sh.position_ms() / 1000;
            format!("{:>2}:{:02}", (secs / 60).min(99), secs % 60)
        }),
        Text::Title => Some(model.current_title()),
        Text::PlaylistTime => {
            let (sel_ms, total_ms, unknown) = model.playlist.durations();
            let plus = if unknown { "+" } else { "" };
            Some(format!("{}/{}{}", format_duration_ms(sel_ms), format_duration_ms(total_ms), plus))
        }
        Text::PlaylistClock => running.then(|| {
            let pos = sh.position_ms();
            let shown = match (model.settings().timeleftmode, sh.length_ms()) {
                (true, Some(l)) => format!("-{}", format_duration_ms(l.saturating_sub(pos))),
                _ => format_duration_ms(pos),
            };
            format!("{shown:>6}")
        }),
    }
}

pub fn menu(model: &Model, m: MenuRef) -> Vec<MenuItem> {
    use Command as C;
    let s = model.settings();
    match m {
        MenuRef::Main => model.main_menu(),
        MenuRef::Options => model.options_menu(),
        MenuRef::Vis => model.vis_menu(),
        MenuRef::EqPresets => model.eq_presets_menu(),
        MenuRef::PlaylistRow => model.playlist_row_menu(),
        MenuRef::PlaylistMisc => model.playlist_misc_menu(),
        MenuRef::PlaylistSort => model.sort_menu(),
        MenuRef::PlaylistRemoveMisc => vec![
            MenuItem::cmd("Remove missing files", "Alt+Del", C::RemoveMissing),
            MenuItem::cmd("Physically remove selected file(s)…", "", C::PhysicallyRemove),
        ],
        MenuRef::Previous => vec![
            MenuItem::cmd("Previous", "Click", C::Previous),
            MenuItem::cmd("Start of list", "Ctrl+Click", C::StartOfList),
            MenuItem::cmd("Rewind 5 seconds", "Shift+Click", C::Back5s),
        ],
        MenuRef::Play => vec![
            MenuItem::cmd("Play/restart", "Click", C::Play),
            MenuItem::cmd("Open URL…", "Ctrl+Click", C::OpenUrl),
            MenuItem::cmd("Open file…", "Shift+Click", C::OpenFile),
        ],
        MenuRef::Pause => vec![MenuItem::cmd("Pause/Unpause", "Click", C::Pause)],
        MenuRef::Stop => vec![
            MenuItem::cmd("Stop", "Click", C::Stop),
            MenuItem::cmd("Stop w/ fadeout", "Shift+Click", C::StopFadeout),
            MenuItem::cmd("Stop after current", "Ctrl+Click", C::StopAfterCurrent).checked(model.stop_after_current),
        ],
        MenuRef::Next => vec![
            MenuItem::cmd("Next", "Click", C::Next),
            MenuItem::cmd("Fast-forward 5 seconds", "Shift+Click", C::Fwd5s),
            MenuItem::cmd("End of list", "Ctrl+Click", C::EndOfList),
        ],
        MenuRef::Eject => vec![
            MenuItem::cmd("Open file…", "Click", C::OpenFile),
            MenuItem::cmd("Open folder…", "Shift+Click", C::OpenFolder),
            MenuItem::cmd("Open URL…", "Ctrl+Click", C::OpenUrl),
        ],
        MenuRef::Seek => vec![
            MenuItem::cmd("Jump to time…", "Ctrl+J", C::JumpToTime),
            MenuItem::cmd("Rewind 5 seconds", "Left", C::Back5s),
            MenuItem::cmd("Forward 5 seconds", "Right", C::Fwd5s),
        ],
        MenuRef::Repeat => vec![
            MenuItem::cmd("Repeat", "R", C::ToggleRepeat).checked(s.repeat),
            MenuItem::cmd("Manual playlist advance", "Shift+R", C::ToggleManualAdvance).checked(!s.pladv),
        ],
        MenuRef::Shuffle => vec![MenuItem::cmd("Shuffle", "S", C::ToggleShuffle).checked(s.shuffle)],
        MenuRef::Time => vec![
            MenuItem::cmd("Time elapsed", "Ctrl+T", C::TimeElapsed).checked(!s.timeleftmode),
            MenuItem::cmd("Time remaining", "Ctrl+T", C::TimeRemaining).checked(s.timeleftmode),
        ],
        MenuRef::SongTitle => vec![
            MenuItem::cmd("View file info…", "Alt+3", C::FileInfo),
            MenuItem::cmd("Jump to file…", "J", C::JumpToFile),
            MenuItem::cmd("Jump to time…", "Ctrl+J", C::JumpToTime),
            MenuItem::cmd("Autoscroll song name", "", C::ToggleAutoscroll).checked(s.autoscrollname),
        ],
    }
}

/// Whether the window shows its shade mode.
pub fn shaded(model: &Model, key: WindowKey) -> bool {
    let s = model.settings();
    match key {
        WindowKey::Main => s.windowshade,
        WindowKey::Eq => s.eq_ws,
        WindowKey::Playlist => s.pe_ws,
    }
}

/// The window's stored size, for resizable dimensions.
pub fn stored_size(model: &Model, key: WindowKey) -> (i32, i32) {
    let s = model.settings();
    match key {
        WindowKey::Playlist => (s.pe_width, s.pe_height),
        _ => (0, 0),
    }
}

pub fn window_size(model: &Model, key: WindowKey) -> (i32, i32) {
    let Some(def) = model.skin.layout.window(key) else { return (0, 0) };
    let mode = def.mode(shaded(model, key));
    let (sw, sh) = stored_size(model, key);
    let len = |d: Dim, stored: i32| match d {
        Dim::Fixed(n) => n,
        Dim::Free { .. } => stored,
    };
    (len(mode.width, sw), len(mode.height, sh))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_come_from_the_layout_and_settings() {
        let mut m = Model::for_tests();
        assert_eq!(window_size(&m, WindowKey::Main), (275, 116));
        m.settings_mut().windowshade = true;
        assert_eq!(window_size(&m, WindowKey::Main), (275, 14));
        m.settings_mut().pe_width = 450;
        m.settings_mut().pe_height = 290;
        assert_eq!(window_size(&m, WindowKey::Playlist), (450, 290));
        m.settings_mut().pe_ws = true;
        assert_eq!(window_size(&m, WindowKey::Playlist), (450, 14));
    }

    #[test]
    fn stopped_player_shows_no_play_details() {
        let m = Model::for_tests();
        assert!(flag(&m, Flag::Stopped) && !flag(&m, Flag::Seekable) && !flag(&m, Flag::Stereo) && !flag(&m, Flag::Mono));
        assert_eq!(text(&m, Text::Kbps), None);
        assert_eq!(text(&m, Text::ShortTime), None);
        assert_eq!(value(&m, Value::Position), None);
        assert_eq!(value(&m, Value::Balance), Some((0, -127, 127)));
    }
}
