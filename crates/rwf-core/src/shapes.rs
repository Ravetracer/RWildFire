// RWildFire - a fractal flame editor written in Rust
// Copyright (C) 2026 Christian Nielebock
//
// Ported from JWildfire - an image and animation processor written in Java
// Copyright (C) 1995-2026 Andreas Maschke
//
// This program is free software: you can redistribute it and/or modify it under the terms of
// the GNU Lesser General Public License as published by the Free Software Foundation, either
// version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Lesser General Public License for more details.
//
// You should have received a copy of the GNU Lesser General Public License along with this
// program. If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Point sets of the `svg_wf` and `text_wf` variations: the SVG document (resvg) or the text
//! (system font, ab_glyph) is rendered into an image whose non-empty pixels become the points,
//! as in JWildfire (which renders with svgSalamander / AWT, so edges differ slightly).

use std::sync::{Arc, Mutex, OnceLock};

use resvg::{tiny_skia, usvg};

use crate::math::ftoi;

/// A point with its color (0..255).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapePoint {
    pub x: f64,
    pub y: f64,
    pub rgb: [u8; 3],
}

fn font_db() -> Arc<usvg::fontdb::Database> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    })
    .clone()
}

fn cached(key: String, build: impl FnOnce() -> Vec<ShapePoint>) -> Arc<Vec<ShapePoint>> {
    type Entry = (String, Arc<Vec<ShapePoint>>);
    static CACHE: OnceLock<Mutex<Vec<Entry>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some((_, p)) = cache.lock().unwrap().iter().find(|(k, _)| *k == key) {
        return p.clone();
    }
    let p = Arc::new(build());
    let mut c = cache.lock().unwrap();
    if c.len() >= 16 {
        c.remove(0);
    }
    c.push((key, p.clone()));
    p
}

/// `SVGWFFunc.getPoints`: the SVG rendered at `resolution` times its size; points are
/// normalised to the bounding box of the non-empty pixels (`-0.5 .. 0.5` on the longer side).
pub fn svg_points(svg: &str, resolution: f64, antialias: bool) -> Arc<Vec<ShapePoint>> {
    cached(format!("svg#{resolution}#{antialias}#{svg}"), || render_svg(svg, resolution, antialias).unwrap_or_default())
}

fn render_svg(svg: &str, resolution: f64, antialias: bool) -> Option<Vec<ShapePoint>> {
    let mut opt = usvg::Options { fontdb: font_db(), ..Default::default() };
    if !antialias {
        opt.shape_rendering = usvg::ShapeRendering::CrispEdges;
        opt.text_rendering = usvg::TextRendering::OptimizeSpeed;
    }
    let tree = usvg::Tree::from_str(svg, &opt).ok()?;
    let w = ftoi(tree.size().width() as f64 * resolution);
    let h = ftoi(tree.size().height() as f64 * resolution);
    let mut pixmap = tiny_skia::Pixmap::new(w.max(1) as u32, h.max(1) as u32)?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(resolution as f32, resolution as f32), &mut pixmap.as_mut());
    let (w, h) = (pixmap.width() as i32, pixmap.height() as i32);
    let px = |x: i32, y: i32| pixmap.pixel(x as u32, y as u32).unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT);
    let set = |p: tiny_skia::PremultipliedColorU8| p.red() != 0 || p.green() != 0 || p.blue() != 0 || p.alpha() != 0;
    // bounding box (with JWildfire's `else if`: a pixel updates the minimum or the maximum)
    let (mut x0, mut x1, mut y0, mut y1) = (w - 1, 0, h - 1, 0);
    for i in 0..h {
        for j in 0..w {
            if set(px(j, i)) {
                if j < x0 {
                    x0 = j;
                } else if j > x1 {
                    x1 = j;
                }
                if i < y0 {
                    y0 = i;
                } else if i > y1 {
                    y1 = i;
                }
            }
        }
    }
    let (xs, ys) = (x1 - x0, y1 - y0);
    let max = xs.max(ys);
    let mut out = Vec::new();
    if max > 0 {
        for i in 0..h {
            for j in 0..w {
                let p = px(j, i);
                if set(p) {
                    let c = p.demultiply();
                    out.push(ShapePoint {
                        x: ((j - x0) as f64 - xs as f64 / 2.0) / max as f64,
                        y: ((i - y0) as f64 - ys as f64 / 2.0) / max as f64,
                        rgb: [c.red(), c.green(), c.blue()],
                    });
                }
            }
        }
    }
    Some(out)
}

/// Family names of the installed system fonts (sorted, unique).
pub fn font_families() -> Vec<String> {
    let mut names: Vec<String> = font_db().faces().flat_map(|f| f.families.iter().map(|(n, _)| n.clone())).collect();
    names.sort();
    names.dedup();
    names
}

/// Font of a family name; like Java's logical fonts, unknown names fall back to a sans-serif
/// system font.
fn font(name: &str) -> Option<ab_glyph::FontVec> {
    use usvg::fontdb::{Family, Query};
    let db = font_db();
    let families = [Family::Name(name), Family::Name("DejaVu Sans"), Family::Name("Liberation Sans"), Family::SansSerif];
    let id = families
        .iter()
        .find_map(|f| db.query(&Query { families: std::slice::from_ref(f), ..Default::default() }))
        .or_else(|| db.faces().next().map(|f| f.id))?;
    db.with_face_data(id, |data, index| ab_glyph::FontVec::try_from_vec_and_index(data.to_vec(), index).ok()).flatten()
}

/// `TextWFFunc.getPoints` (`TextTransformer`, one row, centred, without antialiasing): the
/// lit pixels of the text in an image padded by `font_size` on every side, normalised to the
/// image size.
pub fn text_points(text: &str, font_name: &str, font_size: i32, baseline: i32) -> Arc<Vec<ShapePoint>> {
    cached(format!("text#{text}#{font_name}#{font_size}#{baseline}"), || render_text(text, font_name, font_size, baseline).unwrap_or_default())
}

fn render_text(text: &str, font_name: &str, font_size: i32, baseline: i32) -> Option<Vec<ShapePoint>> {
    use ab_glyph::{Font, ScaleFont};
    if text.is_empty() || font_size <= 0 {
        return None;
    }
    let font = font(font_name)?;
    let fs = font_size as f32;
    let upem = font.units_per_em()?;
    // ab_glyph scales so that ascent - descent = PxScale; Java's font size is the em size
    let scale = ab_glyph::PxScale::from(fs * font.height_unscaled() / upem);
    let sf = font.as_scaled(scale);
    let width: f32 = text.chars().map(|c| sf.h_advance(font.glyph_id(c))).sum();
    // Font.getStringBounds: advance width x (ascent + descent + leading)
    let row_w = (width + 0.5) as i32;
    let row_h = (sf.ascent() - sf.descent() + sf.line_gap() + 0.5) as i32 + baseline;
    let (img_w, img_h) = ((row_w as f32 + 2.0 * fs) as i32, (row_h as f32 + 2.0 * fs) as i32);
    if img_w <= 0 || img_h <= 0 {
        return None;
    }
    let left = (img_w - row_w) / 2;
    let top = (img_h - row_h) / 2 + baseline;
    let base_y = (top + sf.ascent().ceil() as i32) as f32;
    let mut lit = vec![false; (img_w * img_h) as usize];
    let mut x = left as f32;
    for c in text.chars() {
        let id = font.glyph_id(c);
        let glyph = id.with_scale_and_position(scale, ab_glyph::point(x, base_y));
        x += sf.h_advance(id);
        if let Some(outline) = font.outline_glyph(glyph) {
            let b = outline.px_bounds();
            outline.draw(|gx, gy, cov| {
                let (px, py) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
                if cov >= 0.5 && px >= 0 && py >= 0 && px < img_w && py < img_h {
                    lit[(py * img_w + px) as usize] = true;
                }
            });
        }
    }
    let (w2, h2) = (img_w as f64 / 2.0, img_h as f64 / 2.0);
    let mut out = Vec::new();
    for i in 0..img_h {
        for j in 0..img_w {
            if lit[(i * img_w + j) as usize] {
                out.push(ShapePoint { x: (j as f64 - w2) / img_w as f64, y: (i as f64 - h2) / img_h as f64, rgb: [255; 3] });
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_rectangle_points() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect x="0" y="0" width="20" height="10" fill="#ff0000"/></svg>"##;
        let p = svg_points(svg, 2.0, false);
        assert_eq!(p.len(), 40 * 20);
        assert!(p.iter().all(|q| q.rgb == [255, 0, 0] && q.x.abs() <= 0.5 && q.y.abs() <= 0.5));
    }

    #[test]
    fn default_svg_renders() {
        assert!(svg_points(include_str!("../assets/svg_wf_default.svg"), 2.0, true).len() > 1000);
    }

    #[test]
    fn text_has_points_if_a_font_exists() {
        if font("Arial").is_none() {
            return; // no system fonts on this machine
        }
        let p = text_points("JWildfire", "Arial", 64, 0);
        assert!(p.len() > 500);
        assert!(p.iter().all(|q| q.x.abs() <= 0.5 && q.y.abs() <= 0.5));
    }
}
