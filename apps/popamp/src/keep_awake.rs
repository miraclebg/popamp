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

/// An IOKit power assertion against the display sleeping; macOS ends it
/// with the process should Popamp die.
#[cfg(target_os = "macos")]
mod os {
    use objc2_core_foundation::CFString;

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOPMAssertionCreateWithName(kind: &CFString, level: u32, name: &CFString, id: *mut u32) -> i32;
        fn IOPMAssertionRelease(id: u32) -> i32;
    }

    /// kIOPMAssertionLevelOn.
    const ON: u32 = 255;

    pub struct Hold {
        id: u32,
    }

    pub fn hold() -> Option<Hold> {
        let (kind, name) = (CFString::from_str("PreventUserIdleDisplaySleep"), CFString::from_str("Popamp visualizer"));
        let mut id = 0;
        // SAFETY: two CFStrings and an out-pointer, as IOKit asks.
        let status = unsafe { IOPMAssertionCreateWithName(&kind, ON, &name, &mut id) };
        if status != 0 {
            log::warn!("cannot keep the screen on (IOKit error {status})");
            return None;
        }
        Some(Hold { id })
    }

    impl Drop for Hold {
        fn drop(&mut self) {
            // SAFETY: the assertion made in `hold`, released once.
            let status = unsafe { IOPMAssertionRelease(self.id) };
            if status != 0 {
                log::warn!("cannot let the screen sleep again (IOKit error {status})");
            }
        }
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

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn a_hold_keeps_the_screen_on_until_dropped() {
        let listed = || {
            let out = std::process::Command::new("pmset").args(["-g", "assertions"]).output().unwrap();
            String::from_utf8_lossy(&out.stdout).contains("Popamp visualizer")
        };
        let hold = os::hold().expect("a power assertion");
        assert!(listed(), "macOS lists the hold");
        drop(hold);
        assert!(!listed(), "and forgets it once dropped");
    }
}
