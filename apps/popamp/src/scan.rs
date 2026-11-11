//! Scanning the selected tracks for ReplayGain. The values go to
//! `replaygain.txt`, never into the files, and play from then on.

use crate::model::{AppEvent, Model};
use popamp_audio::replaygain::{self, Outcome};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How a scan running on its own thread reports.
#[derive(Debug)]
pub enum ScanEvent {
    Progress {
        done: usize,
        total: usize,
    },
    /// Each location with what its scan gave.
    Done(Vec<(String, Outcome)>),
}

impl Model {
    /// Scans the selected local tracks.
    pub fn scan_replaygain(&mut self) {
        if self.scanning {
            return self.flash("ReplayGain: a scan is running");
        }
        let locations: Vec<String> =
            self.playlist.entries().iter().filter(|e| e.selected && !e.is_url()).map(|e| e.location.clone()).collect();
        if locations.is_empty() {
            return self.flash("ReplayGain: select the tracks to scan");
        }
        let (events, sources, cx, total) = (self.events.clone(), self.sources.clone(), self.source_context(), locations.len());
        let spawned = std::thread::Builder::new().name("popamp-rgscan".into()).spawn(move || {
            let outcomes = replaygain::scan(&locations, &sources, &cx, &|done| {
                events.send(AppEvent::Scan(ScanEvent::Progress { done, total }));
            });
            events.send(AppEvent::Scan(ScanEvent::Done(locations.into_iter().zip(outcomes).collect())));
        });
        match spawned {
            Ok(_) => {
                self.scanning = true;
                self.show_scan(format!("ReplayGain: scanning 0/{total}"));
            }
            Err(e) => log::warn!("cannot start a ReplayGain scan: {e}"),
        }
    }

    pub fn on_scan(&mut self, ev: ScanEvent) {
        match ev {
            ScanEvent::Progress { done, total } => self.show_scan(format!("ReplayGain: scanning {done}/{total}")),
            ScanEvent::Done(results) => self.scan_done(results),
        }
    }

    /// Keeps what was measured and plays it from the next track on.
    pub(crate) fn scan_done(&mut self, results: Vec<(String, Outcome)>) {
        self.scanning = false;
        let (total, mut failed) = (results.len(), 0);
        let store = Arc::make_mut(&mut self.scanned);
        for (location, outcome) in results {
            match outcome {
                Ok((stamp, gains)) => store.insert(location, stamp, gains),
                Err(e) => {
                    log::warn!("ReplayGain scan of {location}: {e}");
                    failed += 1;
                }
            }
        }
        if let Err(e) = self.scanned.save(&self.paths.replaygain_file()) {
            log::warn!("cannot save ReplayGain values: {e}");
        }
        self.push_engine_settings();
        self.show_scan(scan_summary(total, failed, self.settings().replaygain));
    }

    fn show_scan(&mut self, text: String) {
        self.transient = Some((text, Instant::now() + Duration::from_secs(4)));
    }
}

/// What the marquee says when a scan ends.
fn scan_summary(total: usize, failed: usize, on: bool) -> String {
    let mut s = format!("ReplayGain: {} of {total} scanned", total - failed);
    if !on {
        s.push_str(", turn it on in Preferences");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::tests::songs;
    use popamp_core::replaygain::{Gains, Level, Scanned, Stamp};

    fn message(m: &Model) -> String {
        m.transient.as_ref().map(|t| t.0.clone()).unwrap_or_default()
    }

    #[test]
    fn nothing_selected_is_nothing_to_scan() {
        let mut m = songs(&["Alpha"]);
        m.scan_replaygain();
        assert_eq!((m.scanning, message(&m).as_str()), (false, "ReplayGain: select the tracks to scan"));
    }

    #[test]
    fn one_scan_runs_at_a_time() {
        let mut m = songs(&["Alpha", "Beta"]);
        m.playlist.select_all(true);
        m.scan_replaygain();
        assert_eq!((m.scanning, message(&m).as_str()), (true, "ReplayGain: scanning 0/2"));
        m.scan_replaygain();
        assert_eq!(message(&m), "ReplayGain: a scan is running");
    }

    #[test]
    fn measured_values_are_kept_and_reach_playback() {
        let mut m = songs(&["Alpha", "Beta"]);
        m.scanning = true;
        let stamp = Stamp { size: 10, modified: 20 };
        let gains = Gains { track: Level { gain_db: -3.0, peak: 0.9 }, album: None };
        m.scan_done(vec![("/music/0.mp3".into(), Ok((stamp, gains))), ("/music/1.mp3".into(), Err("broken".into()))]);
        assert_eq!((m.scanning, message(&m).as_str()), (false, "ReplayGain: 1 of 2 scanned, turn it on in Preferences"));
        let saved = Scanned::load(&m.paths.replaygain_file());
        assert_eq!(saved.get("/music/0.mp3", stamp), Some(&gains));
        assert_eq!(m.source_context().scanned.get("/music/0.mp3", stamp), Some(&gains), "the engine is told");
        m.config.settings.replaygain = true;
        m.scan_done(vec![]);
        assert_eq!(message(&m), "ReplayGain: 0 of 0 scanned");
        assert_eq!(Scanned::load(&m.paths.replaygain_file()).len(), 1, "earlier scans stay");
    }
}
