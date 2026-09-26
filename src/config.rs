use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub theme: String,
    pub language: String,
    pub overlay_fps: f64,
    pub max_gap_seconds: f64,
    pub width: u32,
    pub height: u32,
    pub speed_max_kmh: f64,
    pub distance_decimals: usize,
    pub total_decimals: usize,
    pub total_round_up: bool,
    pub total_km: Option<f64>,
    pub gauge: Widget,
    pub map: Widget,
    pub stats: Widget,
    pub show_coordinates: bool,
    pub show_elevation: bool,
    pub show_distance: bool,
    pub show_location: bool,
    pub show_accuracy: bool,
    pub map_zoom: u8,
    pub map_tile_pixels: u32,
    pub map_source: String,
    pub max_download_tiles: usize,
    pub overpass_url: String,
    pub road_snap_m: f64,
    pub place_radius_m: f64,
    pub colors: Colors,
    pub encoding: Encoding,
    pub text_sizes: TextSizes,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TextSizes {
    pub location: f32,
    pub distance: f32,
    pub elevation: f32,
    pub coordinates: f32,
    pub accuracy: f32,
}
impl Default for TextSizes {
    fn default() -> Self {
        Self {
            location: 1.,
            distance: 1.,
            elevation: 1.,
            coordinates: 1.,
            accuracy: 1.,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Encoding {
    pub bitrate_scale: f64,
    pub software_crf: u8,
    pub software_preset: String,
    pub webm_crf: u8,
    pub webm_cpu_used: u8,
    pub filter_threads: u8,
}
impl Default for Encoding {
    fn default() -> Self {
        Self {
            bitrate_scale: 1.0,
            software_crf: 20,
            software_preset: "fast".into(),
            webm_crf: 30,
            webm_cpu_used: 5,
            filter_threads: 2,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Widget {
    pub enabled: bool,
    pub x: f32,
    pub y: f32,
    pub size: f32,
}
impl Default for Widget {
    fn default() -> Self {
        Self {
            enabled: true,
            x: 0.04,
            y: 0.66,
            size: 0.18,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Colors {
    pub text: String,
    pub shadow: String,
    pub accent: String,
    pub ring: String,
    pub low: String,
    pub mid: String,
    pub high: String,
    pub land: String,
    pub park: String,
    pub water: String,
    pub building: String,
    pub road: String,
    pub road_edge: String,
    pub route: String,
}
impl Default for Colors {
    fn default() -> Self {
        Self {
            text: "#FFFFFF".into(),
            shadow: "#091719".into(),
            accent: "#D5F65B".into(),
            ring: "#EDF3E9".into(),
            low: "#28D7A1".into(),
            mid: "#F7C548".into(),
            high: "#F24C45".into(),
            land: "#304544".into(),
            park: "#3C5650".into(),
            water: "#6B9FAD".into(),
            building: "#506662".into(),
            road: "#DBE4DD".into(),
            road_edge: "#1D302F".into(),
            route: "#D5F65B".into(),
        }
    }
}
impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "bicycle".into(),
            language: "ru".into(),
            overlay_fps: 15.,
            max_gap_seconds: 5.,
            width: 3840,
            height: 2160,
            speed_max_kmh: 60.,
            distance_decimals: 1,
            total_decimals: 0,
            total_round_up: true,
            total_km: None,
            gauge: Widget {
                enabled: true,
                x: 0.035,
                y: 0.65,
                size: 0.18,
            },
            map: Widget {
                enabled: true,
                x: 0.805,
                y: 0.66,
                size: 0.16,
            },
            stats: Widget {
                enabled: true,
                x: 0.255,
                y: 0.72,
                size: 0.32,
            },
            show_coordinates: true,
            show_elevation: true,
            show_distance: true,
            show_location: true,
            show_accuracy: false,
            map_zoom: 15,
            map_tile_pixels: 512,
            map_source: "osm".into(),
            max_download_tiles: 128,
            overpass_url: "https://overpass-api.de/api/interpreter".into(),
            road_snap_m: 30.,
            place_radius_m: 3000.,
            colors: Colors::default(),
            encoding: Encoding::default(),
            text_sizes: TextSizes::default(),
        }
    }
}
impl Config {
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let c: Self = if let Some(p) = path {
            toml::from_str(&fs::read_to_string(p)?)?
        } else {
            Self::default()
        };
        c.validate()?;
        Ok(c)
    }
    pub fn validate(&self) -> Result<()> {
        for scale in [
            self.text_sizes.location,
            self.text_sizes.distance,
            self.text_sizes.elevation,
            self.text_sizes.coordinates,
            self.text_sizes.accuracy,
        ] {
            if !scale.is_finite() || !(0.5..=2.).contains(&scale) {
                bail!("Text size multipliers must be between 0.5 and 2")
            }
        }
        let e = &self.encoding;
        if !e.bitrate_scale.is_finite()
            || !(0.25..=4.).contains(&e.bitrate_scale)
            || e.software_crf > 51
            || e.webm_crf > 63
            || e.webm_cpu_used > 8
            || !(1..=32).contains(&e.filter_threads)
            || ![
                "ultrafast",
                "superfast",
                "veryfast",
                "faster",
                "fast",
                "medium",
                "slow",
                "slower",
                "veryslow",
            ]
            .contains(&e.software_preset.as_str())
        {
            bail!("Invalid encoding settings")
        }
        if self.theme != "bicycle" {
            bail!("Unknown theme: {}; supported: bicycle", self.theme)
        }
        if !["ru", "en"].contains(&self.language.as_str()) {
            bail!("language must be ru or en")
        }
        if !["osm", "demo", "none"].contains(&self.map_source.as_str()) {
            bail!("map_source must be osm, demo or none")
        }
        if !(256..=8192).contains(&self.width)
            || !(144..=8192).contains(&self.height)
            || self.width % 2 != 0
            || self.height % 2 != 0
        {
            bail!("Use even canvas dimensions within 256×144 and 8192×8192")
        }
        if !self.overlay_fps.is_finite()
            || !(1. ..=60.).contains(&self.overlay_fps)
            || !self.max_gap_seconds.is_finite()
            || !(0.01..=60.).contains(&self.max_gap_seconds)
        {
            bail!("Invalid fps or maximum gap")
        }
        if !self.speed_max_kmh.is_finite()
            || self.speed_max_kmh <= 0.
            || self.speed_max_kmh > 500.
            || self.distance_decimals > 3
            || self.total_decimals > 3
        {
            bail!("Invalid speed scale or distance precision")
        }
        if self.total_km.is_some_and(|v| !v.is_finite() || v <= 0.) {
            bail!("total_km must be positive")
        }
        if !(10..=18).contains(&self.map_zoom)
            || ![256, 512, 1024].contains(&self.map_tile_pixels)
            || self.max_download_tiles > 2000
        {
            bail!("Invalid map zoom, raster size or download cap")
        }
        if !self.road_snap_m.is_finite()
            || !(0. ..=200.).contains(&self.road_snap_m)
            || !self.place_radius_m.is_finite()
            || !(0. ..=20000.).contains(&self.place_radius_m)
        {
            bail!("Invalid location matching radius")
        }
        for w in [&self.gauge, &self.map, &self.stats] {
            if !w.x.is_finite()
                || !w.y.is_finite()
                || !w.size.is_finite()
                || w.x < 0.
                || w.y < 0.
                || w.size < 0.04
                || w.x + w.size > 1.
            {
                bail!("Widget position/size is outside the canvas")
            }
        }
        for c in [
            &self.colors.text,
            &self.colors.shadow,
            &self.colors.accent,
            &self.colors.ring,
            &self.colors.low,
            &self.colors.mid,
            &self.colors.high,
            &self.colors.land,
            &self.colors.park,
            &self.colors.water,
            &self.colors.building,
            &self.colors.road,
            &self.colors.road_edge,
            &self.colors.route,
        ] {
            parse_color(c)?;
        }
        Ok(())
    }
    pub fn stats_height_factor(&self) -> f32 {
        let t = &self.text_sizes;
        let mut height = 0.02;
        for (show, scale, advance) in [
            (self.show_location, t.location, 1.65),
            (self.show_distance, t.distance, 2.4),
            (self.show_elevation, t.elevation, 1.1),
            (self.show_coordinates, t.coordinates, 1.),
            (self.show_accuracy, t.accuracy, 0.8),
        ] {
            if show {
                height += 0.055 * scale * advance;
            }
        }
        height
    }
    pub fn ru(&self) -> bool {
        self.language == "ru"
    }
}
pub fn parse_color(s: &str) -> Result<[u8; 3]> {
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() != 6 || !s.is_ascii() {
        bail!("Color must be #RRGGBB")
    };
    Ok([
        u8::from_str_radix(&s[0..2], 16)?,
        u8::from_str_radix(&s[2..4], 16)?,
        u8::from_str_radix(&s[4..6], 16)?,
    ])
}
