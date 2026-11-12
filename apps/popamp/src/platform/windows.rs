//! Win32: window styles through winit, shapes, the bar's strip and work
//! areas through the system calls.

use super::{shape_rects, Mask, WindowRole};
use crate::bar::{Screen, Strip};
use popamp_core::config::BarEdge;
use std::cell::Cell;
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, DeleteObject, EnumDisplayMonitors, GetMonitorInfoW, SetWindowRgn, HDC, HMONITOR, MONITORINFO, RGN_OR,
};
use windows_sys::Win32::UI::Shell::{SHAppBarMessage, ABE_BOTTOM, ABE_TOP, ABM_NEW, ABM_REMOVE, ABM_SETPOS, APPBARDATA};
use winit::platform::windows::WindowAttributesExtWindows;
use winit::window::{Window, WindowAttributes, WindowLevel};

/// Fixed-size window attributes for `role`, borderless but for dialogs.
/// Only the main window (or the bar that replaces it) is in the taskbar.
pub fn attributes(role: WindowRole) -> WindowAttributes {
    let attrs = Window::default_attributes().with_decorations(false).with_resizable(false).with_class_name("popamp");
    let above = |a: WindowAttributes| a.with_skip_taskbar(true).with_window_level(WindowLevel::AlwaysOnTop);
    match role {
        WindowRole::Main | WindowRole::Bar => attrs,
        WindowRole::Docked => attrs.with_skip_taskbar(true),
        // Shown without taking the focus from the window it opened from.
        WindowRole::Popup => above(attrs).with_active(false),
        WindowRole::Tooltip => above(attrs).with_active(false),
        WindowRole::Dialog => attrs.with_decorations(true).with_skip_taskbar(true),
    }
}

/// Sends standard output and errors to the console of the program that
/// started this one, if any: a window program has none of its own, and
/// `/STATUS` or `--version` from cmd or PowerShell should still print
/// there. Output redirected to a file or pipe stays where it was.
pub fn use_parent_console() {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
    };
    // SAFETY: plain calls on this process's standard handles.
    unsafe {
        let given = [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE].map(|n| (n, GetStdHandle(n)));
        // Attaching replaces every standard handle; a redirected one goes back.
        if AttachConsole(ATTACH_PARENT_PROCESS) != 0 {
            for (n, h) in given.into_iter().filter(|(_, h)| !h.is_null() && *h != INVALID_HANDLE_VALUE) {
                SetStdHandle(n, h);
            }
        }
    }
}

/// Windows minimizes and restores owned windows with their owner, so only
/// the main window is minimized or restored by hand.
pub const OWNED_FOLLOW_OWNER: bool = true;

/// Nothing to connect to: the calls go straight to the system. Holds the
/// window registered as an application desktop toolbar, if any.
pub struct Platform {
    /// The bar window the shell keeps a strip for; null for none.
    bar: Cell<HWND>,
}

impl Platform {
    pub fn connect() -> Platform {
        Platform { bar: Cell::new(std::ptr::null_mut()) }
    }

    /// Shows only the pixels `mask` covers of a window `size` screen pixels
    /// big; all of it without a mask.
    pub fn set_shape(&self, window: &Window, mask: Option<&Mask>, size: (i32, i32)) -> Result<(), String> {
        let Some(hwnd) = hwnd(window) else { return Ok(()) };
        // SAFETY: plain GDI calls on a live window; a region given to
        // SetWindowRgn belongs to the system, the others are deleted here.
        unsafe {
            let region = match mask {
                None => std::ptr::null_mut(),
                Some(_) => {
                    let region = CreateRectRgn(0, 0, 0, 0);
                    for (x, y, w, h) in shape_rects(mask, size) {
                        let (x, y) = (x as i32, y as i32);
                        let run = CreateRectRgn(x, y, x + w as i32, y + h as i32);
                        CombineRgn(region, region, run, RGN_OR);
                        DeleteObject(run);
                    }
                    region
                }
            };
            if SetWindowRgn(hwnd, region, 1) == 0 {
                if !region.is_null() {
                    DeleteObject(region);
                }
                return Err("SetWindowRgn failed".into());
            }
        }
        Ok(())
    }

    /// Reserves `strip` for the bar `window`, as an application desktop
    /// toolbar like the taskbar: the shell takes it out of the work area.
    pub fn reserve(&self, window: &Window, strip: Strip) -> Result<(), String> {
        let Some(hwnd) = hwnd(window) else { return Ok(()) };
        if self.bar.get() != hwnd {
            self.unregister();
            // SAFETY: a filled APPBARDATA for a live window.
            if unsafe { SHAppBarMessage(ABM_NEW, &mut appbar(hwnd)) } == 0 {
                return Err("the shell refused a new application bar".into());
            }
            self.bar.set(hwnd);
        }
        let Screen { x, y, w, h } = strip.area();
        let mut data = appbar(hwnd);
        data.uEdge = match strip.edge {
            BarEdge::Top => ABE_TOP,
            BarEdge::Bottom => ABE_BOTTOM,
        };
        data.rc = RECT { left: x, top: y, right: x + w, bottom: y + h };
        // SAFETY: as above; the call only reads `data`.
        unsafe { SHAppBarMessage(ABM_SETPOS, &mut data) };
        Ok(())
    }

    /// Nothing to do: owned windows come up with their owner.
    pub fn raise_together(&self, _windows: &[&Window]) {}

    /// Gives back the strip of `window`, a bar about to go.
    pub fn release(&self, window: &Window) {
        if hwnd(window).is_some_and(|h| h == self.bar.get()) {
            self.unregister();
        }
    }

    fn unregister(&self) {
        let hwnd = self.bar.replace(std::ptr::null_mut());
        if !hwnd.is_null() {
            // SAFETY: a filled APPBARDATA; the shell drops the window's strip.
            unsafe { SHAppBarMessage(ABM_REMOVE, &mut appbar(hwnd)) };
        }
    }

    /// Makes `owner` the owner of `window` (none: no owner). Windows hides
    /// an owned window while its owner is minimized and shows it again with
    /// the owner, so the equalizer and the playlist come back when the main
    /// window is restored from the taskbar.
    pub fn attach(&self, window: &Window, owner: Option<&Window>) -> Result<(), String> {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowLongPtrW, GWLP_HWNDPARENT};
        let Some(child) = hwnd(window) else { return Ok(()) };
        let owner = owner.and_then(hwnd).map_or(0, |h| h as isize);
        // SAFETY: both are live windows of this thread; GWLP_HWNDPARENT of
        // a top-level window sets its owner. The previous value is 0 on
        // success as well as on failure, so failure is not reported.
        unsafe { SetWindowLongPtrW(child, GWLP_HWNDPARENT, owner) };
        Ok(())
    }

    /// Keeps a menu or tooltip from taking the focus when it is clicked:
    /// the window it opened from stays active and keeps the keyboard, and
    /// does not close the menu for losing the focus to it.
    pub fn no_activate(&self, window: &Window) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE};
        let Some(hwnd) = hwnd(window) else { return };
        // SAFETY: plain style calls on a live window of this thread.
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_NOACTIVATE as isize);
        }
    }

    /// Each monitor's work area: the monitor less the taskbar and other
    /// docked bars.
    pub fn work_areas(&self) -> Vec<Screen> {
        unsafe extern "system" fn each(monitor: HMONITOR, _: HDC, _: *mut RECT, out: LPARAM) -> i32 {
            // SAFETY: `out` is the Vec passed to EnumDisplayMonitors below,
            // alive and not otherwise borrowed during the call.
            let out = unsafe { &mut *(out as *mut Vec<Screen>) };
            let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..unsafe { std::mem::zeroed() } };
            // SAFETY: `info` is a MONITORINFO with its size set.
            if unsafe { GetMonitorInfoW(monitor, &mut info) } != 0 {
                let r = info.rcWork;
                out.push(Screen { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top });
            }
            1
        }
        let mut out: Vec<Screen> = Vec::new();
        // SAFETY: the callback only touches `out`, which outlives the call.
        unsafe { EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(each), &mut out as *mut _ as LPARAM) };
        out
    }
}

impl Drop for Platform {
    fn drop(&mut self) {
        self.unregister();
    }
}

/// The toolbar data of `hwnd` with nothing else set.
fn appbar(hwnd: HWND) -> APPBARDATA {
    // SAFETY: APPBARDATA is plain data, valid all zero.
    APPBARDATA { cbSize: size_of::<APPBARDATA>() as u32, hWnd: hwnd, ..unsafe { std::mem::zeroed() } }
}

fn hwnd(w: &Window) -> Option<HWND> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match w.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get() as HWND),
        _ => None,
    }
}
