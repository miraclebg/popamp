//! Global hotkeys: Ctrl+Alt with Insert, Home, End, Page Up/Down and the
//! arrows work from any program, as in Winamp. Grabbed through X11 on
//! Linux and RegisterHotKey on Windows, on a thread of their own; a key
//! another program holds already is left to it.

use popamp_core::Command;
use std::sync::Arc;

/// Sends a command to the UI thread.
pub type Send = Arc<dyn Fn(Command) + std::marker::Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Key {
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
}

/// Each key, pressed with Ctrl+Alt, and what it does.
const BINDINGS: [(Key, Command); 9] = [
    (Key::Insert, Command::Play),
    (Key::Home, Command::Pause),
    (Key::End, Command::Stop),
    (Key::PageUp, Command::Previous),
    (Key::PageDown, Command::Next),
    (Key::Up, Command::VolumeUp),
    (Key::Down, Command::VolumeDown),
    (Key::Left, Command::Rewind5s),
    (Key::Right, Command::Forward5s),
];

/// The hotkeys while the user wants them.
pub struct Hotkeys {
    send: Send,
    grab: Option<Option<os::Grab>>,
}

impl Hotkeys {
    pub fn new(send: Send) -> Hotkeys {
        Hotkeys { send, grab: None }
    }

    /// Grabs or lets go of the keys to match `on`. A grab that failed is
    /// tried again only after the setting is turned off and on.
    pub fn want(&mut self, on: bool) {
        crate::hold::update(&mut self.grab, on, || os::grab(self.send.clone()));
    }
}

/// X11's passive grabs on the root window. Under Wayland they only see
/// keys pressed in Xwayland's windows, so there are none.
#[cfg(unix)]
mod os {
    use super::{Key, Send, BINDINGS};
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        ClientMessageEvent, ConnectionExt as _, CreateWindowAux, EventMask, GrabMode, Keycode, Keysym, ModMask, Window, WindowClass,
    };
    use x11rb::protocol::Event;
    use x11rb::rust_connection::RustConnection;
    use x11rb::COPY_DEPTH_FROM_PARENT;

    pub struct Grab {
        conn: Arc<RustConnection>,
        /// Ours alone: a message to it ends the thread.
        wake: Window,
        thread: Option<JoinHandle<()>>,
    }

    impl Key {
        fn keysym(self) -> Keysym {
            match self {
                Key::Home => 0xff50,
                Key::Left => 0xff51,
                Key::Up => 0xff52,
                Key::Right => 0xff53,
                Key::Down => 0xff54,
                Key::PageUp => 0xff55,
                Key::PageDown => 0xff56,
                Key::End => 0xff57,
                Key::Insert => 0xff63,
            }
        }
    }

    pub fn grab(send: Send) -> Option<Grab> {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            log::info!("no global hotkeys under Wayland yet");
            return None;
        }
        let (conn, screen) = x11rb::connect(None).map_err(|e| log::warn!("no global hotkeys: {e}")).ok()?;
        let root = conn.setup().roots[screen].root;
        let keys = keycodes(&conn).map_err(|e| log::warn!("no global hotkeys: {e}")).ok()?;
        // Caps Lock and Num Lock change the modifiers a key comes with, so
        // each key is grabbed with and without them.
        let locks = [ModMask::from(0u16), ModMask::LOCK, ModMask::M2, ModMask::LOCK | ModMask::M2];
        for &(code, key) in &keys {
            let failed = locks.map(|lock| {
                let cookie = conn.grab_key(false, root, ModMask::CONTROL | ModMask::M1 | lock, code, GrabMode::ASYNC, GrabMode::ASYNC);
                cookie.map_or(true, |c| c.check().is_err())
            });
            let taken = failed.contains(&true);
            if taken {
                log::info!("Ctrl+Alt+{key:?} belongs to another program");
            }
        }
        let wake = conn.generate_id().ok()?;
        conn.create_window(COPY_DEPTH_FROM_PARENT, wake, root, 0, 0, 1, 1, 0, WindowClass::INPUT_ONLY, 0, &CreateWindowAux::new()).ok()?;
        conn.flush().ok()?;
        let conn = Arc::new(conn);
        let listener = conn.clone();
        let thread = std::thread::Builder::new()
            .name("popamp-hotkeys".into())
            .spawn(move || listen(&listener, wake, &keys, &send))
            .map_err(|e| log::warn!("no global hotkeys: {e}"))
            .ok()?;
        Some(Grab { conn, wake, thread: Some(thread) })
    }

    /// The keycode of each key this keyboard has.
    fn keycodes(conn: &RustConnection) -> Result<Vec<(Keycode, Key)>, x11rb::errors::ReplyError> {
        let (min, max) = (conn.setup().min_keycode, conn.setup().max_keycode);
        let map = conn.get_keyboard_mapping(min, max - min + 1)?.reply()?;
        Ok(BINDINGS
            .iter()
            .filter_map(|&(key, _)| Some((keycode(&map.keysyms, map.keysyms_per_keycode, min, key.keysym())?, key)))
            .collect())
    }

    /// The first keycode whose unshifted keysym is `sym`.
    pub(super) fn keycode(keysyms: &[Keysym], per: u8, min: Keycode, sym: Keysym) -> Option<Keycode> {
        let n = keysyms.chunks(per.max(1).into()).position(|syms| syms.first() == Some(&sym))?;
        min.checked_add(u8::try_from(n).ok()?)
    }

    fn listen(conn: &RustConnection, wake: Window, keys: &[(Keycode, Key)], send: &Send) {
        loop {
            match conn.wait_for_event() {
                Ok(Event::KeyPress(e)) => {
                    let key = keys.iter().find(|(code, _)| *code == e.detail).map(|&(_, key)| key);
                    if let Some(&(_, command)) = BINDINGS.iter().find(|(k, _)| Some(*k) == key) {
                        send(command);
                    }
                }
                Ok(Event::ClientMessage(m)) if m.window == wake => return,
                Ok(_) => {}
                Err(e) => {
                    log::warn!("global hotkeys: {e}");
                    return;
                }
            }
        }
    }

    impl Drop for Grab {
        /// Wakes the thread to end; the grabs end with the connection.
        fn drop(&mut self) {
            let msg = ClientMessageEvent::new(32, self.wake, x11rb::NONE, [0u32; 5]);
            let woken = self.conn.send_event(false, self.wake, EventMask::NO_EVENT, msg).is_ok() && self.conn.flush().is_ok();
            if let Some(thread) = self.thread.take().filter(|_| woken) {
                let _ = thread.join();
            }
        }
    }
}

/// RegisterHotKey on a thread of its own: the keys come to its message
/// queue, which it reads until told to quit.
#[cfg(windows)]
mod os {
    use super::{Key, Send, BINDINGS};
    use std::thread::JoinHandle;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, VK_DOWN, VK_END, VK_HOME, VK_INSERT, VK_LEFT, VK_NEXT, VK_PRIOR, VK_RIGHT,
        VK_UP,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetMessageW, PeekMessageW, PostThreadMessageW, MSG, PM_NOREMOVE, WM_HOTKEY, WM_QUIT, WM_USER,
    };

    pub struct Grab {
        thread_id: u32,
        thread: Option<JoinHandle<()>>,
    }

    impl Key {
        fn vk(self) -> u16 {
            match self {
                Key::Insert => VK_INSERT,
                Key::Home => VK_HOME,
                Key::End => VK_END,
                Key::PageUp => VK_PRIOR,
                Key::PageDown => VK_NEXT,
                Key::Up => VK_UP,
                Key::Down => VK_DOWN,
                Key::Left => VK_LEFT,
                Key::Right => VK_RIGHT,
            }
        }
    }

    pub fn grab(send: Send) -> Option<Grab> {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let thread = std::thread::Builder::new()
            .name("popamp-hotkeys".into())
            .spawn(move || {
                let mut msg: MSG = unsafe { std::mem::zeroed() };
                // SAFETY: `msg` is ours; peeking makes the thread's queue,
                // so that the quit message can be posted to it.
                unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), WM_USER, WM_USER, PM_NOREMOVE) };
                for (id, &(key, _)) in BINDINGS.iter().enumerate() {
                    // SAFETY: a thread hotkey (no window), freed below.
                    if unsafe { RegisterHotKey(std::ptr::null_mut(), id as i32, MOD_CONTROL | MOD_ALT, key.vk().into()) } == 0 {
                        log::info!("Ctrl+Alt+{key:?} belongs to another program");
                    }
                }
                // SAFETY: no arguments.
                let _ = tx.send(unsafe { GetCurrentThreadId() });
                // SAFETY: `msg` is ours; 0 is WM_QUIT, -1 an error.
                while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
                    if msg.message == WM_HOTKEY {
                        if let Some(&(_, command)) = BINDINGS.get(msg.wParam) {
                            send(command);
                        }
                    }
                }
                for id in 0..BINDINGS.len() {
                    // SAFETY: frees what this thread registered.
                    unsafe { UnregisterHotKey(std::ptr::null_mut(), id as i32) };
                }
            })
            .map_err(|e| log::warn!("no global hotkeys: {e}"))
            .ok()?;
        let thread_id = rx.recv().ok()?;
        Some(Grab { thread_id, thread: Some(thread) })
    }

    impl Drop for Grab {
        fn drop(&mut self) {
            // SAFETY: posts to the queue the thread made before it said its id.
            if unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) } != 0 {
                if let Some(thread) = self.thread.take() {
                    let _ = thread.join();
                }
            }
        }
    }

    #[cfg(test)]
    pub(super) fn press(grab: &Grab, n: usize) {
        // SAFETY: as in drop; a hotkey's message carries its id.
        unsafe { PostThreadMessageW(grab.thread_id, WM_HOTKEY, n, 0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn a_key_is_found_on_the_keyboard_by_its_unshifted_symbol() {
        // Two keysyms per keycode, keycodes from 8: Home on 9, Insert on 10
        // only shifted (as on a keypad).
        let map = [0x61, 0x41, 0xff50, 0, 0xff9e, 0xff63];
        assert_eq!(os::keycode(&map, 2, 8, 0xff50), Some(9));
        assert_eq!(os::keycode(&map, 2, 8, 0xff63), None);
        assert_eq!(os::keycode(&[], 0, 8, 0xff50), None);
    }

    #[test]
    #[cfg(windows)]
    fn a_hotkey_sends_its_command_until_let_go() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut keys = Hotkeys::new(Arc::new(move |c| tx.send(c).unwrap()));
        keys.want(true);
        let grab = keys.grab.as_ref().unwrap().as_ref().expect("a hotkey thread");
        os::press(grab, 2);
        os::press(grab, 4);
        let timeout = std::time::Duration::from_secs(5);
        assert_eq!([rx.recv_timeout(timeout).unwrap(), rx.recv_timeout(timeout).unwrap()], [Command::Stop, Command::Next]);
        keys.want(false);
        assert!(keys.grab.is_none(), "the thread has ended");
    }
}
