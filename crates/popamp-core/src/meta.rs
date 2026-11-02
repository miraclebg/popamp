//! Track metadata as shown in the playlist and fed to title formatting.

use crate::atf::Fields;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackMeta {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<String>,
    pub genre: Option<String>,
    pub track: Option<String>,
    pub disc: Option<String>,
    pub comment: Option<String>,
    pub composer: Option<String>,
    pub length_ms: Option<u64>,
    pub bitrate_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub codec: Option<String>,
    pub replaygain_track_gain: Option<f32>,
    pub replaygain_album_gain: Option<f32>,
    pub replaygain_track_peak: Option<f32>,
    pub replaygain_album_peak: Option<f32>,
}

/// Field provider combining a location with its (optional) metadata.
pub struct TrackFields<'a> {
    pub location: &'a str,
    pub meta: Option<&'a TrackMeta>,
}

impl Fields for TrackFields<'_> {
    fn field(&self, name: &str) -> Option<String> {
        let m = self.meta;
        let get = |f: fn(&TrackMeta) -> &Option<String>| m.and_then(|m| f(m).clone());
        match name {
            "filename" => Some(self.location.to_string()),
            "folder" => {
                let trimmed = self.location.trim_end_matches('/');
                trimmed.rfind('/').map(|i| trimmed[..i].to_string())
            }
            "title" => get(|m| &m.title),
            "artist" => get(|m| &m.artist),
            "album" => get(|m| &m.album),
            "albumartist" => get(|m| &m.album_artist).or_else(|| get(|m| &m.artist)),
            "year" | "date" => get(|m| &m.year),
            "genre" => get(|m| &m.genre),
            "track" | "tracknumber" => get(|m| &m.track).map(|t| t.split('/').next().unwrap_or("").to_string()),
            "disc" | "discnumber" => get(|m| &m.disc),
            "comment" => get(|m| &m.comment),
            "composer" => get(|m| &m.composer),
            "length" => m.and_then(|m| m.length_ms).map(crate::text::format_duration_ms),
            "length_seconds" => m.and_then(|m| m.length_ms).map(|l| (l / 1000).to_string()),
            "bitrate" => m.and_then(|m| m.bitrate_kbps).map(|b| b.to_string()),
            "samplerate" => m.and_then(|m| m.sample_rate).map(|s| s.to_string()),
            "channels" => m.and_then(|m| m.channels).map(|c| c.to_string()),
            "type" | "codec" => get(|m| &m.codec),
            _ => None,
        }
    }
}

/// Parses ReplayGain values such as "-6.54 dB".
pub fn parse_gain(s: &str) -> Option<f32> {
    let t = s.trim().trim_end_matches(|c: char| c.is_alphabetic() || c.is_whitespace());
    t.trim().parse::<f32>().ok().filter(|v| v.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_parse() {
        assert_eq!(parse_gain("-6.54 dB"), Some(-6.54));
        assert_eq!(parse_gain("+1.5dB"), Some(1.5));
        assert_eq!(parse_gain("0.98"), Some(0.98));
        assert_eq!(parse_gain("junk"), None);
    }

    #[test]
    fn fields() {
        let meta = TrackMeta { artist: Some("A".into()), track: Some("4/12".into()), length_ms: Some(65_000), ..Default::default() };
        let f = TrackFields { location: "/x/y/z.mp3", meta: Some(&meta) };
        assert_eq!(f.field("track").as_deref(), Some("4"));
        assert_eq!(f.field("albumartist").as_deref(), Some("A"));
        assert_eq!(f.field("length").as_deref(), Some("1:05"));
        assert_eq!(f.field("folder").as_deref(), Some("/x/y"));
    }
}
