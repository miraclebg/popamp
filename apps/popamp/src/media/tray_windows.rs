//! The tray icon on Windows: a notification area icon owned by a hidden
//! window on a thread of its own. A click brings Popamp up, a middle
//! click plays or pauses, and the right button opens the menu.

use super::{now_playing, tray_menu, Sender, Snapshot};
use crate::cli::Remote;
use parking_lot::Mutex;
use popamp_core::Command;
use std::cell::RefCell;
use std::sync::Arc;
use std::thread::JoinHandle;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconFromResourceEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetCursorPos, GetMessageW, GetSystemMetrics, PostMessageW, PostQuitMessage, RegisterClassW, RegisterWindowMessageW,
    SetForegroundWindow, TrackPopupMenu, HICON, LR_DEFAULTCOLOR, MF_CHECKED, MF_SEPARATOR, MF_STRING, MSG, SM_CXSMICON, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_CLOSE, WM_DESTROY, WM_LBUTTONUP, WM_MBUTTONUP, WM_NULL, WM_RBUTTONUP, WNDCLASSW,
};

/// The icon's mouse messages.
const WM_TRAY: u32 = WM_APP + 1;
/// A new snapshot to show.
const WM_REFRESH: u32 = WM_APP + 2;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// What the window thread keeps; its window procedure reads it.
struct State {
    send: Sender,
    snap: Arc<Mutex<Snapshot>>,
    data: NOTIFYICONDATAW,
    /// Sent to every window when Explorer starts again, icons all gone.
    taskbar_created: u32,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// A tray icon on show; it goes when dropped.
struct Shown {
    window: HWND,
    snap: Arc<Mutex<Snapshot>>,
    thread: Option<JoinHandle<()>>,
}

impl Shown {
    fn new(send: Sender) -> Option<Shown> {
        let snap = Arc::new(Mutex::new(Snapshot::default()));
        let (tx, rx) = crossbeam_channel::bounded(1);
        let shared = snap.clone();
        let thread = std::thread::Builder::new()
            .name("popamp-tray-window".into())
            .spawn(move || {
                let window = create(send, shared);
                let _ = tx.send(window.map(|w| w as usize));
                if window.is_some() {
                    pump();
                }
            })
            .map_err(|e| log::warn!("no tray icon: {e}"))
            .ok()?;
        match rx.recv().ok().flatten() {
            Some(window) => Some(Shown { window: window as HWND, snap, thread: Some(thread) }),
            None => {
                let _ = thread.join();
                None
            }
        }
    }

    fn refresh(&self, snap: &Snapshot) {
        *self.snap.lock() = snap.clone();
        // SAFETY: the window is alive until this is dropped; a post to a
        // window that is gone fails harmlessly.
        unsafe { PostMessageW(self.window, WM_REFRESH, 0, 0) };
    }

    #[cfg(test)]
    fn click(&self, button: u32) {
        // SAFETY: as in `refresh`.
        unsafe { PostMessageW(self.window, WM_TRAY, 0, button as LPARAM) };
    }
}

impl Drop for Shown {
    fn drop(&mut self) {
        // SAFETY: as in `refresh`; closing ends the thread's loop.
        if unsafe { PostMessageW(self.window, WM_CLOSE, 0, 0) } != 0 {
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

/// Makes the hidden window and puts the icon up. The window stays even if
/// the taskbar is not there yet: the icon comes when it is.
fn create(send: Sender, snap: Arc<Mutex<Snapshot>>) -> Option<HWND> {
    let class = wide("PopampTray");
    // SAFETY: plain calls with pointers to locals that outlive them; the
    // class is registered once per process, a second time fails harmlessly.
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSW { lpfnWndProc: Some(window_proc), hInstance: instance, lpszClassName: class.as_ptr(), ..std::mem::zeroed() };
        RegisterClassW(&wc);
        let title = wide(crate::edition::NAME);
        let window = CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        if window.is_null() {
            log::warn!("no tray icon: no window");
            return None;
        }
        let mut data: NOTIFYICONDATAW = std::mem::zeroed();
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = window;
        data.uID = 1;
        data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        data.uCallbackMessage = WM_TRAY;
        data.hIcon = icon();
        tip(&mut data, &snap.lock());
        if Shell_NotifyIconW(NIM_ADD, &data) == 0 {
            log::info!("the taskbar has no room for a tray icon yet");
        }
        let taskbar_created = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
        STATE.with(|s| *s.borrow_mut() = Some(State { send, snap, data, taskbar_created }));
        Some(window)
    }
}

fn pump() {
    // SAFETY: `msg` is ours; 0 is WM_QUIT, -1 an error.
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            DispatchMessageW(&msg);
        }
    }
}

/// The logo at the size of a small icon, from the nearest drawing at least
/// as big: the small logo for small icons, not the big one shrunk.
fn icon() -> HICON {
    let pngs: [(i32, &[u8]); 5] = [
        (16, include_bytes!("../../../../assets/logo/png/popamp-16.png")),
        (24, include_bytes!("../../../../assets/logo/png/popamp-24.png")),
        (32, include_bytes!("../../../../assets/logo/png/popamp-32.png")),
        (48, include_bytes!("../../../../assets/logo/png/popamp-48.png")),
        (64, include_bytes!("../../../../assets/logo/png/popamp-64.png")),
    ];
    // SAFETY: no arguments but a constant.
    let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.clamp(16, 64);
    let png = pngs.iter().find(|(s, _)| *s >= size).map_or(pngs[4].1, |p| p.1);
    // SAFETY: reads `png` only; 0x0003_0000 is the icon format version.
    unsafe { CreateIconFromResourceEx(png.as_ptr(), png.len() as u32, 1, 0x0003_0000, size, size, LR_DEFAULTCOLOR) }
}

/// Puts the name and what plays into the tooltip, cut to its room.
fn tip(data: &mut NOTIFYICONDATAW, snap: &Snapshot) {
    let text: Vec<u16> = format!("{}\n{}", crate::edition::NAME, now_playing(snap)).encode_utf16().take(data.szTip.len() - 1).collect();
    data.szTip = [0; 128];
    data.szTip[..text.len()].copy_from_slice(&text);
}

/// What a mouse button on the icon asks for, the menu aside.
fn clicked(button: u32) -> Option<Remote> {
    match button {
        WM_LBUTTONUP => Some(Remote::Show),
        WM_MBUTTONUP => Some(Remote::Cmd(Command::PlayPause)),
        _ => None,
    }
}

unsafe extern "system" fn window_proc(window: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // The menu runs a loop of its own that comes back here, so nothing is
    // borrowed while it is open.
    let state = STATE.with(|s| s.try_borrow().ok().and_then(|s| s.as_ref().map(|s| (s.send.clone(), s.snap.clone(), s.taskbar_created))));
    let Some((send, snap, taskbar_created)) = state else { return DefWindowProcW(window, msg, wparam, lparam) };
    match msg {
        WM_TRAY => {
            let button = (lparam & 0xffff) as u32;
            if button == WM_RBUTTONUP {
                menu(window, &send, &snap.lock().clone());
            } else if let Some(remote) = clicked(button) {
                send(vec![remote]);
            }
            0
        }
        WM_REFRESH => {
            with_icon(|data| {
                tip(data, &snap.lock());
                Shell_NotifyIconW(NIM_MODIFY, data);
            });
            0
        }
        WM_CLOSE => {
            DestroyWindow(window);
            0
        }
        WM_DESTROY => {
            with_icon(|data| {
                Shell_NotifyIconW(NIM_DELETE, data);
                DestroyIcon(data.hIcon);
            });
            STATE.with(|s| s.borrow_mut().take());
            PostQuitMessage(0);
            0
        }
        m if m == taskbar_created => {
            with_icon(|data| {
                Shell_NotifyIconW(NIM_ADD, data);
            });
            0
        }
        _ => DefWindowProcW(window, msg, wparam, lparam),
    }
}

fn with_icon(f: impl FnOnce(&mut NOTIFYICONDATAW)) {
    STATE.with(|s| {
        if let Some(state) = s.borrow_mut().as_mut() {
            f(&mut state.data);
        }
    });
}

/// Opens the menu at the pointer and does what was chosen.
fn menu(window: HWND, send: &Sender, snap: &Snapshot) {
    let entries = tray_menu(snap);
    // SAFETY: the menu is made, shown and destroyed here; the strings
    // outlive the calls that read them.
    let chosen = unsafe {
        let menu = CreatePopupMenu();
        for (n, entry) in entries.iter().enumerate() {
            match entry {
                Some(e) => {
                    let checked = if e.checked == Some(true) { MF_CHECKED } else { 0 };
                    AppendMenuW(menu, MF_STRING | checked, n + 1, wide(e.label).as_ptr());
                }
                None => {
                    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
                }
            }
        }
        let mut at = POINT { x: 0, y: 0 };
        GetCursorPos(&mut at);
        // Without it the menu would not close on a click elsewhere.
        SetForegroundWindow(window);
        let chosen = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON, at.x, at.y, 0, window, std::ptr::null());
        PostMessageW(window, WM_NULL, 0, 0);
        DestroyMenu(menu);
        chosen
    };
    if let Some(Some(entry)) = usize::try_from(chosen).ok().and_then(|n| n.checked_sub(1)).and_then(|n| entries.get(n)) {
        send(vec![entry.remote.clone()]);
    }
}

/// The tray icon while the user wants one.
pub struct Tray {
    send: Sender,
    shown: Option<Option<Shown>>,
}

impl Tray {
    pub fn new(send: Sender) -> Tray {
        Tray { send, shown: None }
    }

    /// Shows, updates or takes down the icon to match `snap`.
    pub fn update(&mut self, snap: &Snapshot) {
        let send = &self.send;
        crate::hold::update(&mut self.shown, snap.tray, || Shown::new(send.clone()));
        if let Some(Some(shown)) = &self.shown {
            shown.refresh(snap);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_answers_clicks_until_taken_down() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut tray = Tray::new(Arc::new(move |r| r.into_iter().for_each(|r| tx.send(r).unwrap())));
        let on = Snapshot { tray: true, ..Snapshot::default() };
        tray.update(&on);
        let shown = tray.shown.as_ref().unwrap().as_ref().expect("a tray window");
        shown.click(WM_LBUTTONUP);
        shown.click(WM_MBUTTONUP);
        let timeout = std::time::Duration::from_secs(5);
        assert_eq!([rx.recv_timeout(timeout).unwrap(), rx.recv_timeout(timeout).unwrap()], [Remote::Show, Remote::Cmd(Command::PlayPause)]);
        tray.update(&Snapshot { tray: false, ..on });
        assert!(tray.shown.is_none(), "the window thread has ended");
    }

    #[test]
    fn the_tooltip_is_cut_to_its_room() {
        // SAFETY: plain data.
        let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        let long = "x".repeat(300);
        let snap =
            Snapshot { status: "Playing", track: Some(super::super::Track { title: long, ..Default::default() }), ..Snapshot::default() };
        tip(&mut data, &snap);
        assert_eq!(data.szTip.iter().position(|&c| c == 0), Some(127));
        assert_eq!(clicked(WM_RBUTTONUP), None, "the right button opens the menu");
    }
}
