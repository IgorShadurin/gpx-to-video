use super::*;
impl Renderer {
    pub(super) fn draw_gauge(&mut self, p: &mut Pixmap, s: Option<&Sample>) -> Result<()> {
        let c = self.cfg.colors.clone();
        if self.cfg.gauge.enabled {
            let (x, y, size) = self.pos(&self.cfg.gauge);
            let d = size.round() as u32;
            if self.gauge.is_none() {
                let mut g = Pixmap::new(d, d).unwrap();
                arc(
                    &mut g,
                    size * 0.5,
                    size * 0.5,
                    size * 0.455,
                    135.,
                    405.,
                    size * 0.055,
                    &c.shadow,
                    0.5,
                );
                arc(
                    &mut g,
                    size * 0.5,
                    size * 0.5,
                    size * 0.455,
                    135.,
                    405.,
                    size * 0.018,
                    &c.ring,
                    0.9,
                );
                for i in 0..27 {
                    let t = i as f32 / 26.;
                    let (a, b, k) = if t < 0.5 {
                        (&c.low, &c.mid, t * 2.)
                    } else {
                        (&c.mid, &c.high, (t - 0.5) * 2.)
                    };
                    let a = parse_color(a)?;
                    let b = parse_color(b)?;
                    let hex = format!(
                        "#{:02X}{:02X}{:02X}",
                        (a[0] as f32 * (1. - k) + b[0] as f32 * k) as u8,
                        (a[1] as f32 * (1. - k) + b[1] as f32 * k) as u8,
                        (a[2] as f32 * (1. - k) + b[2] as f32 * k) as u8
                    );
                    arc(
                        &mut g,
                        size * 0.5,
                        size * 0.5,
                        size * 0.398,
                        135. + i as f32 * 10.,
                        143. + i as f32 * 10.,
                        size * 0.03,
                        &hex,
                        1.,
                    );
                }
                self.gauge = Some(g)
            }
            p.draw_pixmap(
                x.round() as i32,
                y.round() as i32,
                self.gauge.as_ref().unwrap().as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
            let speed = s.and_then(|v| v.speed).map(|v| v * 3.6);
            if let Some(speed) = speed {
                let a = (135. + 270. * (speed / self.cfg.speed_max_kmh).clamp(0., 1.) as f32)
                    .to_radians();
                let (cx, cy) = (x + size * 0.5, y + size * 0.5);
                let point = |r: f32, side: f32| {
                    (
                        cx + size * r * a.cos() + size * side * (-a.sin()),
                        cy + size * r * a.sin() + size * side * a.cos(),
                    )
                };
                polygon(
                    p,
                    &[point(0.30, 0.), point(0.46, -0.032), point(0.46, 0.032)],
                    &c.text,
                    1.,
                );
            }
            let label = speed.map(|v| format!("{v:0.0}")).unwrap_or("—".into());
            let fs = size * 0.32;
            let tw = self.text_width(&label, fs, true);
            self.text(
                p,
                &label,
                x + (size - tw) / 2.,
                y + size * 0.62,
                fs,
                true,
                &c.text,
                size * 0.7,
            );
            let unit = if self.cfg.ru() { "км/ч" } else { "km/h" };
            self.text(
                p,
                unit,
                x + size * 0.40,
                y + size * 0.79,
                size * 0.075,
                false,
                &c.text,
                size * 0.35,
            );
            let title = if self.cfg.ru() { "ВЕЛО" } else { "RIDE" };
            let tw = self.text_width(title, size * 0.045, false);
            self.text(
                p,
                title,
                x + (size - tw) / 2.,
                y + size * 0.30,
                size * 0.045,
                false,
                &c.ring,
                size * 0.5,
            );
        }
        Ok(())
    }
}
