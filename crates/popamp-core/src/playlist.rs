//! The playlist model: entries, selection, ordering and the
//! next/previous logic for shuffle, repeat and manual advance.

use crate::atf::Format;
use crate::meta::{TrackFields, TrackMeta};
use crate::text::natural_cmp;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct Entry {
    /// Stable identity, survives reordering.
    pub id: u64,
    /// A filesystem path or a URL.
    pub location: String,
    /// Title from the playlist file (`#EXTINF`, `TitleN=`) until tags are read.
    pub file_title: Option<String>,
    pub length_ms: Option<u64>,
    pub meta: Option<Arc<TrackMeta>>,
    /// Formatted display title (recomputed when meta or format changes).
    pub display: String,
    pub selected: bool,
}

impl Entry {
    pub fn new(location: impl Into<String>) -> Self {
        let location = location.into();
        let display = default_display(&location);
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            location,
            file_title: None,
            length_ms: None,
            meta: None,
            display,
            selected: false,
        }
    }

    pub fn is_url(&self) -> bool {
        is_url(&self.location)
    }

    pub fn refresh_display(&mut self, fmt: &Format) {
        self.display = match (&self.meta, &self.file_title) {
            (Some(meta), _) => {
                let s = fmt.eval(&TrackFields { location: &self.location, meta: Some(meta) });
                if s.trim().is_empty() {
                    default_display(&self.location)
                } else {
                    s
                }
            }
            (None, Some(t)) if !t.is_empty() => t.clone(),
            _ => default_display(&self.location),
        };
    }
}

pub fn is_url(s: &str) -> bool {
    let lower = s.get(..8).unwrap_or(s).to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn default_display(location: &str) -> String {
    if is_url(location) {
        return location.to_string();
    }
    let file = location.rsplit('/').next().unwrap_or(location);
    match file.rfind('.') {
        Some(dot) if dot > 0 => file[..dot].to_string(),
        _ => file.to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Title,
    FileName,
    Path,
}

#[derive(Debug, Default)]
pub struct Playlist {
    entries: Vec<Entry>,
    /// Index of the "current" (playing or to-be-played) entry.
    current: Option<usize>,
    /// Keyboard cursor / focus row.
    pub cursor: usize,
    shuffle: ShuffleState,
    /// Bumped on every structural change.
    version: u64,
}

#[derive(Debug, Default)]
struct ShuffleState {
    order: Vec<u64>,
    built_for: u64,
    /// Positions in `order` already played, most recent last.
    history: Vec<u64>,
}

impl Playlist {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    pub fn entry(&self, i: usize) -> Option<&Entry> {
        self.entries.get(i)
    }
    pub fn entry_mut(&mut self, i: usize) -> Option<&mut Entry> {
        self.entries.get_mut(i)
    }
    pub fn version(&self) -> u64 {
        self.version
    }
    pub fn current(&self) -> Option<usize> {
        self.current
    }
    pub fn current_entry(&self) -> Option<&Entry> {
        self.current.and_then(|i| self.entries.get(i))
    }
    pub fn index_of_id(&self, id: u64) -> Option<usize> {
        self.entries.iter().position(|e| e.id == id)
    }
    pub fn find_mut_by_id(&mut self, id: u64) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }

    pub fn set_current(&mut self, idx: Option<usize>) {
        self.current = idx.filter(|&i| i < self.entries.len());
        if let Some(i) = self.current {
            let id = self.entries[i].id;
            if self.shuffle.history.last() != Some(&id) {
                self.shuffle.history.push(id);
                if self.shuffle.history.len() > 10_000 {
                    self.shuffle.history.drain(..5_000);
                }
            }
        }
    }

    fn touch(&mut self) {
        self.version += 1;
        self.cursor = self.cursor.min(self.entries.len().saturating_sub(1));
    }

    pub fn push(&mut self, e: Entry) {
        self.entries.push(e);
        self.touch();
    }

    pub fn extend(&mut self, es: impl IntoIterator<Item = Entry>) {
        self.entries.extend(es);
        self.touch();
    }

    /// Inserts at `at` (clamped), shifting the current index.
    pub fn insert(&mut self, at: usize, es: Vec<Entry>) {
        let at = at.min(self.entries.len());
        let n = es.len();
        self.entries.splice(at..at, es);
        if let Some(c) = self.current.as_mut() {
            if *c >= at {
                *c += n;
            }
        }
        self.touch();
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.current = None;
        self.cursor = 0;
        self.shuffle = ShuffleState::default();
        self.touch();
    }

    fn retain_by(&mut self, keep: impl Fn(&Entry) -> bool) {
        let current_id = self.current_entry().map(|e| e.id);
        self.entries.retain(|e| keep(e));
        self.current = current_id.and_then(|id| self.index_of_id(id));
        self.touch();
    }

    pub fn remove_selected(&mut self) {
        self.retain_by(|e| !e.selected);
    }
    pub fn crop_selected(&mut self) {
        self.retain_by(|e| e.selected);
    }
    pub fn remove_missing(&mut self) {
        self.retain_by(|e| e.is_url() || std::path::Path::new(&e.location).exists());
    }
    pub fn remove_at(&mut self, idx: usize) {
        if idx < self.entries.len() {
            let id = self.entries[idx].id;
            self.retain_by(|e| e.id != id);
        }
    }

    // --- selection -------------------------------------------------------

    pub fn select_all(&mut self, on: bool) {
        self.entries.iter_mut().for_each(|e| e.selected = on);
    }
    pub fn invert_selection(&mut self) {
        self.entries.iter_mut().for_each(|e| e.selected = !e.selected);
    }
    pub fn select_only(&mut self, idx: usize) {
        for (i, e) in self.entries.iter_mut().enumerate() {
            e.selected = i == idx;
        }
        self.cursor = idx.min(self.entries.len().saturating_sub(1));
    }
    pub fn toggle_selected(&mut self, idx: usize) {
        if let Some(e) = self.entries.get_mut(idx) {
            e.selected = !e.selected;
        }
    }
    pub fn select_range(&mut self, a: usize, b: usize) {
        let (lo, hi) = (a.min(b), a.max(b));
        for (i, e) in self.entries.iter_mut().enumerate() {
            e.selected = i >= lo && i <= hi;
        }
    }
    pub fn selected_indices(&self) -> Vec<usize> {
        self.entries.iter().enumerate().filter(|(_, e)| e.selected).map(|(i, _)| i).collect()
    }
    pub fn selected_count(&self) -> usize {
        self.entries.iter().filter(|e| e.selected).count()
    }

    /// Moves the selected block by `delta` rows (Alt+Up/Down, drag).
    /// Returns how far the block actually moved.
    pub fn move_selected(&mut self, delta: isize) -> isize {
        let sel = self.selected_indices();
        let (Some(&first), Some(&last)) = (sel.first(), sel.last()) else { return 0 };
        let delta = delta.clamp(-(first as isize), (self.entries.len() - 1 - last) as isize);
        if delta == 0 {
            return 0;
        }
        let current_id = self.current_entry().map(|e| e.id);
        let (moving, mut rest): (Vec<Entry>, Vec<Entry>) = self.entries.drain(..).partition(|e| e.selected);
        // Each selected item keeps its offset relative to the others.
        let mut slots: Vec<Option<Entry>> = (0..rest.len() + moving.len()).map(|_| None).collect();
        for (k, e) in sel.iter().zip(moving) {
            slots[(*k as isize + delta) as usize] = Some(e);
        }
        let mut rest_iter = rest.drain(..);
        self.entries = slots.into_iter().map(|s| s.unwrap_or_else(|| rest_iter.next().expect("slot count matches"))).collect();
        self.current = current_id.and_then(|id| self.index_of_id(id));
        self.cursor = (self.cursor as isize + delta).clamp(0, self.entries.len() as isize - 1) as usize;
        self.touch();
        delta
    }

    // --- ordering ---------------------------------------------------------

    fn reorder(&mut self, f: impl FnOnce(&mut Vec<Entry>)) {
        let current_id = self.current_entry().map(|e| e.id);
        f(&mut self.entries);
        self.current = current_id.and_then(|id| self.index_of_id(id));
        self.touch();
    }

    pub fn sort(&mut self, key: SortKey) {
        self.reorder(|v| match key {
            SortKey::Title => v.sort_by(|a, b| natural_cmp(&a.display, &b.display)),
            SortKey::FileName => {
                v.sort_by(|a, b| natural_cmp(a.location.rsplit('/').next().unwrap_or(""), b.location.rsplit('/').next().unwrap_or("")))
            }
            SortKey::Path => v.sort_by(|a, b| natural_cmp(&a.location, &b.location)),
        });
    }
    pub fn reverse(&mut self) {
        self.reorder(|v| v.reverse());
    }
    pub fn randomize(&mut self) {
        self.reorder(|v| fastrand::shuffle(v));
    }

    pub fn refresh_all_displays(&mut self, fmt: &Format) {
        self.entries.iter_mut().for_each(|e| e.refresh_display(fmt));
    }

    // --- navigation -------------------------------------------------------

    fn ensure_shuffle(&mut self) {
        if self.shuffle.built_for == self.version && self.shuffle.order.len() == self.entries.len() {
            return;
        }
        let mut order: Vec<u64> = self.entries.iter().map(|e| e.id).collect();
        fastrand::shuffle(&mut order);
        // Keep the current track first so a new cycle doesn't repeat it immediately.
        if let Some(cur) = self.current_entry().map(|e| e.id) {
            if let Some(p) = order.iter().position(|&id| id == cur) {
                order.swap(0, p);
            }
        }
        self.shuffle.order = order;
        self.shuffle.built_for = self.version;
    }

    /// The entry to play after the current one, or `None` to stop.
    pub fn next_index(&mut self, shuffle: bool, repeat: bool) -> Option<usize> {
        let n = self.entries.len();
        if n == 0 {
            return None;
        }
        if !shuffle {
            return match self.current {
                None => Some(0),
                Some(c) if c + 1 < n => Some(c + 1),
                Some(_) if repeat => Some(0),
                Some(_) => None,
            };
        }
        self.ensure_shuffle();
        let pos = self.current_entry().and_then(|e| self.shuffle.order.iter().position(|&id| id == e.id));
        let next_pos = match pos {
            None => 0,
            Some(p) if p + 1 < n => p + 1,
            Some(_) if repeat => {
                // New cycle: reshuffle, avoiding an immediate repeat.
                self.shuffle.built_for = 0;
                self.ensure_shuffle();
                if n > 1 {
                    1
                } else {
                    0
                }
            }
            Some(_) => return None,
        };
        let id = self.shuffle.order[next_pos];
        self.index_of_id(id)
    }

    /// The entry for "previous": walks shuffle history when shuffling.
    pub fn prev_index(&mut self, shuffle: bool, repeat: bool) -> Option<usize> {
        let n = self.entries.len();
        if n == 0 {
            return None;
        }
        if shuffle {
            while self.shuffle.history.len() > 1 {
                self.shuffle.history.pop();
                if let Some(&id) = self.shuffle.history.last() {
                    if let Some(i) = self.index_of_id(id) {
                        self.shuffle.history.pop(); // set_current re-pushes it
                        return Some(i);
                    }
                }
            }
        }
        match self.current {
            None => Some(0),
            Some(0) if repeat => Some(n - 1),
            Some(0) => Some(0),
            Some(c) => Some(c - 1),
        }
    }

    /// Total and selected lengths in ms; `unknown` is true if any length is missing.
    pub fn durations(&self) -> (u64, u64, bool) {
        let mut total = 0;
        let mut selected = 0;
        let mut unknown = false;
        for e in &self.entries {
            match e.length_ms {
                Some(l) => {
                    total += l;
                    if e.selected {
                        selected += l;
                    }
                }
                None => unknown = true,
            }
        }
        (selected, total, unknown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(n: usize) -> Playlist {
        let mut pl = Playlist::new();
        pl.extend((0..n).map(|i| Entry::new(format!("/m/{i:02}.mp3"))));
        pl
    }

    fn names(pl: &Playlist) -> Vec<String> {
        pl.entries().iter().map(|e| e.display.clone()).collect()
    }

    #[test]
    fn linear_navigation() {
        let mut pl = list(3);
        assert_eq!(pl.next_index(false, false), Some(0));
        pl.set_current(Some(2));
        assert_eq!(pl.next_index(false, false), None);
        assert_eq!(pl.next_index(false, true), Some(0));
        pl.set_current(Some(0));
        assert_eq!(pl.prev_index(false, true), Some(2));
        assert_eq!(pl.prev_index(false, false), Some(0));
    }

    #[test]
    fn shuffle_plays_each_once_per_cycle() {
        let mut pl = list(20);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..20 {
            let i = pl.next_index(true, false).expect("still items left in the cycle");
            assert!(seen.insert(i), "repeated {i}");
            pl.set_current(Some(i));
        }
        assert_eq!(pl.next_index(true, false), None);
        assert!(pl.next_index(true, true).is_some());
    }

    #[test]
    fn shuffle_prev_walks_history() {
        let mut pl = list(10);
        let mut played = vec![];
        for _ in 0..4 {
            let i = pl.next_index(true, false).unwrap();
            pl.set_current(Some(i));
            played.push(i);
        }
        let p = pl.prev_index(true, false).unwrap();
        assert_eq!(p, played[2]);
        pl.set_current(Some(p));
        assert_eq!(pl.prev_index(true, false).unwrap(), played[1]);
    }

    #[test]
    fn remove_and_crop_keep_current() {
        let mut pl = list(5);
        pl.set_current(Some(3));
        pl.entry_mut(1).unwrap().selected = true;
        pl.remove_selected();
        assert_eq!(pl.len(), 4);
        assert_eq!(pl.current_entry().unwrap().display, "03");
        pl.entry_mut(0).unwrap().selected = true;
        pl.crop_selected();
        assert_eq!(names(&pl), ["00"]);
        assert_eq!(pl.current(), None);
    }

    #[test]
    fn move_selection_block() {
        let mut pl = list(5);
        pl.entry_mut(1).unwrap().selected = true;
        pl.entry_mut(2).unwrap().selected = true;
        assert_eq!(pl.move_selected(2), 2);
        assert_eq!(names(&pl), ["00", "03", "04", "01", "02"]);
        assert_eq!(pl.move_selected(5), 0);
        assert_eq!(pl.move_selected(-10), -3);
        assert_eq!(names(&pl), ["01", "02", "00", "03", "04"]);
    }

    #[test]
    fn sorting_and_insert() {
        let mut pl = Playlist::new();
        pl.extend(["/b/10.mp3", "/a/2.mp3", "/c/1.mp3"].map(Entry::new));
        pl.set_current(Some(0));
        pl.sort(SortKey::FileName);
        assert_eq!(names(&pl), ["1", "2", "10"]);
        assert_eq!(pl.current(), Some(2));
        pl.sort(SortKey::Path);
        assert_eq!(names(&pl), ["2", "10", "1"]);
        pl.insert(0, vec![Entry::new("/z.mp3")]);
        assert_eq!(pl.current(), Some(2));
        pl.reverse();
        assert_eq!(names(&pl)[0], "1");
    }

    #[test]
    fn display_uses_format() {
        let mut e = Entry::new("/x/song.ogg");
        e.meta = Some(Arc::new(TrackMeta { artist: Some("A".into()), title: Some("T".into()), ..Default::default() }));
        e.refresh_display(&Format::parse(crate::config::DEFAULT_TITLE_FORMAT));
        assert_eq!(e.display, "A - T");
        assert_eq!(Entry::new("http://x/stream").display, "http://x/stream");
    }
}
