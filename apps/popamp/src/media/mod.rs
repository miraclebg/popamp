//! The desktop's media controls: media keys, desktop widgets, playerctl.
//! The UI thread sends [`Snapshot`]s through [`Controls`]; each system's
//! service turns them into its own notifications and sends method calls
//! back as remote commands. MPRIS2 over D-Bus on Unix; none on Windows yet.

use crate::model::AppEvent;
use popamp_audio::Shared;
use std::sync::Arc;
use winit::event_loop::EventLoopProxy;

#[cfg(unix)]
mod mpris;
#[cfg(unix)]
use mpris as service;

#[cfg(not(unix))]
mod service {
    use super::{AppEvent, Arc, EventLoopProxy, Shared, Snapshot};

    pub fn spawn(_: EventLoopProxy<AppEvent>, _: Arc<Shared>, _: &str) -> Option<crossbeam_channel::Sender<Snapshot>> {
        log::info!("no media controls on this system");
        None
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Track {
    pub id: u64,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub url: String,
    pub length_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub status: &'static str,
    pub repeat: bool,
    pub shuffle: bool,
    pub volume: f64,
    pub track: Option<Track>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self { status: "Stopped", repeat: false, shuffle: false, volume: 1.0, track: None }
    }
}

pub struct Controls {
    tx: crossbeam_channel::Sender<Snapshot>,
    last: Option<Snapshot>,
}

impl Controls {
    /// Publishes the snapshot if it changed.
    pub fn update(&mut self, snap: Snapshot) {
        if self.last.as_ref() != Some(&snap) {
            self.last = Some(snap.clone());
            let _ = self.tx.send(snap);
        }
    }
}

/// Starts the system's service; `None` if there is none or it is not
/// running.
pub fn spawn(proxy: EventLoopProxy<AppEvent>, shared: Arc<Shared>, instance: &str) -> Option<Controls> {
    Some(Controls { tx: service::spawn(proxy, shared, instance)?, last: None })
}
