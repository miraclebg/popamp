//! Win32: window styles through winit, shapes and work areas through the
//! system calls.

use super::{shape_rects, Mask, WindowRole};
use crate::bar::{Screen, Strip};
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, DeleteObject, EnumDisplayMonitors, GetMonitorInfoW, SetWindowRgn, HDC, HMONITOR, MONITORINFO, RGN_OR,
};
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
        WindowRole::Popup => above(attrs),
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

/// Nothing to connect to: the calls go straight to the system.
pub struct Platform;

impl Platform {
    pub fn connect() -> Platform {
        Platform
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

    /// Keeping other windows out of a bar's strip is not done on Windows
    /// yet: the bar sits inside the work area, and others may cover it.
    pub fn reserve(&self, _window: &Window, _strip: Strip) -> Result<(), String> {
        Ok(())
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

fn hwnd(w: &Window) -> Option<HWND> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match w.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get() as HWND),
        _ => None,
    }
}
