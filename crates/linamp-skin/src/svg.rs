//! A vector skin's SVG pictures, rasterized for the size they are shown at
//! and kept until the skin changes.

use crate::bitmap::Rgba;
use crate::sprites::Rect;
use resvg::{tiny_skia, usvg};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Picture number, area of it, and the screen size it is drawn at.
type Key = (u16, Rect, i32, i32);

#[derive(Default)]
pub struct Svgs {
    trees: Vec<usvg::Tree>,
    cache: RefCell<HashMap<Key, Rc<Rgba>>>,
}

impl Svgs {
    /// Parses an SVG file and returns its number.
    pub fn add(&mut self, data: &[u8]) -> Result<u16, String> {
        let id = u16::try_from(self.trees.len()).map_err(|_| "too many SVG files".to_string())?;
        let tree = usvg::Tree::from_data(data, &usvg::Options::default()).map_err(|e| e.to_string())?;
        self.trees.push(tree);
        Ok(id)
    }

    /// The size of picture `id` in its own units.
    pub fn size(&self, id: u16) -> Option<(f32, f32)> {
        self.trees.get(id as usize).map(|t| (t.size().width(), t.size().height()))
    }

    /// Area `src` of picture `id` drawn to fill `w` x `h` pixels.
    pub fn raster(&self, id: u16, src: Rect, w: i32, h: i32) -> Option<Rc<Rgba>> {
        let key = (id, src, w, h);
        if let Some(r) = self.cache.borrow().get(&key) {
            return Some(r.clone());
        }
        let tree = self.trees.get(id as usize)?;
        let mut pixmap = tiny_skia::Pixmap::new(w.max(1) as u32, h.max(1) as u32)?;
        let (sx, sy) = (w as f32 / src.w.max(1) as f32, h as f32 / src.h.max(1) as f32);
        let at = tiny_skia::Transform::from_row(sx, 0.0, 0.0, sy, -src.x as f32 * sx, -src.y as f32 * sy);
        resvg::render(tree, at, &mut pixmap.as_mut());
        let px = pixmap.pixels().iter().map(|p| u32::from_be_bytes([p.alpha(), p.red(), p.green(), p.blue()])).collect();
        let raster = Rc::new(Rgba { w: pixmap.width() as i32, h: pixmap.height() as i32, px });
        let mut cache = self.cache.borrow_mut();
        // Old scales pile up as the user changes the size; start over then.
        if cache.len() > 2048 {
            cache.clear();
        }
        cache.insert(key, raster.clone());
        Some(raster)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HALF_RED: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="1">
        <rect x="0" y="0" width="1" height="1" fill="#ff0000"/></svg>"##;

    #[test]
    fn pictures_render_at_the_asked_size_and_area() {
        let mut s = Svgs::default();
        let id = s.add(HALF_RED).unwrap();
        let full = s.raster(id, Rect { x: 0, y: 0, w: 2, h: 1 }, 4, 2).unwrap();
        assert_eq!(full.px, [0xffff0000, 0xffff0000, 0, 0, 0xffff0000, 0xffff0000, 0, 0]);
        let right = s.raster(id, Rect { x: 1, y: 0, w: 1, h: 1 }, 3, 3).unwrap();
        assert!(right.px.iter().all(|&p| p == 0), "the right half is empty");
    }

    #[test]
    fn broken_files_are_errors() {
        assert!(Svgs::default().add(b"<svg").is_err());
    }
}
