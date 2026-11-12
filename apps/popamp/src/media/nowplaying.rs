//! macOS's Now Playing and media keys. Not there yet: no service starts.

use super::Snapshot;
use crate::model::AppEvent;
use popamp_audio::Shared;
use std::sync::Arc;
use winit::event_loop::EventLoopProxy;

/// Starts nothing yet; the player runs without system media controls.
pub fn spawn(_proxy: EventLoopProxy<AppEvent>, _shared: Arc<Shared>, _instance: &str) -> Option<crossbeam_channel::Sender<Snapshot>> {
    log::debug!("no media controls on macOS yet");
    None
}
