//! Stops the screen saver and the monitor's sleep for as long as something
//! is being watched. On a thread of its own: the desktop's bus can be slow
//! or gone, and neither the player nor the drawing should wait for it.

use std::time::Duration;

const CHECK: Duration = Duration::from_secs(1);

/// Runs until `stopped`, holding the screen on while `wanted`.
pub fn spawn(wanted: impl Fn() -> bool + Send + 'static, stopped: impl Fn() -> bool + Send + 'static) {
    let keep = move || {
        let mut held = None;
        while !stopped() {
            crate::hold::update(&mut held, wanted(), os::hold);
            std::thread::sleep(CHECK);
        }
    };
    if let Err(e) = std::thread::Builder::new().name("popamp-keep-awake".into()).spawn(keep) {
        log::warn!("cannot keep the screen on: {e}");
    }
}

/// The freedesktop call is served by GNOME, KDE, Xfce, MATE and Cinnamon.
/// A desktop without it just blanks as usual. Should the program die, the
/// bus ends the hold along with the connection.
#[cfg(linux_like)]
mod os {
    use zbus::blocking::Connection;

    const NAME: &str = "org.freedesktop.ScreenSaver";
    const PATH: &str = "/org/freedesktop/ScreenSaver";

    pub struct Hold {
        conn: Connection,
        cookie: u32,
    }

    pub fn hold() -> Option<Hold> {
        inhibit().map_err(|e| log::warn!("cannot keep the screen on: {e}")).ok()
    }

    fn inhibit() -> zbus::Result<Hold> {
        let conn = Connection::session()?;
        let cookie =
            conn.call_method(Some(NAME), PATH, Some(NAME), "Inhibit", &("Popamp", "Visualizer in full screen"))?.body().deserialize()?;
        Ok(Hold { conn, cookie })
    }

    impl Drop for Hold {
        fn drop(&mut self) {
            if let Err(e) = self.conn.call_method(Some(NAME), PATH, Some(NAME), "UnInhibit", &self.cookie) {
                log::warn!("cannot let the screen sleep again: {e}");
            }
        }
    }
}

/// Not there yet on macOS: the screen sleeps as usual.
#[cfg(target_os = "macos")]
mod os {
    pub struct Hold;

    pub fn hold() -> Option<Hold> {
        log::warn!("cannot keep the screen on: not on macOS yet");
        None
    }
}

/// The request belongs to the calling thread, which is why it is made and
/// ended on the same one, and it ends with the thread.
#[cfg(windows)]
mod os {
    use windows_sys::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED};

    pub struct Hold;

    pub fn hold() -> Option<Hold> {
        // SAFETY: takes flags only and touches nothing of ours.
        if unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED) } == 0 {
            log::warn!("cannot keep the screen on");
            return None;
        }
        Some(Hold)
    }

    impl Drop for Hold {
        fn drop(&mut self) {
            // SAFETY: as above.
            unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
        }
    }
}
