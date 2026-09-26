use super::*;
impl Renderer {
    pub(super) fn draw_stats(
        &mut self,
        p: &mut Pixmap,
        track: &Track,
        s: Option<&Sample>,
        maps: &mut Maps,
    ) -> Result<()> {
        let c = self.cfg.colors.clone();
        if self.cfg.stats.enabled {
            let (x, y, size) = self.pos(&self.cfg.stats);
            let mut yy = y;
            let fs = size * 0.055;
            if self.cfg.show_location {
                let fs = fs * self.cfg.text_sizes.location;
                let place = if let Some(s) = s {
                    maps.location(s, &self.cfg)?
                } else {
                    String::new()
                };
                let label = if place.is_empty() {
                    if self.cfg.ru() {
                        "МАРШРУТ"
                    } else {
                        "ROUTE"
                    }
                    .into()
                } else {
                    place
                };
                self.text(p, &label, x, yy + fs, fs, false, &c.accent, size);
                yy += fs * 1.65;
            }
            if self.cfg.show_distance {
                let fs = fs * self.cfg.text_sizes.distance;
                let text = if let Some(s) = s {
                    distance_text(
                        s.distance / 1000.,
                        self.cfg.total_km.unwrap_or(track.total() / 1000.),
                        &self.cfg,
                    )
                } else {
                    if self.cfg.ru() {
                        "— из — км"
                    } else {
                        "— of — km"
                    }
                    .into()
                };
                self.text(p, &text, x, yy + fs * 1.8, fs * 1.8, true, &c.text, size);
                yy += fs * 2.4;
            }
            if self.cfg.show_elevation {
                let fs = fs * self.cfg.text_sizes.elevation;
                let v = s
                    .and_then(|v| v.elevation)
                    .map(|v| format!("{v:0.0}"))
                    .unwrap_or("—".into());
                let label = if self.cfg.ru() {
                    format!("ВЫСОТА   {v} м")
                } else {
                    format!("ELEVATION   {v} m")
                };
                self.text(
                    p,
                    &label,
                    x,
                    yy + fs * 0.65,
                    fs * 0.65,
                    false,
                    &c.text,
                    size,
                );
                yy += fs * 1.1;
            }
            if self.cfg.show_coordinates {
                let fs = fs * self.cfg.text_sizes.coordinates;
                let label = s
                    .map(|v| format!("{:0.5}°  {:0.5}°", v.lat, v.lon))
                    .unwrap_or("GPS —".into());
                self.text(
                    p,
                    &label,
                    x,
                    yy + fs * 0.58,
                    fs * 0.58,
                    false,
                    &c.ring,
                    size,
                );
                yy += fs;
            }
            if self.cfg.show_accuracy {
                let fs = fs * self.cfg.text_sizes.accuracy;
                let label = s
                    .and_then(|v| v.accuracy)
                    .map(|v| format!("GPS ±{v:0.0} m"))
                    .unwrap_or("GPS ±—".into());
                self.text(p, &label, x, yy + fs * 0.5, fs * 0.5, false, &c.ring, size);
            }
        }
        Ok(())
    }
}
