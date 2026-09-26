mod gauge;
mod map;
mod stats;
use crate::{
    config::{Config, parse_color},
    maps::{Maps, Tile, neighbors, tile, world},
    telemetry::{Sample, Track},
};
use anyhow::{Result, bail};
use fontdue::{Font, FontSettings, Metrics};
use std::collections::{HashMap, VecDeque};
use tiny_skia::*;

#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
pub struct Renderer {
    pub cfg: Config,
    pub bounds: Bounds,
    font: Font,
    digits: Font,
    glyphs: HashMap<(char, u16, bool), (Metrics, Vec<u8>)>,
    raster: HashMap<Tile, Pixmap>,
    raster_order: VecDeque<Tile>,
    gauge: Option<Pixmap>,
}
fn color(s: &str) -> Color {
    let c = parse_color(s).unwrap();
    Color::from_rgba8(c[0], c[1], c[2], 255)
}
fn paint(s: &str, alpha: f32) -> Paint<'static> {
    let mut p = Paint::default();
    let mut c = color(s);
    c.set_alpha(alpha);
    p.set_color(c);
    p.anti_alias = true;
    p
}
pub fn line(p: &mut Pixmap, pts: &[(f32, f32)], width: f32, c: &str, alpha: f32) {
    if pts.len() < 2 {
        return;
    }
    let mut b = PathBuilder::new();
    b.move_to(pts[0].0, pts[0].1);
    for x in &pts[1..] {
        b.line_to(x.0, x.1)
    }
    if let Some(path) = b.finish() {
        p.stroke_path(
            &path,
            &paint(c, alpha),
            &Stroke {
                width,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
}
fn polygon(p: &mut Pixmap, pts: &[(f32, f32)], c: &str, alpha: f32) {
    if pts.len() < 3 {
        return;
    }
    let mut b = PathBuilder::new();
    b.move_to(pts[0].0, pts[0].1);
    for x in &pts[1..] {
        b.line_to(x.0, x.1)
    }
    b.close();
    if let Some(path) = b.finish() {
        p.fill_path(
            &path,
            &paint(c, alpha),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}
fn circle(p: &mut Pixmap, x: f32, y: f32, r: f32, c: &str, alpha: f32) {
    if let Some(path) = PathBuilder::from_circle(x, y, r) {
        p.fill_path(
            &path,
            &paint(c, alpha),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}
#[allow(clippy::too_many_arguments)] // Drawing primitive: geometry and paint are explicit.
fn arc(p: &mut Pixmap, cx: f32, cy: f32, r: f32, a: f32, b: f32, w: f32, c: &str, alpha: f32) {
    let n = ((b - a).abs() * 2.).max(3.) as usize;
    let pts: Vec<_> = (0..=n)
        .map(|i| {
            let theta = (a + (b - a) * i as f32 / n as f32).to_radians();
            (cx + r * theta.cos(), cy + r * theta.sin())
        })
        .collect();
    line(p, &pts, w, c, alpha)
}
impl Renderer {
    pub fn new(cfg: Config) -> Result<Self> {
        cfg.validate()?;
        let mut xmin = cfg.width as f32;
        let mut ymin = cfg.height as f32;
        let (mut xmax, mut ymax) = (0f32, 0f32);
        for (w, hfactor) in [
            (&cfg.gauge, 1.04),
            (&cfg.map, 1.13),
            (&cfg.stats, cfg.stats_height_factor()),
        ] {
            if w.enabled {
                let x = w.x * cfg.width as f32;
                let y = w.y * cfg.height as f32;
                let size = w.size * cfg.width as f32;
                xmin = xmin.min(x - 8.);
                ymin = ymin.min(y - 8.);
                xmax = xmax.max(x + size + 8.);
                ymax = ymax.max(y + size * hfactor + 8.);
                if y + size * hfactor > cfg.height as f32 {
                    bail!("A widget extends below the canvas; reduce y or size")
                }
            }
        }
        if xmax == 0. {
            bail!("Enable at least one widget")
        };
        let x = (xmin.max(0.) as u32 / 2) * 2;
        let y = (ymin.max(0.) as u32 / 2) * 2;
        let right = ((xmax.ceil() as u32).div_ceil(2) * 2).min(cfg.width);
        let bottom = ((ymax.ceil() as u32).div_ceil(2) * 2).min(cfg.height);
        let bounds = Bounds {
            x,
            y,
            width: right - x,
            height: bottom - y,
        };
        Ok(Self {
            cfg,
            bounds,
            font: Font::from_bytes(
                include_bytes!("../assets/fonts/NotoSans.ttf") as &[u8],
                FontSettings::default(),
            )
            .map_err(anyhow::Error::msg)?,
            digits: Font::from_bytes(
                include_bytes!("../assets/fonts/BarlowCondensed-BoldItalic.ttf") as &[u8],
                FontSettings::default(),
            )
            .map_err(anyhow::Error::msg)?,
            glyphs: HashMap::new(),
            raster: HashMap::new(),
            raster_order: VecDeque::new(),
            gauge: None,
        })
    }
    fn glyph(&mut self, ch: char, size: f32, digits: bool) -> &(Metrics, Vec<u8>) {
        let digits = digits && self.digits.lookup_glyph_index(ch) != 0;
        let key = (ch, size.round() as u16, digits);
        self.glyphs.entry(key).or_insert_with(|| {
            if digits {
                self.digits.rasterize(ch, size.round())
            } else {
                self.font.rasterize(ch, size.round())
            }
        })
    }
    fn text_width(&mut self, text: &str, size: f32, digits: bool) -> f32 {
        text.chars()
            .map(|c| self.glyph(c, size, digits).0.advance_width)
            .sum()
    }
    #[allow(clippy::too_many_arguments)] // Shared text primitive used by independent widgets.
    fn text(
        &mut self,
        p: &mut Pixmap,
        text: &str,
        x: f32,
        baseline: f32,
        size: f32,
        digits: bool,
        c: &str,
        max_width: f32,
    ) {
        let width = self.text_width(text, size, digits);
        let size = if width > max_width {
            (size * max_width / width).max(size * 0.55)
        } else {
            size
        };
        let mut text = text.to_string();
        while self.text_width(&text, size, digits) > max_width && text.chars().count() > 2 {
            text.pop();
            if text.ends_with('…') {
                text.pop();
            }
            text.pop();
            text.push('…');
        }
        let shadow = self.cfg.colors.shadow.clone();
        for (dx, dy, col, opacity) in [(1.5, 2.5, shadow.as_str(), 0.7), (0., 0., c, 1.)] {
            let rgb = parse_color(col).unwrap();
            let mut pen = x + dx;
            for ch in text.chars() {
                let (m, b) = self.glyph(ch, size, digits);
                let ox = pen.round() as i32 + m.xmin;
                let oy = (baseline + dy).round() as i32 - m.height as i32 - m.ymin;
                let w = p.width() as i32;
                let h = p.height() as i32;
                for gy in 0..m.height {
                    let y = oy + gy as i32;
                    if y < 0 || y >= h {
                        continue;
                    }
                    for gx in 0..m.width {
                        let x = ox + gx as i32;
                        if x < 0 || x >= w {
                            continue;
                        }
                        let a = (b[gy * m.width + gx] as f32 * opacity) as u32;
                        if a == 0 {
                            continue;
                        }
                        let i = (y as usize * w as usize + x as usize) * 4;
                        let pixel = &mut p.data_mut()[i..i + 4];
                        for c in 0..3 {
                            pixel[c] = ((rgb[c] as u32 * a + pixel[c] as u32 * (255 - a) + 127)
                                / 255) as u8
                        }
                        pixel[3] = (a + pixel[3] as u32 * (255 - a) / 255).min(255) as u8;
                    }
                }
                pen += m.advance_width;
            }
        }
    }
    fn pos(&self, w: &crate::config::Widget) -> (f32, f32, f32) {
        (
            w.x * self.cfg.width as f32 - self.bounds.x as f32,
            w.y * self.cfg.height as f32 - self.bounds.y as f32,
            w.size * self.cfg.width as f32,
        )
    }
    pub fn frame(&mut self, track: &Track, s: Option<&Sample>, maps: &mut Maps) -> Result<Pixmap> {
        let mut p = Pixmap::new(self.bounds.width, self.bounds.height)
            .ok_or_else(|| anyhow::anyhow!("Overlay allocation failed"))?;
        self.draw_gauge(&mut p, s)?;
        self.draw_map(&mut p, track, s, maps)?;
        self.draw_stats(&mut p, track, s, maps)?;
        Ok(p)
    }
    pub fn full_frame(
        &mut self,
        track: &Track,
        s: Option<&Sample>,
        maps: &mut Maps,
    ) -> Result<Pixmap> {
        let part = self.frame(track, s, maps)?;
        let mut full = Pixmap::new(self.cfg.width, self.cfg.height).unwrap();
        full.draw_pixmap(
            self.bounds.x as i32,
            self.bounds.y as i32,
            part.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
        Ok(full)
    }
}
pub fn distance_text(done: f64, total: f64, cfg: &Config) -> String {
    let total = if cfg.total_decimals == 0 && cfg.total_round_up {
        total.ceil()
    } else {
        total
    };
    let unit = if cfg.ru() { "из" } else { "of" };
    let km = if cfg.ru() { "км" } else { "km" };
    format!(
        "{:.*} {unit} {:.*} {km}",
        cfg.distance_decimals, done, cfg.total_decimals, total
    )
}
fn raster_tile(data: &crate::maps::Data, t: Tile, cfg: &Config) -> Result<Pixmap> {
    let n = cfg.map_tile_pixels;
    let mut p = Pixmap::new(n, n).unwrap();
    p.fill(color(&cfg.colors.land));
    let pt = |q: &crate::maps::Geo| {
        let (x, y) = world(q.lat, q.lon, t.z);
        (
            (x - t.x as f64) as f32 * n as f32,
            (y - t.y as f64) as f32 * n as f32,
        )
    };
    for f in &data.elements {
        if f.geometry.len() < 3 {
            continue;
        }
        let c = if f.tags.get("natural").is_some_and(|x| x == "water")
            || f.tags.get("waterway").is_some_and(|x| x == "riverbank")
        {
            Some(&cfg.colors.water)
        } else if f.tags.contains_key("building") {
            Some(&cfg.colors.building)
        } else if f.tags.contains_key("landuse")
            || f.tags.get("leisure").is_some_and(|x| x == "park")
        {
            Some(&cfg.colors.park)
        } else {
            None
        };
        if let Some(c) = c {
            polygon(
                &mut p,
                &f.geometry.iter().map(pt).collect::<Vec<_>>(),
                c,
                1.,
            )
        }
    }
    for casing in [true, false] {
        for f in &data.elements {
            if let Some(kind) = f.tags.get("highway") {
                let w = match kind.as_str() {
                    "motorway" | "trunk" => 10.,
                    "primary" | "secondary" => 7.,
                    "footway" | "path" | "cycleway" => 2.5,
                    _ => 4.,
                };
                let w = (w + if casing { 3. } else { 0. }) * n as f32 / 512.;
                line(
                    &mut p,
                    &f.geometry.iter().map(pt).collect::<Vec<_>>(),
                    w,
                    if casing {
                        &cfg.colors.road_edge
                    } else {
                        &cfg.colors.road
                    },
                    1.,
                )
            } else if !casing && f.tags.contains_key("waterway") {
                line(
                    &mut p,
                    &f.geometry.iter().map(pt).collect::<Vec<_>>(),
                    5. * n as f32 / 512.,
                    &cfg.colors.water,
                    1.,
                )
            }
        }
    }
    Ok(p)
}
