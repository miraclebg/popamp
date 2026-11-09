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
            update(&mut held, wanted(), os::hold);
            std::thread::sleep(CHECK);
        }
    };
    if let Err(e) = std::thread::Builder::new().name("popamp-keep-awake".into()).spawn(keep) {
        log::warn!("cannot keep the screen on: {e}");
    }
}

/// `held` is `None` while not wanted. `Some(None)` is a hold that failed:
/// it is not tried again every second, nor logged every second.
fn update<H>(held: &mut Option<Option<H>>, wanted: bool, hold: impl FnOnce() -> Option<H>) {
    if wanted != held.is_some() {
        *held = wanted.then(hold);
    }
}

/// The freedesktop call is served by GNOME, KDE, Xfce, MATE and Cinnamon.
/// A desktop without it just blanks as usual. Should the program die, the
/// bus ends the hold along with the connection.
#[cfg(unix)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    /// Counts the holds still open.
    struct Open(Rc<Cell<i32>>);

    impl Drop for Open {
        fn drop(&mut self) {
            self.0.set(self.0.get() - 1);
        }
    }

    #[test]
    fn the_screen_is_held_once_while_wanted_and_let_go_after() {
        let (open, made) = (Rc::new(Cell::new(0)), Cell::new(0));
        let hold = || {
            made.set(made.get() + 1);
            open.set(open.get() + 1);
            Some(Open(open.clone()))
        };
        let mut held = None;
        update(&mut held, false, hold);
        assert_eq!((open.get(), made.get()), (0, 0));
        for _ in 0..3 {
            update(&mut held, true, hold);
        }
        assert_eq!((open.get(), made.get()), (1, 1));
        update(&mut held, false, hold);
        assert_eq!((open.get(), made.get()), (0, 1));
        update(&mut held, true, hold);
        assert_eq!((open.get(), made.get()), (1, 2));
    }

    #[test]
    fn a_hold_that_failed_is_tried_again_only_the_next_time_it_is_wanted() {
        let tries = Cell::new(0);
        let hold = || {
            tries.set(tries.get() + 1);
            None::<()>
        };
        let mut held = None;
        update(&mut held, true, hold);
        update(&mut held, true, hold);
        assert_eq!(tries.get(), 1);
        update(&mut held, false, hold);
        update(&mut held, true, hold);
        assert_eq!(tries.get(), 2);
    }
}
