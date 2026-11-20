//! A sheet kept inside the album file: a CUESHEET tag (FLAC, APE,
//! WavPack) or a FLAC CUESHEET block. Only the tags are read, never the
//! audio, and a FLAC file's pictures are skipped over.

use crate::text::decode_lossy;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The formats that may carry a sheet.
pub const EXTENSIONS: &[&str] = &["flac", "ape", "wv"];

/// Tags larger than this are not read.
const MAX_TAG: u64 = 16 << 20;

pub fn may_carry(location: &str) -> bool {
    let ext = Path::new(location).extension().and_then(|e| e.to_str()).unwrap_or_default();
    EXTENSIONS.iter().any(|e| e.eq_ignore_ascii_case(ext))
}

/// The text of the sheet in the file at `path`, if it has one.
pub fn sheet_text(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "flac" => flac(&mut file),
        "ape" | "wv" => ape_tag(&mut file).and_then(|items| find_item(&items)),
        _ => None,
    }
}

fn read_exact(r: &mut impl Read, len: u64) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    r.take(len).read_to_end(&mut buf).ok()?;
    (buf.len() as u64 == len).then_some(buf)
}

fn u32_le(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u64_be(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// The value of the `CUESHEET` item among `KEY=value` (or key, value) pairs.
fn find_item(items: &[(String, Vec<u8>)]) -> Option<String> {
    items.iter().find(|(k, _)| k.eq_ignore_ascii_case("cuesheet")).map(|(_, v)| decode_lossy(v))
}

/// A FLAC file's sheet: its CUESHEET tag, which names the tracks, else its CUESHEET block.
fn flac(r: &mut (impl Read + Seek)) -> Option<String> {
    skip_id3v2(r)?;
    if read_exact(r, 4)? != b"fLaC" {
        return None;
    }
    let blocks = flac_blocks(r, &[STREAM_INFO, COMMENTS, CUESHEET])?;
    let block = |kind: u8| blocks.iter().find(|(k, _)| *k == kind).map(|(_, b)| b.as_slice());
    if let Some(text) = block(COMMENTS).and_then(vorbis_comments).and_then(|items| find_item(&items)) {
        return Some(text);
    }
    cuesheet_block(block(CUESHEET)?, stream_rate(block(STREAM_INFO)?)?)
}

const STREAM_INFO: u8 = 0;
const COMMENTS: u8 = 4;
const CUESHEET: u8 = 5;

/// The metadata blocks of the kinds in `wanted`, read; the rest are skipped.
fn flac_blocks(r: &mut (impl Read + Seek), wanted: &[u8]) -> Option<Vec<(u8, Vec<u8>)>> {
    let mut blocks = Vec::new();
    loop {
        let header = read_exact(r, 4)?;
        let (kind, len) = (header[0] & 0x7F, u64::from(u32::from_be_bytes([0, header[1], header[2], header[3]])));
        if wanted.contains(&kind) && len <= MAX_TAG {
            blocks.push((kind, read_exact(r, len)?));
        } else {
            r.seek(SeekFrom::Current(len as i64)).ok()?;
        }
        if header[0] & 0x80 != 0 {
            return Some(blocks);
        }
    }
}

/// Steps over an ID3v2 tag some tools put before `fLaC`.
fn skip_id3v2(r: &mut (impl Read + Seek)) -> Option<()> {
    let head = read_exact(r, 10)?;
    if &head[..3] != b"ID3" {
        r.seek(SeekFrom::Start(0)).ok()?;
        return Some(());
    }
    let size = head[6..10].iter().fold(0u64, |n, b| (n << 7) | u64::from(b & 0x7F));
    r.seek(SeekFrom::Start(10 + size)).ok()?;
    Some(())
}

/// The sample rate in a STREAMINFO block: 20 bits from byte 10.
fn stream_rate(b: &[u8]) -> Option<u32> {
    let rate = (u32::from(*b.get(10)?) << 12) | (u32::from(*b.get(11)?) << 4) | (u32::from(*b.get(12)?) >> 4);
    (rate > 0).then_some(rate)
}

fn vorbis_comments(b: &[u8]) -> Option<Vec<(String, Vec<u8>)>> {
    let mut at = 4 + u32_le(b, 0)? as usize;
    let count = u32_le(b, at)?;
    at += 4;
    let mut items = Vec::new();
    for _ in 0..count {
        let len = u32_le(b, at)? as usize;
        let field = b.get(at + 4..at + 4 + len)?;
        at += 4 + len;
        if let Some(eq) = field.iter().position(|&c| c == b'=') {
            items.push((String::from_utf8_lossy(&field[..eq]).into_owned(), field[eq + 1..].to_vec()));
        }
    }
    Some(items)
}

/// A CUESHEET block written out as a sheet. The block has no titles;
/// a track starts at its index 1 (after its pregap), and the lead-out
/// (170 on a CD, 255 elsewhere) and data tracks are left out.
fn cuesheet_block(b: &[u8], rate: u32) -> Option<String> {
    let mut text = String::from("FILE \"\" WAVE\n");
    let mut at = 396;
    for _ in 0..*b.get(395)? {
        let (number, start, audio, next) = block_track(b, at)?;
        at = next;
        if audio && number != 170 && number != 255 {
            let frames = start * 75 / u64::from(rate);
            let (m, s, f) = (frames / 75 / 60, frames / 75 % 60, frames % 75);
            text.push_str(&format!("  TRACK {number:02} AUDIO\n    INDEX 01 {m:02}:{s:02}:{f:02}\n"));
        }
    }
    Some(text)
}

/// The track of a CUESHEET block at `at`: its number, start in samples,
/// whether it is audio, and where the next track begins.
fn block_track(b: &[u8], at: usize) -> Option<(u8, u64, bool, usize)> {
    let offset = u64_be(b, at)?;
    let audio = b.get(at + 21)? & 0x80 == 0;
    let points = (0..usize::from(*b.get(at + 35)?)).map(|i| at + 36 + i * 12);
    let points: Option<Vec<(u8, u64)>> = points.map(|p| Some((*b.get(p + 8)?, u64_be(b, p)?))).collect();
    let points = points?;
    let start = points.iter().find(|(n, _)| *n == 1).or(points.first()).map_or(0, |(_, o)| *o);
    Some((*b.get(at + 8)?, offset + start, audio, at + 36 + points.len() * 12))
}

/// The items of an APEv2 tag at the end of the file (before an ID3v1 tag, if any).
fn ape_tag(r: &mut (impl Read + Seek)) -> Option<Vec<(String, Vec<u8>)>> {
    let (at, footer) = ape_footer(r)?;
    let (size, count) = (u64::from(u32_le(&footer, 12)?), u32_le(&footer, 16)?);
    if !(32..=MAX_TAG).contains(&size) || size - 32 > at {
        return None;
    }
    r.seek(SeekFrom::Start(at + 32 - size)).ok()?;
    ape_items(&read_exact(r, size - 32)?, count)
}

/// Where the tag's footer is, and the footer.
fn ape_footer(r: &mut (impl Read + Seek)) -> Option<(u64, Vec<u8>)> {
    let len = r.seek(SeekFrom::End(0)).ok()?;
    [32u64, 160].into_iter().filter(|back| *back <= len).find_map(|back| {
        r.seek(SeekFrom::Start(len - back)).ok()?;
        let f = read_exact(r, 32)?;
        (&f[..8] == b"APETAGEX").then_some((len - back, f))
    })
}

/// Each item: value length, flags, the key ending in a zero, the value.
fn ape_items(b: &[u8], count: u32) -> Option<Vec<(String, Vec<u8>)>> {
    let mut items = Vec::new();
    let mut i = 0;
    for _ in 0..count {
        let value_len = u32_le(b, i)? as usize;
        let key_end = i + 8 + b.get(i + 8..)?.iter().position(|&c| c == 0)?;
        let key = String::from_utf8_lossy(&b[i + 8..key_end]).into_owned();
        items.push((key, b.get(key_end + 1..key_end + 1 + value_len)?.to_vec()));
        i = key_end + 1 + value_len;
    }
    Some(items)
}
