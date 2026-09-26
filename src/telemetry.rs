use anyhow::{Context, Result, bail};
use chrono::DateTime;
use quick_xml::{events::Event, name::ResolveResult, reader::NsReader};
use serde::Serialize;
use std::{fs::File, io::BufReader, path::Path};

#[derive(Clone, Debug, Default)]
pub struct Point {
    pub time: f64,
    pub lat: f64,
    pub lon: f64,
    pub speed: Option<f64>,
    pub elevation: Option<f64>,
    pub accuracy: Option<f64>,
    pub course: Option<f64>,
    pub segment: u32,
    pub distance: f64,
    pub speed_invalid: bool,
}
#[derive(Clone, Debug)]
pub struct Track {
    pub points: Vec<Point>,
    pub max_gap: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Sample {
    pub time: f64,
    pub lat: f64,
    pub lon: f64,
    pub speed: Option<f64>,
    pub elevation: Option<f64>,
    pub accuracy: Option<f64>,
    pub course: f64,
    pub distance: f64,
    pub segment: u32,
    pub interpolated: bool,
    pub speed_estimated: bool,
}
pub fn timestamp(s: &str) -> Result<f64> {
    let d = DateTime::parse_from_rfc3339(s.trim())
        .with_context(|| format!("Timestamp needs an explicit UTC offset (Z or ±HH:MM): {s}"))?;
    Ok(d.timestamp_millis() as f64 / 1000.)
}
pub fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (p1, p2) = (a.0.to_radians(), b.0.to_radians());
    let d = (b.0 - a.0).to_radians();
    let l = (b.1 - a.1).to_radians();
    let h = (d / 2.).sin().powi(2) + p1.cos() * p2.cos() * (l / 2.).sin().powi(2);
    6371008.8 * 2. * h.sqrt().min(1.).asin()
}
pub fn angle(a: f64, b: f64, t: f64) -> f64 {
    (a + ((b - a + 540.).rem_euclid(360.) - 180.) * t).rem_euclid(360.)
}
pub fn bearing(a: &Point, b: &Point) -> f64 {
    let l = (b.lon - a.lon).to_radians();
    let a = a.lat.to_radians();
    let b = b.lat.to_radians();
    (l.sin() * b.cos())
        .atan2(a.cos() * b.sin() - a.sin() * b.cos() * l.cos())
        .to_degrees()
        .rem_euclid(360.)
}
fn number(s: &str) -> Result<Option<f64>> {
    if s.trim().is_empty() {
        return Ok(None);
    }
    let n: f64 = s.trim().parse()?;
    if !n.is_finite() {
        bail!("Non-finite telemetry value")
    };
    Ok(Some(n))
}
impl Track {
    pub fn load(path: &Path, max_gap: f64) -> Result<Self> {
        let p = match path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase()
            .as_str()
        {
            "gpx" => read_gpx(path)?,
            "csv" => read_csv(path)?,
            _ => bail!("Expected a .gpx or .csv track"),
        };
        Self::new(p, max_gap)
    }
    pub fn new(mut points: Vec<Point>, max_gap: f64) -> Result<Self> {
        if points.len() < 2 {
            bail!("Track needs at least two timed points")
        };
        let mut total = 0.;
        for i in 0..points.len() {
            let p = &points[i];
            if !p.time.is_finite()
                || !p.lat.is_finite()
                || !p.lon.is_finite()
                || p.lat.abs() > 85.05112878
                || p.lon.abs() > 180.
            {
                bail!("Invalid time or coordinate at point {i}")
            };
            if p.speed.is_some_and(|x| !x.is_finite() || x < 0.) {
                bail!("Invalid speed at point {i}")
            };
            if i > 0 {
                let a = &points[i - 1];
                if p.time <= a.time {
                    bail!(
                        "Timestamps must increase strictly (duplicate/reversed point {i}); do not sort across recording gaps"
                    )
                };
                if p.segment == a.segment && p.time - a.time <= max_gap {
                    total += distance((a.lat, a.lon), (p.lat, p.lon));
                }
            }
            points[i].distance = total;
        }
        Ok(Self { points, max_gap })
    }
    pub fn start(&self) -> f64 {
        self.points[0].time
    }
    pub fn end(&self) -> f64 {
        self.points.last().unwrap().time
    }
    pub fn total(&self) -> f64 {
        self.points.last().unwrap().distance
    }
    pub fn at(&self, t: f64) -> Option<Sample> {
        if !t.is_finite() || t < self.start() || t > self.end() {
            return None;
        }
        let i = self.points.partition_point(|p| p.time < t);
        let b = &self.points[i.min(self.points.len() - 1)];
        let a = if i == 0 { b } else { &self.points[i - 1] };
        let exact = (b.time - t).abs() < 0.000001;
        let p = if exact { b } else { a };
        if !exact && (a.segment != b.segment || b.time - a.time > self.max_gap) {
            return None;
        }
        let k = if exact || a.time == b.time {
            1.
        } else {
            (t - a.time) / (b.time - a.time)
        };
        let lerp = |x: f64, y: f64| x + (y - x) * k;
        let opt = |x: Option<f64>, y: Option<f64>| {
            if exact {
                y
            } else {
                x.zip(y).map(|(a, b)| lerp(a, b))
            }
        };
        let mut speed = opt(a.speed, b.speed);
        let mut estimated = false;
        // Derive only when a generic GPX omits speed, never override explicit invalid measurements.
        if speed.is_none()
            && !a.speed_invalid
            && !b.speed_invalid
            && a.speed.is_none()
            && b.speed.is_none()
            && a.segment == b.segment
            && b.time > a.time
            && b.time - a.time <= self.max_gap
        {
            speed = Some(distance((a.lat, a.lon), (b.lat, b.lon)) / (b.time - a.time));
            estimated = true;
        }
        let course = opt(a.course, b.course)
            .map(|_| {
                if exact {
                    b.course.unwrap()
                } else {
                    angle(a.course.unwrap(), b.course.unwrap(), k)
                }
            })
            .unwrap_or_else(|| bearing(a, b));
        Some(Sample {
            time: t,
            lat: if exact { b.lat } else { lerp(a.lat, b.lat) },
            lon: if exact {
                b.lon
            } else {
                (angle(a.lon, b.lon, k) + 180.).rem_euclid(360.) - 180.
            },
            speed,
            elevation: opt(a.elevation, b.elevation),
            accuracy: opt(a.accuracy, b.accuracy),
            course,
            distance: if exact {
                b.distance
            } else {
                lerp(a.distance, b.distance)
            },
            segment: p.segment,
            interpolated: !exact,
            speed_estimated: estimated,
        })
    }
}
fn read_csv(path: &Path) -> Result<Vec<Point>> {
    let mut r = csv::Reader::from_path(path)?;
    let h = r.headers()?.clone();
    for key in ["timestamp", "latitude", "longitude"] {
        if !h.iter().any(|x| x == key) {
            bail!("CSV missing required column {key}")
        }
    }
    let mut out = Vec::new();
    for row in r.records() {
        let row = row?;
        let get = |key: &str| {
            h.iter()
                .position(|x| x == key)
                .and_then(|i| row.get(i))
                .unwrap_or("")
        };
        let valid = get("speed_valid");
        if !["", "true", "false"].contains(&valid) {
            bail!("Invalid speed_valid")
        };
        out.push(Point {
            time: timestamp(get("timestamp"))?,
            lat: number(get("latitude"))?.context("Missing latitude")?,
            lon: number(get("longitude"))?.context("Missing longitude")?,
            speed: if valid == "false" {
                None
            } else {
                number(get("speed_m_s"))?
            },
            elevation: number(get("altitude_m"))?,
            accuracy: number(get("horizontal_accuracy_m"))?,
            course: number(get("course_deg"))?,
            segment: if get("segment").is_empty() {
                1
            } else {
                get("segment").parse().context("Invalid CSV segment")?
            },
            speed_invalid: valid == "false",
            ..Default::default()
        });
    }
    Ok(out)
}
fn read_gpx(path: &Path) -> Result<Vec<Point>> {
    let mut r = NsReader::from_reader(BufReader::new(File::open(path)?));
    r.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let (mut out, mut p, mut field, mut segment) = (Vec::new(), None::<Point>, String::new(), 0);
    let mut has_time = false;
    loop {
        let (ns, e) = r.read_resolved_event_into(&mut buf)?;
        let namespace = match ns {
            ResolveResult::Bound(n) => String::from_utf8_lossy(n.as_ref()).to_string(),
            _ => String::new(),
        };
        let standard = namespace.is_empty()
            || namespace == "http://www.topografix.com/GPX/1/1"
            || namespace == "http://www.topografix.com/GPX/1/0";
        let ours = namespace == "https://yumcut.com/mobile/speedometer-gps";
        match e {
            Event::Start(e) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                field = if standard || ours {
                    name.clone()
                } else {
                    String::new()
                };
                if standard && name == "trkseg" {
                    segment += 1
                }
                if standard && name == "trkpt" {
                    let mut q = Point {
                        segment,
                        ..Default::default()
                    };
                    let (mut lat, mut lon) = (None, None);
                    for attr in e.attributes() {
                        let a = attr?;
                        let s = a.unescape_value()?;
                        match a.key.as_ref() {
                            b"lat" => lat = number(&s)?,
                            b"lon" => lon = number(&s)?,
                            _ => {}
                        }
                    }
                    q.lat = lat.context("GPX point missing lat")?;
                    q.lon = lon.context("GPX point missing lon")?;
                    p = Some(q);
                    has_time = false;
                }
            }
            Event::Text(e) => {
                if let Some(q) = p.as_mut() {
                    let s = e.decode()?;
                    match field.as_str() {
                        "time" => {
                            q.time = timestamp(&s)?;
                            has_time = true
                        }
                        "ele" => q.elevation = number(&s)?,
                        "speed" => q.speed = number(&s)?,
                        "speedValid" => {
                            q.speed_invalid = s == "false";
                            if !["true", "false"].contains(&s.as_ref()) {
                                bail!("Invalid GPX speedValid")
                            }
                        }
                        "horizontalAccuracy" => q.accuracy = number(&s)?,
                        "course" => q.course = number(&s)?,
                        _ => {}
                    }
                }
            }
            Event::End(e) => {
                if standard && e.local_name().as_ref() == b"trkpt" {
                    if !has_time {
                        bail!(
                            "GPX track point has no time; a planned route cannot be synchronized to video"
                        )
                    }
                    if let Some(mut q) = p.take() {
                        if q.speed_invalid {
                            q.speed = None;
                        }
                        out.push(q)
                    }
                }
                field.clear()
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(out)
}

pub fn demo_track() -> Track {
    let start = timestamp("2026-01-01T12:00:00Z").unwrap();
    Track::new(
        (0..=600)
            .map(|i| {
                let a = i as f64 / 400. * std::f64::consts::TAU;
                Point {
                    time: start + i as f64,
                    lat: 51.532 + a.sin() * 0.003,
                    lon: -0.155 + a.cos() * 0.005,
                    speed: Some(6.0 + 2. * (a * 0.7).sin()),
                    elevation: Some(38. + a.sin() * 5.),
                    accuracy: Some(4.),
                    segment: 1,
                    ..Default::default()
                }
            })
            .collect(),
        5.,
    )
    .unwrap()
}
