//! Headless player: `cargo run -p popamp-audio --example play -- [--vol N] FILE...`
//! Plays the files gaplessly and prints position once a second.

use popamp_audio::{Engine, EngineCmd, EngineEvent};
use std::time::Duration;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut volume = 200u8;
    if args.first().map(String::as_str) == Some("--vol") && args.len() > 1 {
        volume = args[1].parse().unwrap_or(200);
        args.drain(..2);
    }
    let seconds: u64 = std::env::var("PLAY_SECONDS").ok().and_then(|s| s.parse().ok()).unwrap_or(u64::MAX);
    let (tx, rx) = crossbeam_channel::unbounded();
    let engine = Engine::spawn("default".into(), Default::default(), move |e| {
        let _ = tx.send(e);
    });
    engine.send(EngineCmd::Volume(volume));
    let files: Vec<(u64, String)> = args.into_iter().enumerate().map(|(i, f)| (i as u64, f)).collect();
    let Some((id, first)) = files.first().cloned() else {
        eprintln!("usage: play [--vol 0-255] FILE...");
        return;
    };
    engine.send(EngineCmd::Open { id, location: first, start_ms: 0 });
    let start = std::time::Instant::now();
    loop {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(EngineEvent::Started { id, info, meta, .. }) | Ok(EngineEvent::TrackChanged { id, info, meta }) => {
                println!("track {id}: {:?} - {:?} {info:?}", meta.artist, meta.title);
                engine.send(EngineCmd::SetNext(files.get(id as usize + 1).cloned()));
            }
            Ok(EngineEvent::Finished { .. }) | Ok(EngineEvent::Stopped) => break,
            Ok(other) => println!("{other:?}"),
            Err(_) => {}
        }
        let s = engine.shared();
        println!("pos {} ms / {:?} ms, {} kbps", s.position_ms(), s.length_ms(), s.bitrate_kbps());
        if start.elapsed().as_secs() >= seconds {
            break;
        }
    }
}
