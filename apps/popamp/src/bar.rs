//! A bar skin's main window: a strip across a monitor at its top or bottom
//! edge. Its strip is reserved like a taskbar's, so maximized windows keep
//! out of it; the playlist and the equalizer sit against it at the
//! monitor's right end.

use crate::model::Model;
use popamp_core::config::BarEdge;
use popamp_skin::layout::{Placement, WindowKey};

/// A monitor's area in screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Screen {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// The edge the bar is at; `None` when the skin's main window is no bar.
pub fn edge(model: &Model) -> Option<BarEdge> {
    match model.skin.layout.window(WindowKey::Main)?.placement {
        Placement::Bar(skin_edge) => Some(model.settings().bar_edge.unwrap_or(skin_edge)),
        _ => None,
    }
}

/// Top-left corners of the windows in [`WindowKey::ALL`] order, from their
/// sizes in screen pixels: the bar across `screen` at `edge`, the playlist
/// at the right end against the bar and the equalizer left of the playlist,
/// or at the right end while the playlist is closed.
pub fn places(edge: BarEdge, screen: Screen, sizes: [(i32, i32); 3], pl_open: bool) -> [(i32, i32); 3] {
    let [(_, bar_h), (eq_w, eq_h), (pl_w, pl_h)] = sizes;
    let against = |h: i32| match edge {
        BarEdge::Top => screen.y + bar_h,
        BarEdge::Bottom => screen.y + screen.h - bar_h - h,
    };
    let right = screen.x + screen.w;
    let pl_x = right - pl_w;
    let eq_x = if pl_open { pl_x - eq_w } else { right - eq_w };
    let bar_y = match edge {
        BarEdge::Top => screen.y,
        BarEdge::Bottom => screen.y + screen.h - bar_h,
    };
    [(screen.x, bar_y), (eq_x, against(eq_h)), (pl_x, against(pl_h))]
}

/// `_NET_WM_STRUT_PARTIAL` for a bar `bar_h` pixels high: the strip it
/// takes from the root window, `root_h` pixels high, at `edge` of `screen`.
pub fn strut(edge: BarEdge, screen: Screen, root_h: i32, bar_h: i32) -> [u32; 12] {
    let (start, end) = (screen.x as u32, (screen.x + screen.w - 1) as u32);
    match edge {
        BarEdge::Top => [0, 0, (screen.y + bar_h) as u32, 0, 0, 0, 0, 0, start, end, 0, 0],
        BarEdge::Bottom => [0, 0, 0, (root_h - screen.y - screen.h + bar_h) as u32, 0, 0, 0, 0, 0, 0, start, end],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RIGHT_MONITOR: Screen = Screen { x: 1920, y: 0, w: 1280, h: 1024 };
    const SIZES: [(i32, i32); 3] = [(1280, 28), (275, 116), (275, 232)];

    #[test]
    fn a_bottom_bar_has_the_playlist_and_equalizer_above_it_at_the_right() {
        let [bar, eq, pl] = places(BarEdge::Bottom, RIGHT_MONITOR, SIZES, true);
        assert_eq!(bar, (1920, 1024 - 28));
        assert_eq!(pl, (3200 - 275, 1024 - 28 - 232));
        assert_eq!(eq, (3200 - 550, 1024 - 28 - 116));
    }

    #[test]
    fn a_top_bar_has_them_below_it() {
        let [bar, eq, pl] = places(BarEdge::Top, RIGHT_MONITOR, SIZES, true);
        assert_eq!(bar, (1920, 0));
        assert_eq!((eq.1, pl.1), (28, 28));
    }

    #[test]
    fn the_equalizer_takes_the_corner_while_the_playlist_is_closed() {
        let [_, eq, _] = places(BarEdge::Bottom, RIGHT_MONITOR, SIZES, false);
        assert_eq!(eq.0, 3200 - 275);
    }

    #[test]
    fn struts_count_from_the_root_window_edges() {
        // The right monitor is shorter than the 1080-pixel root window.
        let bottom = strut(BarEdge::Bottom, RIGHT_MONITOR, 1080, 28);
        assert_eq!(bottom, [0, 0, 0, 56 + 28, 0, 0, 0, 0, 0, 0, 1920, 3199]);
        let top = strut(BarEdge::Top, RIGHT_MONITOR, 1080, 28);
        assert_eq!(top, [0, 0, 28, 0, 0, 0, 0, 0, 1920, 3199, 0, 0]);
    }

    #[test]
    fn the_edge_is_the_users_choice_or_the_skins() {
        let mut m = Model::for_tests();
        assert_eq!(edge(&m), None, "the default skin has no bar");
        m.skin.layout.windows[0].placement = Placement::Bar(BarEdge::Bottom);
        assert_eq!(edge(&m), Some(BarEdge::Bottom));
        m.settings_mut().bar_edge = Some(BarEdge::Top);
        assert_eq!(edge(&m), Some(BarEdge::Top));
    }
}
