//! A track's cover: a picture in its tags, else an image file beside it
//! named as players name them (`cover.jpg`, `folder.jpg`, …).

use std::path::{Path, PathBuf};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardVisualKey};

/// A picture from a track's tags, encoded as `media_type` says.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub media_type: String,
    pub data: Box<[u8]>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Cover {
    Tagged(Picture),
    File(PathBuf),
}

/// Image file names a cover goes by, best first, without the extension.
const NAMES: [&str; 5] = ["cover", "folder", "front", "album", "albumart"];
const EXTENSIONS: [&str; 3] = ["jpg", "jpeg", "png"];

/// The cover of the track at `path`, if it has one.
pub fn find(path: &Path) -> Option<Cover> {
    tagged(path).map(Cover::Tagged).or_else(|| beside(path).map(Cover::File))
}

/// The front cover in the tags of the track at `path`, else the first
/// picture there.
pub fn tagged(path: &Path) -> Option<Picture> {
    let file = std::fs::File::open(path).ok()?;
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut probed = symphonia::default::get_probe()
        .format(&crate::decoder::hint_for(ext.as_deref()), mss, &FormatOptions::default(), &MetadataOptions::default())
        .ok()?;
    // Tags before the stream (ID3v2), then the container's own; the first
    // front cover of them, else the first picture.
    let before = probed.metadata.get().and_then(|m| m.current().and_then(best));
    let own = probed.format.metadata().current().and_then(best);
    [before, own].into_iter().flatten().min_by_key(|(front, _)| !front).map(|(_, p)| p)
}

/// The best picture in `rev`, and whether it is the front cover.
fn best(rev: &MetadataRevision) -> Option<(bool, Picture)> {
    let visuals = rev.visuals();
    let v = visuals.iter().find(|v| v.usage == Some(StandardVisualKey::FrontCover)).or(visuals.first())?;
    Some((v.usage == Some(StandardVisualKey::FrontCover), Picture { media_type: v.media_type.clone(), data: v.data.clone() }))
}

/// The image beside the track at `path` that is named as a cover.
pub fn beside(path: &Path) -> Option<PathBuf> {
    let files = std::fs::read_dir(path.parent()?).ok()?.filter_map(|e| Some(e.ok()?.path()));
    files.filter_map(|p| Some((rank(&p)?, p))).min().map(|(_, p)| p)
}

/// Where the file at `p` comes in [`NAMES`], if it is an image named so.
fn rank(p: &Path) -> Option<usize> {
    let ext = p.extension()?.to_str()?.to_ascii_lowercase();
    let stem = p.file_stem()?.to_str()?.to_ascii_lowercase();
    EXTENSIONS.contains(&ext.as_str()).then(|| NAMES.iter().position(|n| *n == stem))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("popamp-cover-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A FLAC file of one frame of silence with `pictures` as (FLAC picture
    /// type, media type, data) blocks.
    fn flac(pictures: &[(u32, &str, &[u8])]) -> Vec<u8> {
        let mut out = b"fLaC".to_vec();
        let mut block = |kind: u8, last: bool, body: &[u8]| {
            out.push(kind | if last { 0x80 } else { 0 });
            out.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
            out.extend_from_slice(body);
        };
        // STREAMINFO: 192-sample blocks, 44.1 kHz, stereo, 16 bits.
        let mut info = vec![0x00, 0xC0, 0x00, 0xC0, 0, 0, 0, 0, 0, 0];
        info.extend_from_slice(&[0x0A, 0xC4, 0x42, 0xF0, 0, 0, 0, 0]);
        info.extend_from_slice(&[0; 16]);
        block(0, pictures.is_empty(), &info);
        for (i, (kind, media_type, data)) in pictures.iter().enumerate() {
            let mut body = kind.to_be_bytes().to_vec();
            body.extend_from_slice(&(media_type.len() as u32).to_be_bytes());
            body.extend_from_slice(media_type.as_bytes());
            body.extend_from_slice(&[0; 4 * 5]); // no description; size and colours unknown
            body.extend_from_slice(&(data.len() as u32).to_be_bytes());
            body.extend_from_slice(data);
            block(6, i + 1 == pictures.len(), &body);
        }
        out.extend_from_slice(&silent_frame());
        out
    }

    /// A FLAC frame of 192 samples, both channels constant zero.
    fn silent_frame() -> Vec<u8> {
        let crc = |data: &[u8], poly: u16, bits: u32| {
            let top = 1u32 << (bits - 1);
            let mask = (1u32 << bits) - 1;
            data.iter().fold(0u32, |mut c, &b| {
                c ^= (b as u32) << (bits - 8);
                for _ in 0..8 {
                    c = if c & top != 0 { (c << 1) ^ poly as u32 } else { c << 1 } & mask;
                }
                c
            })
        };
        // Sync, 192 samples at 44.1 kHz, left/right, 16 bits, frame 0.
        let mut frame = vec![0xFF, 0xF8, 0x19, 0x18, 0x00];
        frame.push(crc(&frame, 0x07, 8) as u8);
        frame.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // two CONSTANT 0 subframes
        let footer = crc(&frame, 0x8005, 16) as u16;
        frame.extend_from_slice(&footer.to_be_bytes());
        frame
    }

    #[test]
    fn the_front_cover_in_the_tags_comes_first() {
        let d = dir("tags");
        let track = d.join("a.flac");
        std::fs::write(&track, flac(&[(0, "image/jpeg", b"other"), (3, "image/png", b"front")])).unwrap();
        std::fs::write(d.join("cover.jpg"), b"file").unwrap();
        let want = Picture { media_type: "image/png".into(), data: b"front".to_vec().into() };
        assert_eq!(find(&track), Some(Cover::Tagged(want)));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// An MP3 file of a few silent frames after an ID3v2.3 tag with a front
    /// cover of `data`.
    fn mp3(media_type: &str, data: &[u8]) -> Vec<u8> {
        let mut apic = vec![0]; // Latin-1 text
        apic.extend_from_slice(media_type.as_bytes());
        apic.extend_from_slice(&[0, 3, 0]); // front cover, no description
        apic.extend_from_slice(data);
        let mut frame = b"APIC".to_vec();
        frame.extend_from_slice(&(apic.len() as u32).to_be_bytes());
        frame.extend_from_slice(&[0, 0]);
        frame.extend_from_slice(&apic);
        // The tag size is "synchsafe": seven bits a byte.
        let n = frame.len() as u32;
        let mut out = b"ID3\x03\x00\x00".to_vec();
        out.extend_from_slice(&[(n >> 21) as u8 & 0x7F, (n >> 14) as u8 & 0x7F, (n >> 7) as u8 & 0x7F, n as u8 & 0x7F]);
        out.extend_from_slice(&frame);
        // MPEG-1 Layer III, 128 kbit/s, 44.1 kHz: 417 bytes a frame.
        for _ in 0..4 {
            out.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
            out.extend_from_slice(&[0; 413]);
        }
        out
    }

    #[test]
    fn an_mp3s_id3_cover_is_found() {
        let d = dir("mp3");
        let track = d.join("a.mp3");
        std::fs::write(&track, mp3("image/jpeg", b"id3 cover")).unwrap();
        let want = Picture { media_type: "image/jpeg".into(), data: b"id3 cover".to_vec().into() };
        assert_eq!(find(&track), Some(Cover::Tagged(want)));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn any_picture_in_the_tags_beats_a_file() {
        let d = dir("any");
        let track = d.join("a.flac");
        std::fs::write(&track, flac(&[(0, "image/jpeg", b"other")])).unwrap();
        std::fs::write(d.join("cover.jpg"), b"file").unwrap();
        assert!(matches!(find(&track), Some(Cover::Tagged(p)) if &*p.data == b"other"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn without_one_a_file_named_as_a_cover_is_taken() {
        let d = dir("files");
        let track = d.join("a.flac");
        std::fs::write(&track, flac(&[])).unwrap();
        assert_eq!(find(&track), None);
        for f in ["notes.jpg", "Folder.JPG", "cover.txt"] {
            std::fs::write(d.join(f), b"x").unwrap();
        }
        assert_eq!(find(&track), Some(Cover::File(d.join("Folder.JPG"))));
        std::fs::write(d.join("cover.png"), b"x").unwrap();
        assert_eq!(find(&track), Some(Cover::File(d.join("cover.png"))), "cover before folder");
        let _ = std::fs::remove_dir_all(&d);
    }
}
