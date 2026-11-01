//! Command line: Winamp-style `/SWITCH` and GNU `--switch`.

use linamp_core::Command;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum Remote {
    /// Replace the playlist with these and play.
    Open(Vec<String>),
    /// Append to the playlist.
    Add(Vec<String>),
    Cmd(Command),
    SetVolumePercent(u8),
    VolumeStep(i32),
    SetPan(i32),
    PanStep(i32),
    Shuffle(Option<bool>),
    Repeat(Option<bool>),
    SeekMs(u64),
    Clear,
    Show,
    Quit,
    Status,
}

#[derive(Debug, Default, PartialEq)]
pub struct Options {
    pub new_instance: bool,
    pub instance_class: Option<String>,
    pub config_file: Option<PathBuf>,
    pub ini_dir: Option<PathBuf>,
    pub start_minimized: bool,
    pub kill: bool,
    pub safe: bool,
    pub del_m3u: bool,
    pub remotes: Vec<Remote>,
    /// Draw the windows into PNG files in this directory and exit.
    pub render: Option<PathBuf>,
    /// Skin for `render`.
    pub skin: Option<PathBuf>,
    pub help: bool,
    pub version: bool,
}

/// Normalizes `/FOO=bar`, `--foo=bar`, `-foo` to (`FOO`, Some("bar")).
fn split_switch(arg: &str) -> Option<(String, Option<String>)> {
    let body = if let Some(b) = arg.strip_prefix("--") {
        b
    } else {
        let b = arg.strip_prefix('/')?;
        // A real absolute path is not a switch; `/INIDIR=/x/y` is.
        let name = b.split('=').next().unwrap_or(b);
        if name.contains('/') || name.is_empty() || std::path::Path::new(arg).exists() {
            return None;
        }
        b
    };
    let (name, value) = match body.split_once('=') {
        Some((n, v)) => (n, Some(v.to_string())),
        None => (body, None),
    };
    (!name.is_empty()).then(|| (name.to_ascii_uppercase().replace('-', "_"), value))
}

fn toggle(v: Option<String>) -> Option<bool> {
    v.and_then(|s| match s.trim() {
        "1" | "on" | "true" => Some(true),
        "0" | "off" | "false" => Some(false),
        _ => None,
    })
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut o = Options::default();
    let mut files = Vec::new();
    let mut add_mode = false;
    for arg in args {
        let Some((name, value)) = split_switch(&arg) else {
            files.push(arg);
            continue;
        };
        let cmd = |c: Command| Some(Remote::Cmd(c));
        let remote = match name.as_str() {
            "NEW" => {
                o.new_instance = true;
                None
            }
            "ADD" | "ENQUEUE" => {
                add_mode = true;
                None
            }
            "CLASS" => {
                o.instance_class = value;
                None
            }
            "CONFIG" => {
                o.config_file = value.map(PathBuf::from);
                None
            }
            "INIDIR" => {
                o.ini_dir = value.map(PathBuf::from);
                None
            }
            "STARTMIN" => {
                o.start_minimized = true;
                None
            }
            "STARTMAX" => Some(Remote::Show),
            "KILL" => {
                o.kill = true;
                None
            }
            "SAFE" | "SAFEALWAYS" => {
                o.safe = true;
                None
            }
            "RENDER" => {
                o.render = Some(value.map(PathBuf::from).ok_or("/RENDER= needs a directory")?);
                None
            }
            "SKIN" => {
                o.skin = value.map(PathBuf::from);
                None
            }
            "DELM3U" => {
                o.del_m3u = true;
                None
            }
            "HELP" | "H" | "?" => {
                o.help = true;
                None
            }
            "VERSION" => {
                o.version = true;
                None
            }
            "CLOSE" | "QUIT" | "EXIT" => Some(Remote::Quit),
            "STATUS" => Some(Remote::Status),
            "PLAY" => cmd(Command::Play),
            "PAUSE" => cmd(Command::Pause),
            "PLAYPAUSE" => cmd(Command::PlayPause),
            "STOP" => cmd(Command::Stop),
            "STOPFADE" => cmd(Command::StopFadeout),
            "STOPAFTER" => cmd(Command::StopAfterCurrent),
            "PREV" => cmd(Command::Previous),
            "NEXT" => cmd(Command::Next),
            "REV" => cmd(Command::Back5s),
            "FWD" => cmd(Command::Fwd5s),
            "JUMPTO" => cmd(Command::JumpToFile),
            "CLEAR" => Some(Remote::Clear),
            "RANDOM" | "SHUFFLE" => Some(Remote::Shuffle(toggle(value))),
            "REPEAT" => Some(Remote::Repeat(toggle(value))),
            "VOLUP" => Some(Remote::VolumeStep(5)),
            "VOLDOWN" => Some(Remote::VolumeStep(-5)),
            "VOL" => {
                let v: u8 = value.as_deref().and_then(|v| v.trim().parse().ok()).ok_or("/VOL= needs 0-100")?;
                Some(Remote::SetVolumePercent(v.min(100)))
            }
            "PANLEFT" => Some(Remote::PanStep(-6)),
            "PANRIGHT" => Some(Remote::PanStep(6)),
            "PAN" => {
                let v: i32 = value.as_deref().and_then(|v| v.trim().parse().ok()).ok_or("/PAN= needs -127..127")?;
                Some(Remote::SetPan(v.clamp(-127, 127)))
            }
            "COMMAND" => {
                let id: u32 = value.as_deref().and_then(|v| v.trim().parse().ok()).ok_or("/COMMAND= needs a number")?;
                Some(Remote::Cmd(Command::from_id(id).ok_or(format!("unknown command id {id}"))?))
            }
            other => return Err(format!("unknown option /{other} (try --help)")),
        };
        if let Some(r) = remote {
            o.remotes.push(r);
        }
    }
    if !files.is_empty() {
        let files = files.into_iter().map(absolutize).collect();
        o.remotes.insert(0, if add_mode { Remote::Add(files) } else { Remote::Open(files) });
    }
    Ok(o)
}

fn absolutize(f: String) -> String {
    if linamp_core::playlist::is_url(&f) {
        return f;
    }
    let p = PathBuf::from(&f);
    if p.is_absolute() {
        f
    } else {
        std::env::current_dir().map(|d| d.join(p).to_string_lossy().into_owned()).unwrap_or(f)
    }
}

pub const HELP: &str = "\
Linamp - a Winamp-style audio player for Linux

Usage: linamp [OPTIONS] [FILES/URLS...]

Files are played (replacing the playlist) unless /ADD is given.
Options accept /SWITCH or --switch spelling:
  /NEW                 start a separate instance
  /ADD                 enqueue the files instead of playing them
  /CLASS=<name>        instance name (separate control socket)
  /INIDIR=<dir>        settings directory
  /CONFIG=<file>       settings file
  /PLAY /PAUSE /PLAYPAUSE /STOP /STOPFADE /STOPAFTER /PREV /NEXT /REV /FWD
  /RANDOM[=0|1] /SHUFFLE[=0|1] /REPEAT[=0|1]
  /VOLUP /VOLDOWN /VOL=<0-100> /PANLEFT /PANRIGHT /PAN=<-127..127>
  /JUMPTO /CLEAR /STARTMIN /STARTMAX /DELM3U
  /COMMAND=<id>        run a Winamp command id (e.g. 40045 = Play)
  /RENDER=<dir>        draw the windows into PNG files (no display needed)
                       and exit; FILES fill the playlist
  /SKIN=<file>         skin for /RENDER (default: the built-in skin)
  /CLOSE /QUIT /EXIT   quit the running instance
  /KILL                kill the running instance
  /STATUS              print the running instance's status as JSON
  /HELP /VERSION
";

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Options {
        parse(args.iter().map(|s| s.to_string())).unwrap()
    }

    #[test]
    fn switches_both_styles() {
        let o = p(&["/PLAY", "--next", "/VOL=50", "/RANDOM=1", "/repeat", "--pan=-200"]);
        assert_eq!(
            o.remotes,
            [
                Remote::Cmd(Command::Play),
                Remote::Cmd(Command::Next),
                Remote::SetVolumePercent(50),
                Remote::Shuffle(Some(true)),
                Remote::Repeat(None),
                Remote::SetPan(-127)
            ]
        );
    }

    #[test]
    fn files_open_or_add() {
        let o = p(&["/tmp/a.mp3", "http://x/y"]);
        assert_eq!(o.remotes, [Remote::Open(vec!["/tmp/a.mp3".into(), "http://x/y".into()])]);
        let o = p(&["/ADD", "/tmp/a.mp3"]);
        assert!(matches!(&o.remotes[0], Remote::Add(v) if v.len() == 1));
    }

    #[test]
    fn instance_options() {
        let o = p(&["/NEW", "/CLASS=foo", "/INIDIR=/tmp/x", "/COMMAND=40047", "/home/nope/a.mp3"]);
        assert!(o.new_instance);
        assert_eq!(o.instance_class.as_deref(), Some("foo"));
        assert_eq!(o.ini_dir, Some(PathBuf::from("/tmp/x")));
        assert_eq!(o.remotes, [Remote::Open(vec!["/home/nope/a.mp3".into()]), Remote::Cmd(Command::Stop)]);
    }

    #[test]
    fn render_options() {
        let o = p(&["--render=/tmp/out", "/SKIN=/tmp/Base.wsz", "/tmp/a.mp3"]);
        assert_eq!(o.render, Some(PathBuf::from("/tmp/out")));
        assert_eq!(o.skin, Some(PathBuf::from("/tmp/Base.wsz")));
        assert_eq!(o.remotes, [Remote::Open(vec!["/tmp/a.mp3".into()])]);
    }

    #[test]
    fn errors() {
        assert!(parse(["/BOGUS".to_string()]).is_err());
        assert!(parse(["/COMMAND=1".to_string()]).is_err());
        assert!(parse(["/VOL=abc".to_string()]).is_err());
        assert!(parse(["/RENDER".to_string()]).is_err());
    }
}
