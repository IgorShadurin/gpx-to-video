use crate::{
    config::Config,
    telemetry::{Sample, Track, distance},
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Tile {
    pub z: u8,
    pub x: u32,
    pub y: u32,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Geo {
    pub lat: f64,
    pub lon: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Feature {
    #[serde(default)]
    pub tags: BTreeMap<String, String>,
    #[serde(default)]
    pub geometry: Vec<Geo>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Data {
    #[serde(default)]
    pub elements: Vec<Feature>,
}
pub fn world(lat: f64, lon: f64, z: u8) -> (f64, f64) {
    let n = 2f64.powi(z as i32);
    let p = lat.clamp(-85.05112878, 85.05112878).to_radians();
    (
        (lon + 180.) / 360. * n,
        (1. - (p.tan() + 1. / p.cos()).ln() / std::f64::consts::PI) / 2. * n,
    )
}
pub fn coordinate(x: f64, y: f64, z: u8) -> (f64, f64) {
    let n = 2f64.powi(z as i32);
    (
        (std::f64::consts::PI * (1. - 2. * y / n))
            .sinh()
            .atan()
            .to_degrees(),
        x / n * 360. - 180.,
    )
}
pub fn tile(lat: f64, lon: f64, z: u8) -> Tile {
    let (x, y) = world(lat, lon, z);
    let n = 1u32 << z;
    Tile {
        z,
        x: (x.floor() as i64).rem_euclid(n as i64) as u32,
        y: (y.floor() as u32).min(n - 1),
    }
}
pub fn neighbors(t: Tile) -> Vec<Tile> {
    let n = 1i64 << t.z;
    let mut v = vec![];
    for dy in -1..=1 {
        for dx in -1..=1 {
            let y = t.y as i64 + dy;
            if y >= 0 && y < n {
                v.push(Tile {
                    z: t.z,
                    x: (t.x as i64 + dx).rem_euclid(n) as u32,
                    y: y as u32,
                });
            }
        }
    }
    v
}
impl Tile {
    pub fn path(self, root: &Path) -> PathBuf {
        root.join("osm-v1")
            .join(self.z.to_string())
            .join(self.x.to_string())
            .join(format!("{}.json", self.y))
    }
}
pub struct Maps {
    pub root: PathBuf,
    pub source: String,
    cache: HashMap<Tile, Data>,
    order: VecDeque<Tile>,
    pub imported: Option<Data>,
}
impl Maps {
    pub fn new(root: Option<PathBuf>, source: &str, import: Option<&Path>) -> Result<Self> {
        let root = root.unwrap_or_else(|| {
            dirs::cache_dir()
                .unwrap_or_else(|| PathBuf::from(".cache"))
                .join("gpx-to-video")
        });
        Ok(Self {
            root,
            source: source.into(),
            cache: HashMap::new(),
            order: VecDeque::new(),
            imported: import
                .map(|p| -> Result<Data> { Ok(serde_json::from_slice(&fs::read(p)?)?) })
                .transpose()?,
        })
    }
    pub fn prepare(
        &self,
        track: &Track,
        ranges: &[(f64, f64)],
        cfg: &Config,
        offline: bool,
    ) -> Result<usize> {
        if self.source != "osm" || self.imported.is_some() || !cfg.map.enabled {
            return Ok(0);
        }
        let mut needed = BTreeSet::new();
        for p in &track.points {
            if ranges.iter().any(|(a, b)| {
                p.time >= *a - cfg.max_gap_seconds && p.time <= *b + cfg.max_gap_seconds
            }) {
                needed.extend(neighbors(tile(p.lat, p.lon, cfg.map_zoom)));
            }
        }
        if needed.is_empty() {
            return Ok(0);
        }
        let missing: Vec<_> = needed
            .iter()
            .filter(|t| !t.path(&self.root).exists())
            .copied()
            .collect();
        if missing.len() > cfg.max_download_tiles {
            bail!(
                "{} map tiles missing, limit is {}. Increase max_download_tiles explicitly, use --osm-data for a regional Overpass JSON export, or disable map. Cache is {}",
                missing.len(),
                cfg.max_download_tiles,
                self.root.display()
            )
        }
        if offline && !missing.is_empty() {
            bail!(
                "Offline: {} required OSM tiles are missing from {}",
                missing.len(),
                self.root.display()
            )
        }
        let client = reqwest::blocking::Client::builder()
            .user_agent("gpx-to-video/0.1 (https://github.com/IgorShadurin/gpx-to-video)")
            .timeout(Duration::from_secs(45))
            .build()?;
        for (i, t) in missing.iter().enumerate() {
            if i > 0 {
                std::thread::sleep(Duration::from_secs(1))
            }
            let (s, w) = coordinate(t.x as f64, t.y as f64 + 1., t.z);
            let (n, e) = coordinate(t.x as f64 + 1., t.y as f64, t.z);
            let bbox = format!("{s},{w},{n},{e}");
            let query = format!(
                "[out:json][timeout:25];(way[highway]({bbox});way[building]({bbox});way[landuse]({bbox});way[natural=water]({bbox});way[waterway]({bbox});way[leisure=park]({bbox});node[place]({bbox}););out tags geom;"
            );
            eprintln!("OSM {}/{}: {}/{}/{}", i + 1, missing.len(), t.z, t.x, t.y);
            let mut response = None;
            for attempt in 0..3 {
                let result = client
                    .get(&cfg.overpass_url)
                    .query(&[("data", &query)])
                    .send()
                    .and_then(|r| r.error_for_status())
                    .and_then(|r| r.bytes());
                match result {
                    Ok(bytes) => { response = Some(bytes); break; }
                    Err(e) if attempt < 2 => { eprintln!("OSM request failed ({e}); retrying"); std::thread::sleep(Duration::from_secs(2 << attempt)); }
                    Err(e) => return Err(e).context("OSM download failed; completed tiles remain cached. Retry later or configure overpass_url / --osm-data"),
                }
            }
            let bytes = response.context("No OSM response")?;
            let value: serde_json::Value = serde_json::from_slice(&bytes)?;
            if value.get("remark").is_some() {
                bail!("Overpass returned incomplete data: {}", value["remark"])
            }
            let _: Data = serde_json::from_slice(&bytes)?;
            let path = t.path(&self.root);
            fs::create_dir_all(path.parent().unwrap())?;
            let temporary = path.with_extension("json.part");
            fs::write(&temporary, &bytes)?;
            fs::rename(temporary, path)?;
        }
        eprintln!(
            "OSM: {} cached tiles, {} downloaded",
            needed.len() - missing.len(),
            missing.len()
        );
        Ok(missing.len())
    }
    pub fn data(&mut self, t: Tile) -> Result<&Data> {
        if let Some(ref d) = self.imported {
            return Ok(d);
        }
        if !self.cache.contains_key(&t) {
            let d = if self.source == "demo" {
                demo(t)
            } else if self.source == "none" {
                Data::default()
            } else {
                let p = t.path(&self.root);
                serde_json::from_slice(&fs::read(&p).with_context(|| {
                    format!(
                        "Missing cached map {}; run without --offline first",
                        p.display()
                    )
                })?)?
            };
            while self.cache.len() >= 18 {
                if let Some(old) = self.order.pop_front() {
                    self.cache.remove(&old);
                }
            }
            self.cache.insert(t, d);
        }
        self.order.retain(|x| *x != t);
        self.order.push_back(t);
        Ok(self.cache.get(&t).unwrap())
    }
    pub fn location(&mut self, s: &Sample, cfg: &Config) -> Result<String> {
        if self.source == "none" {
            return Ok(String::new());
        }
        if self.source == "demo" {
            return Ok(if cfg.ru() {
                "наб. Велосипедная"
            } else {
                "Riverside Cycleway"
            }
            .into());
        }
        let mut road = (f64::MAX, String::new());
        let mut place = (f64::MAX, String::new());
        for t in neighbors(tile(s.lat, s.lon, cfg.map_zoom)) {
            for f in &self.data(t)?.elements {
                let name = localized_name(&f.tags, &cfg.language);
                if name.is_empty() {
                    continue;
                }
                if f.tags.contains_key("highway") {
                    for pair in f.geometry.windows(2) {
                        let d = segment_distance(s, &pair[0], &pair[1]);
                        if d < road.0 {
                            road = (d, name.clone())
                        }
                    }
                } else if f.tags.get("place").is_some_and(|p| {
                    [
                        "city",
                        "town",
                        "village",
                        "hamlet",
                        "suburb",
                        "neighbourhood",
                    ]
                    .contains(&p.as_str())
                }) && let (Some(lat), Some(lon)) = (f.lat, f.lon)
                {
                    let d = distance((lat, lon), (s.lat, s.lon));
                    if d < place.0 {
                        let kind = f.tags.get("place").unwrap().as_str();
                        let prefix = match (kind, cfg.ru()) {
                            ("city", true) | ("town", true) => "г. ",
                            ("village", true) => "д. ",
                            ("hamlet", true) => "пос. ",
                            _ => "",
                        };
                        place = (d, format!("{prefix}{name}"));
                    }
                }
            }
        }
        Ok(if road.0 <= cfg.road_snap_m {
            short_name(&road.1, cfg.ru())
        } else if place.0 <= cfg.place_radius_m {
            format!(
                "{} {}",
                if cfg.ru() { "окр." } else { "near" },
                short_name(&place.1, cfg.ru())
            )
        } else {
            String::new()
        })
    }
}
pub fn localized_name(tags: &BTreeMap<String, String>, lang: &str) -> String {
    tags.get(&format!("name:{lang}"))
        .or_else(|| tags.get("name"))
        .or_else(|| tags.get("ref"))
        .cloned()
        .unwrap_or_default()
}
pub fn short_name(name: &str, ru: bool) -> String {
    let mut s = name.to_string();
    let pairs = if ru {
        vec![
            ("улица ", "ул. "),
            ("Улица ", "ул. "),
            ("проспект ", "пр-т "),
            ("набережная ", "наб. "),
            ("бульвар ", "бул. "),
            ("посёлок ", "пос. "),
            ("поселок ", "пос. "),
            ("деревня ", "д. "),
        ]
    } else {
        vec![
            (" Street", " St"),
            (" Avenue", " Ave"),
            (" Boulevard", " Blvd"),
            (" Road", " Rd"),
        ]
    };
    for (a, b) in pairs {
        s = s.replace(a, b)
    }
    truncate(&s, 38)
}
pub fn truncate(s: &str, n: usize) -> String {
    let g: Vec<_> = s.graphemes(true).collect();
    if g.len() <= n {
        s.into()
    } else {
        format!("{}…", g[..n.saturating_sub(1)].concat())
    }
}
fn segment_distance(s: &Sample, a: &Geo, b: &Geo) -> f64 {
    let k = 111195.;
    let c = s.lat.to_radians().cos();
    let ax = (a.lon - s.lon) * k * c;
    let ay = (a.lat - s.lat) * k;
    let bx = (b.lon - s.lon) * k * c;
    let by = (b.lat - s.lat) * k;
    let dx = bx - ax;
    let dy = by - ay;
    let t = (-(ax * dx + ay * dy) / (dx * dx + dy * dy).max(1e-9)).clamp(0., 1.);
    (ax + t * dx).hypot(ay + t * dy)
}
fn demo(t: Tile) -> Data {
    let mut elements = vec![];
    for i in 0..7 {
        let x = t.x as f64 + (i as f64 + 0.5) / 7.;
        let y = t.y as f64 + (i as f64 + 0.5) / 7.;
        for vertical in [true, false] {
            let (a, b) = if vertical {
                (
                    coordinate(x, t.y as f64, t.z),
                    coordinate(x, t.y as f64 + 1., t.z),
                )
            } else {
                (
                    coordinate(t.x as f64, y, t.z),
                    coordinate(t.x as f64 + 1., y, t.z),
                )
            };
            elements.push(Feature {
                tags: BTreeMap::from([("highway".into(), "residential".into())]),
                geometry: vec![Geo { lat: a.0, lon: a.1 }, Geo { lat: b.0, lon: b.1 }],
                lat: None,
                lon: None,
            });
        }
    }
    let mut poly = |key: &str, val: &str, xy: &[(f64, f64)]| {
        elements.push(Feature {
            tags: BTreeMap::from([(key.into(), val.into())]),
            geometry: xy
                .iter()
                .map(|(x, y)| {
                    let (a, b) = coordinate(t.x as f64 + x, t.y as f64 + y, t.z);
                    Geo { lat: a, lon: b }
                })
                .collect(),
            lat: None,
            lon: None,
        });
    };
    poly(
        "natural",
        "water",
        &[(0.7, 0.), (0.9, 0.), (0.6, 1.), (0.45, 1.), (0.7, 0.)],
    );
    poly(
        "leisure",
        "park",
        &[(0.1, 0.1), (0.4, 0.1), (0.4, 0.35), (0.1, 0.35), (0.1, 0.1)],
    );
    Data { elements }
}
