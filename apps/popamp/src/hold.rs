//! Something held while wanted: the screen kept on, the global hotkeys.

/// Takes or lets go of a hold to match `wanted`. `held` is `None` while
/// not wanted. `Some(None)` is a hold that failed: it is not tried again
/// every second, nor logged every second.
pub fn update<H>(held: &mut Option<Option<H>>, wanted: bool, hold: impl FnOnce() -> Option<H>) {
    if wanted != held.is_some() {
        *held = wanted.then(hold);
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
