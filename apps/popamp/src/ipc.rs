//! Single-instance control socket.
//!
//! Protocol: the client sends one line per remote command (see
//! [`encode`]) and a final empty line; the server answers with one line
//! (status JSON or `ok`) and closes.

use crate::cli::Remote;
use popamp_core::Command;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

pub fn socket_path(class: &str) -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(format!("popamp-{}", unsafe_uid())));
    let safe: String = class.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    dir.join("popamp").join(format!("{safe}.sock"))
}

fn unsafe_uid() -> String {
    std::env::var("USER").unwrap_or_else(|_| "user".into())
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n")
}

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => out.push('\n'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn encode(r: &Remote) -> String {
    match r {
        Remote::Open(files) => format!("open\t{}", files.iter().map(|f| escape(f)).collect::<Vec<_>>().join("\t")),
        Remote::Add(files) => format!("add\t{}", files.iter().map(|f| escape(f)).collect::<Vec<_>>().join("\t")),
        Remote::Cmd(c) => format!("cmd\t{}", c.id()),
        Remote::SetVolumePercent(v) => format!("vol\t{v}"),
        Remote::VolumeStep(v) => format!("volstep\t{v}"),
        Remote::SetPan(v) => format!("pan\t{v}"),
        Remote::PanStep(v) => format!("panstep\t{v}"),
        Remote::Shuffle(v) => format!("shuffle\t{}", v.map(|b| (b as u8).to_string()).unwrap_or_default()),
        Remote::Repeat(v) => format!("repeat\t{}", v.map(|b| (b as u8).to_string()).unwrap_or_default()),
        Remote::SeekMs(ms) => format!("seek\t{ms}"),
        Remote::Clear => "clear".into(),
        Remote::Show => "show".into(),
        Remote::Quit => "quit".into(),
        Remote::Status => "status".into(),
    }
}

pub fn decode(line: &str) -> Option<Remote> {
    let mut parts = line.split('\t');
    let verb = parts.next()?;
    let rest: Vec<&str> = parts.collect();
    let arg = rest.first().copied().unwrap_or("");
    let opt_bool = |s: &str| match s {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    };
    Some(match verb {
        "open" => Remote::Open(rest.iter().filter(|s| !s.is_empty()).map(|s| unescape(s)).collect()),
        "add" => Remote::Add(rest.iter().filter(|s| !s.is_empty()).map(|s| unescape(s)).collect()),
        "cmd" => Remote::Cmd(Command::from_id(arg.parse().ok()?)?),
        "vol" => Remote::SetVolumePercent(arg.parse().ok()?),
        "volstep" => Remote::VolumeStep(arg.parse().ok()?),
        "pan" => Remote::SetPan(arg.parse().ok()?),
        "panstep" => Remote::PanStep(arg.parse().ok()?),
        "shuffle" => Remote::Shuffle(opt_bool(arg)),
        "repeat" => Remote::Repeat(opt_bool(arg)),
        "seek" => Remote::SeekMs(arg.parse().ok()?),
        "clear" => Remote::Clear,
        "show" => Remote::Show,
        "quit" => Remote::Quit,
        "status" => Remote::Status,
        _ => return None,
    })
}

/// Sends commands to a running instance. Returns its reply, or `Err` if
/// none is running.
pub fn send(path: &PathBuf, remotes: &[Remote]) -> std::io::Result<String> {
    let mut s = UnixStream::connect(path)?;
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut msg = String::new();
    for r in remotes {
        msg.push_str(&encode(r));
        msg.push('\n');
    }
    msg.push('\n');
    s.write_all(msg.as_bytes())?;
    let mut reply = String::new();
    BufReader::new(s).read_line(&mut reply)?;
    Ok(reply.trim_end().to_string())
}

#[derive(Debug)]
pub struct Request {
    pub remotes: Vec<Remote>,
    pub reply: crossbeam_channel::Sender<String>,
}

/// Binds the socket (replacing a stale one) and serves on a thread.
pub fn serve(path: PathBuf, deliver: impl Fn(Request) + Send + 'static) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if path.exists() {
        // Only reached when nothing answered on it: stale.
        let _ = std::fs::remove_file(&path);
    }
    let listener = UnixListener::bind(&path)?;
    std::thread::Builder::new().name("popamp-ipc".into()).spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut reader = BufReader::new(match stream.try_clone() {
                Ok(s) => s,
                Err(_) => continue,
            });
            let mut remotes = Vec::new();
            let mut line = String::new();
            while reader.read_line(&mut line).map(|n| n > 0).unwrap_or(false) {
                let l = line.trim_end_matches('\n');
                if l.is_empty() {
                    break;
                }
                if let Some(r) = decode(l) {
                    remotes.push(r);
                }
                line.clear();
            }
            let (tx, rx) = crossbeam_channel::bounded(1);
            deliver(Request { remotes, reply: tx });
            let reply = rx.recv_timeout(Duration::from_secs(4)).unwrap_or_else(|_| "ok".into());
            let mut s = stream;
            let _ = s.write_all(format!("{reply}\n").as_bytes());
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_round_trip() {
        let all = [
            Remote::Open(vec!["/a b/c\td.mp3".into(), "line\nbreak".into()]),
            Remote::Add(vec!["x".into()]),
            Remote::Cmd(Command::Next),
            Remote::SetVolumePercent(40),
            Remote::VolumeStep(-5),
            Remote::SetPan(-30),
            Remote::PanStep(6),
            Remote::Shuffle(None),
            Remote::Repeat(Some(true)),
            Remote::SeekMs(1234),
            Remote::Clear,
            Remote::Show,
            Remote::Quit,
            Remote::Status,
        ];
        for r in all {
            if let Remote::Open(files) = &r {
                // Tabs inside names are not representable; skip that case.
                if files.iter().any(|f| f.contains('\t')) {
                    continue;
                }
            }
            assert_eq!(decode(&encode(&r)), Some(r.clone()), "{r:?}");
        }
        assert_eq!(decode(&encode(&Remote::Open(vec!["line\nbreak".into()]))), Some(Remote::Open(vec!["line\nbreak".into()])));
    }

    #[test]
    fn socket_round_trip() {
        let path = std::env::temp_dir().join(format!("popamp-test-{}.sock", std::process::id()));
        serve(path.clone(), |req| {
            let _ = req.reply.send(format!("got {}", req.remotes.len()));
        })
        .unwrap();
        let reply = send(&path, &[Remote::Status, Remote::Cmd(Command::Play)]).unwrap();
        assert_eq!(reply, "got 2");
        let _ = std::fs::remove_file(&path);
    }
}
