//! The calls between Popamp and its plug-ins, shared by the player and by
//! plug-ins written in Rust so both sides are checked by the compiler.
//!
//! A plug-in is an [Extism](https://extism.org) module. Every call passes
//! JSON: the input is the JSON of the type named at each call, and so is
//! the output. A call fails by returning an Extism error; its message is
//! shown in the title display.
//!
//! A `source` plug-in exports:
//!
//! | Export | Input | Output |
//! |---|---|---|
//! | [`calls::API_VERSION`] | none | [`Version`] |
//! | [`calls::EXPAND`] | a location (`String`) | `Option<Vec<Track>>`; `None` when it is one track |
//! | [`calls::DESCRIBE`] | a location | `Option<Tags>`; `None` when it has none to give |
//! | [`calls::OPEN`] | a location | [`Play`] |
//!
//! Which locations a source takes is written in its manifest, so the
//! player can tell without calling it.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// The API version this crate describes.
pub const VERSION: Version = Version { major: 1, minor: 0 };

/// Names of the functions a plug-in exports.
pub mod calls {
    pub const API_VERSION: &str = "api_version";
    pub const EXPAND: &str = "expand";
    pub const DESCRIBE: &str = "describe";
    pub const OPEN: &str = "open";
}

/// Names of the functions the player gives plug-ins.
pub mod host {
    /// Runs a program named in the manifest's `exec`: [`super::ExecRequest`]
    /// in, [`super::ExecOutput`] out.
    pub const EXEC: &str = "popamp_exec";
}

/// Keys of the plug-in configuration the player sets, besides the
/// plug-in's own settings (keyed as in its manifest).
pub mod config {
    /// The user's HTTP proxy, `[user:pass@]host:port`; empty for none.
    pub const PROXY: &str = "popamp.proxy";
}

/// In an argument of [`Play::Pipe`], replaced by the start position in
/// seconds (`12.345`) each time the player starts the program.
pub const START: &str = "{start}";

/// `MAJOR.MINOR`. Minor versions only add calls or optional fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
}

impl Version {
    /// True when a player of this version loads a plug-in built for `plugin`.
    pub fn loads(self, plugin: Version) -> bool {
        self.major == plugin.major && plugin.minor <= self.minor
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

impl FromStr for Version {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let bad = || format!("bad API version {s:?}, expected MAJOR.MINOR");
        let (major, minor) = s.split_once('.').ok_or_else(bad)?;
        Ok(Version { major: major.parse().map_err(|_| bad())?, minor: minor.parse().map_err(|_| bad())? })
    }
}

impl Serialize for Version {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?.parse().map_err(serde::de::Error::custom)
    }
}

/// One track of a list.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Track {
    pub location: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length_ms: Option<u64>,
}

/// Tags of a track; every field may be missing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tags {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate_kbps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codec: Option<String>,
}

/// How the player plays a track; the plug-in never handles audio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
// One answer per track opened: its size does not matter, and a box would
// only make plug-ins harder to write.
#[allow(clippy::large_enum_variant)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Play {
    /// The player opens the URL like any HTTP stream.
    Http { url: String },
    /// The player starts `program` (one of the manifest's `exec`) and plays
    /// what it writes to standard output: interleaved stereo 32-bit float
    /// samples, little-endian, at `rate`. [`START`] in `args` is replaced
    /// by the position when the track opens and on every seek.
    Pipe {
        program: String,
        args: Vec<String>,
        rate: u32,
        seekable: bool,
        #[serde(default)]
        tags: Tags,
    },
}

/// Input of [`host::EXEC`]. Standard input is closed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecRequest {
    /// A name from the manifest's `exec`, not a path: the player finds it.
    pub program: String,
    pub args: Vec<String>,
}

/// Output of [`host::EXEC`]. Asking for a program the manifest does not
/// name fails the call itself, and counts as the plug-in failing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecOutput {
    /// Why the program did not run to its end: not found, or stopped
    /// because the call's time was up. The other fields are empty then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The exit code; `None` when the program was killed by a signal.
    pub status: Option<i32>,
    /// Standard output and error, as text (invalid UTF-8 is replaced).
    pub stdout: String,
    pub stderr: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_player_loads_plugins_of_its_major_version_and_no_newer_minor() {
        let v = |s: &str| s.parse::<Version>().unwrap();
        assert!(v("1.2").loads(v("1.0")));
        assert!(v("1.2").loads(v("1.2")));
        assert!(!v("1.2").loads(v("1.3")));
        assert!(!v("2.0").loads(v("1.0")));
        for bad in ["1", "1.x", "", "1.0.0"] {
            assert!(bad.parse::<Version>().is_err(), "{bad}");
        }
        assert_eq!(serde_json::to_string(&VERSION).unwrap(), r#""1.0""#);
    }

    #[test]
    fn play_answers_read_as_the_documented_json() {
        let pipe: Play =
            serde_json::from_str(r#"{"kind":"pipe","program":"ffmpeg","args":["-ss","{start}"],"rate":48000,"seekable":true}"#).unwrap();
        assert_eq!(
            pipe,
            Play::Pipe {
                program: "ffmpeg".into(),
                args: vec!["-ss".into(), START.into()],
                rate: 48000,
                seekable: true,
                tags: Tags::default()
            }
        );
        let http: Play = serde_json::from_str(r#"{"kind":"http","url":"http://a/b"}"#).unwrap();
        assert_eq!(http, Play::Http { url: "http://a/b".into() });
        assert!(serde_json::from_str::<Play>(r#"{"kind":"http","url":"u","extra":1}"#).is_err());
    }

    #[test]
    fn a_list_of_tracks_omits_what_is_unknown() {
        let t = Track { location: "x".into(), ..Default::default() };
        assert_eq!(serde_json::to_string(&Some(vec![t])).unwrap(), r#"[{"location":"x"}]"#);
        assert_eq!(serde_json::from_str::<Option<Vec<Track>>>("null").unwrap(), None);
    }
}
