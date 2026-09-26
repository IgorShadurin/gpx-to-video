use super::*;
impl Renderer {
    pub(super) fn draw_map(
        &mut self,
        p: &mut Pixmap,
        track: &Track,
        s: Option<&Sample>,
        maps: &mut Maps,
    ) -> Result<()> {
        let c = self.cfg.colors.clone();
        if self.cfg.map.enabled && self.cfg.map_source != "none" {
            let (x, y, size) = self.pos(&self.cfg.map);
            if let Some(s) = s {
                let d = size.round() as u32;
                let mut m = Pixmap::new(d, d).unwrap();
                m.fill(color(&c.land));
                let (wx, wy) = world(s.lat, s.lon, self.cfg.map_zoom);
                let t = tile(s.lat, s.lon, self.cfg.map_zoom);
                let tile_px = self.cfg.map_tile_pixels as f32;
                let scale = size / self.cfg.map_tile_pixels as f32; // one tile across the diameter
                for t in neighbors(t) {
                    if !self.raster.contains_key(&t) {
                        let data = maps.data(t)?;
                        let raster = raster_tile(data, t, &self.cfg)?;
                        while self.raster.len() >= 18 {
                            if let Some(old) = self.raster_order.pop_front() {
                                self.raster.remove(&old);
                            }
                        }
                        self.raster.insert(t, raster);
                    }
                    self.raster_order.retain(|v| *v != t);
                    self.raster_order.push_back(t);
                    let (dx, dy) = (
                        ((t.x as f64 - wx) * tile_px as f64) as f32,
                        ((t.y as f64 - wy) * tile_px as f64) as f32,
                    );
                    m.draw_pixmap(
                        0,
                        0,
                        self.raster[&t].as_ref(),
                        &PixmapPaint {
                            quality: FilterQuality::Bilinear,
                            ..Default::default()
                        },
                        Transform::from_scale(scale, scale)
                            .post_translate(size * 0.5 + dx * scale, size * 0.5 + dy * scale),
                        None,
                    );
                }
                let end = track.points.partition_point(|v| v.time <= s.time);
                let start = end.saturating_sub(800);
                let mut pts = vec![];
                let mut previous = None;
                for q in &track.points[start..end] {
                    if q.segment != s.segment {
                        continue;
                    }
                    if previous.is_some_and(|t| q.time - t > track.max_gap) {
                        line(&mut m, &pts, size * 0.014, &c.route, 1.);
                        pts.clear()
                    }
                    let (a, b) = world(q.lat, q.lon, self.cfg.map_zoom);
                    pts.push((
                        size * 0.5 + (a - wx) as f32 * size,
                        size * 0.5 + (b - wy) as f32 * size,
                    ));
                    previous = Some(q.time);
                }
                pts.push((size * 0.5, size * 0.5));
                line(&mut m, &pts, size * 0.023, &c.shadow, 0.85);
                line(&mut m, &pts, size * 0.011, &c.route, 1.);
                let mut mask = Mask::new(d, d).unwrap();
                mask.fill_path(
                    &PathBuilder::from_circle(size * 0.5, size * 0.5, size * 0.478).unwrap(),
                    FillRule::Winding,
                    true,
                    Transform::identity(),
                );
                m.apply_mask(&mask);
                p.draw_pixmap(
                    x.round() as i32,
                    y.round() as i32,
                    m.as_ref(),
                    &PixmapPaint::default(),
                    Transform::identity(),
                    None,
                );
                let theta = (s.course - 90.).to_radians() as f32;
                let (cx, cy) = (x + size * 0.5, y + size * 0.5);
                let point = |r: f32, a: f32| {
                    (
                        cx + size * r * (theta + a).cos(),
                        cy + size * r * (theta + a).sin(),
                    )
                };
                polygon(
                    p,
                    &[point(0.057, 0.), point(0.04, 2.5), point(0.04, -2.5)],
                    &c.shadow,
                    1.,
                );
                polygon(
                    p,
                    &[point(0.045, 0.), point(0.028, 2.5), point(0.028, -2.5)],
                    &c.text,
                    1.,
                );
            } else {
                circle(
                    p,
                    x + size * 0.5,
                    y + size * 0.5,
                    size * 0.477,
                    &c.land,
                    0.8,
                );
                let text = if self.cfg.ru() {
                    "НЕТ GPS"
                } else {
                    "NO GPS"
                };
                self.text(
                    p,
                    text,
                    x + size * 0.25,
                    y + size * 0.53,
                    size * 0.065,
                    false,
                    &c.text,
                    size * 0.5,
                );
            }
            arc(
                p,
                x + size * 0.5,
                y + size * 0.5,
                size * 0.482,
                0.,
                360.,
                size * 0.01,
                &c.shadow,
                1.,
            );
            arc(
                p,
                x + size * 0.5,
                y + size * 0.5,
                size * 0.49,
                0.,
                360.,
                size * 0.006,
                &c.ring,
                0.85,
            );
            circle(
                p,
                x + size * 0.5,
                y + size * 0.025,
                size * 0.048,
                &c.shadow,
                1.,
            );
            let north = if self.cfg.ru() { "С" } else { "N" };
            let tw = self.text_width(north, size * 0.047, false);
            self.text(
                p,
                north,
                x + size * 0.5 - tw * 0.5,
                y + size * 0.041,
                size * 0.047,
                false,
                &c.text,
                size * 0.12,
            );
            let credit = if self.cfg.map_source == "demo" {
                "DEMO MAP · SYNTHETIC"
            } else {
                "© OpenStreetMap contributors"
            };
            self.text(
                p,
                credit,
                x + size * 0.035,
                y + size * 1.065,
                size * 0.033,
                false,
                &c.text,
                size * 0.94,
            );
        }
        Ok(())
    }
}
