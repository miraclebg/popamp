//! Skin mouse cursors (`*.cur`, sometimes animated `.ani` renamed to
//! `.cur`). Which cursor shows where is decided by the windows.

/// The cursors a classic skin can provide, one file each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CursorKind {
    Close,
    EqClose,
    EqNormal,
    EqSlid,
    EqTitle,
    MainMenu,
    MMenu,
    Min,
    Normal,
    PClose,
    PNormal,
    PosBar,
    PSize,
    PTBar,
    PVScroll,
    PWinBut,
    PWSNorm,
    PWSSize,
    SongName,
    TitleBar,
    VolBal,
    WinBut,
    WSNormal,
    WSPosBar,
}

impl CursorKind {
    pub const ALL: [CursorKind; 24] = [
        CursorKind::Close,
        CursorKind::EqClose,
        CursorKind::EqNormal,
        CursorKind::EqSlid,
        CursorKind::EqTitle,
        CursorKind::MainMenu,
        CursorKind::MMenu,
        CursorKind::Min,
        CursorKind::Normal,
        CursorKind::PClose,
        CursorKind::PNormal,
        CursorKind::PosBar,
        CursorKind::PSize,
        CursorKind::PTBar,
        CursorKind::PVScroll,
        CursorKind::PWinBut,
        CursorKind::PWSNorm,
        CursorKind::PWSSize,
        CursorKind::SongName,
        CursorKind::TitleBar,
        CursorKind::VolBal,
        CursorKind::WinBut,
        CursorKind::WSNormal,
        CursorKind::WSPosBar,
    ];

    pub fn file_name(self) -> &'static str {
        match self {
            CursorKind::Close => "close.cur",
            CursorKind::EqClose => "eqclose.cur",
            CursorKind::EqNormal => "eqnormal.cur",
            CursorKind::EqSlid => "eqslid.cur",
            CursorKind::EqTitle => "eqtitle.cur",
            CursorKind::MainMenu => "mainmenu.cur",
            CursorKind::MMenu => "mmenu.cur",
            CursorKind::Min => "min.cur",
            CursorKind::Normal => "normal.cur",
            CursorKind::PClose => "pclose.cur",
            CursorKind::PNormal => "pnormal.cur",
            CursorKind::PosBar => "posbar.cur",
            CursorKind::PSize => "psize.cur",
            CursorKind::PTBar => "ptbar.cur",
            CursorKind::PVScroll => "pvscroll.cur",
            CursorKind::PWinBut => "pwinbut.cur",
            CursorKind::PWSNorm => "pwsnorm.cur",
            CursorKind::PWSSize => "pwssize.cur",
            CursorKind::SongName => "songname.cur",
            CursorKind::TitleBar => "titlebar.cur",
            CursorKind::VolBal => "volbal.cur",
            CursorKind::WinBut => "winbut.cur",
            CursorKind::WSNormal => "wsnormal.cur",
            CursorKind::WSPosBar => "wsposbar.cur",
        }
    }
}

/// A decoded cursor: RGBA pixels and the click point.
#[derive(Debug, Clone, PartialEq)]
pub struct CursorImage {
    pub w: u16,
    pub h: u16,
    pub hot_x: u16,
    pub hot_y: u16,
    pub rgba: Vec<u8>,
}

/// Decodes a `.cur` file. Animated cursors use their first frame.
pub fn parse(bytes: &[u8]) -> Option<CursorImage> {
    if bytes.starts_with(b"RIFF") {
        return parse(ani_first_frame(bytes)?);
    }
    // A .cur is an .ico with type 2, whose directory entry stores the
    // hotspot where an icon has its planes and bit depth.
    if bytes.len() < 22 || bytes[2] != 2 {
        return None;
    }
    let hot_x = u16::from_le_bytes([bytes[10], bytes[11]]);
    let hot_y = u16::from_le_bytes([bytes[12], bytes[13]]);
    let mut ico = bytes.to_vec();
    ico[2] = 1;
    ico[10..14].copy_from_slice(&[1, 0, 0, 0]);
    let img = image::load_from_memory_with_format(&ico, image::ImageFormat::Ico).ok()?.to_rgba8();
    let (w, h) = (u16::try_from(img.width()).ok()?, u16::try_from(img.height()).ok()?);
    Some(CursorImage { w, h, hot_x: hot_x.min(w.saturating_sub(1)), hot_y: hot_y.min(h.saturating_sub(1)), rgba: img.into_raw() })
}

/// The first `icon` chunk of an animated cursor (RIFF `ACON`).
fn ani_first_frame(bytes: &[u8]) -> Option<&[u8]> {
    let mut i = 12;
    while i + 8 <= bytes.len() {
        let id = &bytes[i..i + 4];
        let len = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().ok()?) as usize;
        if id == b"LIST" {
            i += 12; // step into the list
            continue;
        }
        if id == b"icon" {
            return bytes.get(i + 8..i + 8 + len);
        }
        i += 8 + len + (len & 1);
    }
    None
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A 4x3 cursor with its hotspot at (2, 1), PNG inside as modern
    /// cursor editors write it.
    pub(crate) fn sample_cur() -> Vec<u8> {
        let mut img = image::RgbaImage::new(4, 3);
        img.put_pixel(2, 1, image::Rgba([255, 0, 0, 255]));
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let mut cur = vec![0, 0, 2, 0, 1, 0]; // reserved, type 2 (cursor), 1 image
        cur.extend_from_slice(&[4, 3, 0, 0]); // width, height, colours, reserved
        cur.extend_from_slice(&2u16.to_le_bytes()); // hotspot x
        cur.extend_from_slice(&1u16.to_le_bytes()); // hotspot y
        cur.extend_from_slice(&(png.len() as u32).to_le_bytes());
        cur.extend_from_slice(&22u32.to_le_bytes()); // offset of the image
        cur.extend_from_slice(&png);
        cur
    }

    fn chunk(id: &[u8], data: &[u8]) -> Vec<u8> {
        let mut c = id.to_vec();
        c.extend_from_slice(&(data.len() as u32).to_le_bytes());
        c.extend_from_slice(data);
        if data.len() % 2 == 1 {
            c.push(0);
        }
        c
    }

    #[test]
    fn reads_pixels_and_hotspot() {
        let c = parse(&sample_cur()).unwrap();
        assert_eq!((c.w, c.h, c.hot_x, c.hot_y), (4, 3, 2, 1));
        let (x, y, w) = (2, 1, 4);
        let px = (y * w + x) * 4;
        assert_eq!(&c.rgba[px..px + 4], &[255, 0, 0, 255]);
        assert_eq!(&c.rgba[0..4], &[0, 0, 0, 0]);
    }

    #[test]
    fn animated_cursor_uses_first_frame() {
        let mut fram = b"fram".to_vec();
        fram.extend(chunk(b"icon", &sample_cur()));
        let mut body = b"ACON".to_vec();
        body.extend(chunk(b"anih", &[0; 36]));
        body.extend(chunk(b"LIST", &fram));
        let ani = chunk(b"RIFF", &body);
        assert_eq!(parse(&ani), parse(&sample_cur()));
    }

    #[test]
    fn rejects_other_files() {
        assert_eq!(parse(b"BM not a cursor at all......"), None);
        let mut icon = sample_cur();
        icon[2] = 1; // an icon, not a cursor
        assert_eq!(parse(&icon), None);
        assert_eq!(parse(&sample_cur()[..30]), None);
    }
}
