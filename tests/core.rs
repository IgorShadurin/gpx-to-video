use gpx_to_video::{
    config::Config,
    maps::{Maps, coordinate, short_name, tile, truncate, world},
    render::{Renderer, distance_text},
    telemetry::{Point, Track, angle, demo_track, timestamp},
    video::parse_probe,
};
use std::{fs, path::Path};
fn p(t: f64, lat: f64, lon: f64) -> Point {
    Point {
        time: t,
        lat,
        lon,
        speed: Some(5.),
        segment: 1,
        ..Default::default()
    }
}
#[test]
fn csv_and_namespaced_gpx_match_every_second() {
    let a = Track::load(Path::new("examples/synthetic-ride.gpx"), 5.).unwrap();
    let b = Track::load(Path::new("examples/synthetic-ride.csv"), 5.).unwrap();
    assert_eq!(a.points.len(), 121);
    for i in 0..=240 {
        let t = a.start() + i as f64 / 2.;
        let a = a.at(t).unwrap();
        let b = b.at(t).unwrap();
        assert_eq!(
            serde_json::to_value(a).unwrap(),
            serde_json::to_value(b).unwrap()
        );
    }
}
#[test]
fn time_offset_is_explicit() {
    assert_eq!(
        timestamp("2026-01-01T15:00:00+03:00").unwrap(),
        timestamp("2026-01-01T12:00:00Z").unwrap()
    );
    assert!(timestamp("2026-01-01T12:00:00").is_err())
}
#[test]
fn interpolate_but_do_not_cross_gaps() {
    let mut points = vec![
        p(0., 1., 1.),
        p(1., 1., 1.001),
        p(10., 1., 2.),
        p(11., 1., 2.001),
    ];
    points[2].segment = 2;
    points[3].segment = 2;
    let t = Track::new(points, 5.).unwrap();
    assert!(t.at(5.).is_none());
    assert!(t.at(-1.).is_none());
    assert!(t.at(12.).is_none());
    assert!(t.at(0.5).unwrap().interpolated);
    assert_eq!(t.at(10.).unwrap().segment, 2);
    assert!(t.total() < 250.);
}
#[test]
fn long_gps_outage_is_a_gap_even_in_same_segment() {
    let t = Track::new(vec![p(0., 0., 0.), p(10., 1., 1.)], 5.).unwrap();
    assert!(t.at(5.).is_none());
    assert_eq!(t.total(), 0.);
}
#[test]
fn invalid_speed_is_not_a_stop_or_estimate() {
    let mut a = p(0., 0., 0.);
    a.speed = None;
    a.speed_invalid = true;
    let t = Track::new(vec![a, p(1., 0., 0.001)], 5.).unwrap();
    assert!(t.at(0.).unwrap().speed.is_none());
    assert!(t.at(0.5).unwrap().speed.is_none());
    let mut a = p(0., 0., 0.);
    a.speed = Some(0.);
    let mut b = a.clone();
    b.time = 1.;
    let t = Track::new(vec![a, b], 5.).unwrap();
    assert_eq!(t.at(0.5).unwrap().speed, Some(0.));
}
#[test]
fn generic_gpx_can_derive_speed() {
    let (mut a, mut b) = (p(0., 0., 0.), p(2., 0., 0.001));
    a.speed = None;
    b.speed = None;
    let t = Track::new(vec![a, b], 5.).unwrap();
    let s = t.at(1.).unwrap();
    assert!(s.speed_estimated);
    assert!((s.speed.unwrap() - 55.5975).abs() < 0.01);
}
#[test]
fn longitudes_remain_negative_and_wrap_at_dateline() {
    let t = Track::new(vec![p(0., 0., -0.2), p(1., 0., -0.1)], 5.).unwrap();
    assert_eq!(t.at(0.).unwrap().lon, -0.2);
    assert!((t.at(0.5).unwrap().lon + 0.15).abs() < 1e-9);
    let t = Track::new(vec![p(0., 0., 179.9), p(1., 0., -179.9)], 5.).unwrap();
    assert!((t.at(0.5).unwrap().lon.abs() - 180.).abs() < 1e-9);
    assert!(angle(359., 1., 0.5) < 1e-9);
}
#[test]
fn reject_invalid_data() {
    assert!(Track::new(vec![p(0., 0., 0.), p(0., 0., 0.)], 5.).is_err());
    assert!(Track::new(vec![p(1., 0., 0.), p(0., 0., 0.)], 5.).is_err());
    assert!(Track::new(vec![p(0., 90., 0.), p(1., 0., 0.)], 5.).is_err());
    assert!(Track::new(vec![p(f64::NAN, 0., 0.), p(1., 0., 0.)], 5.).is_err());
}
#[test]
fn foreign_extension_does_not_override_speed() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("a.gpx");
    fs::write(&path,r#"<gpx xmlns="http://www.topografix.com/GPX/1/1" xmlns:bad="https://example.invalid"><trk><trkseg><trkpt lat="1" lon="1"><time>2026-01-01T00:00:00Z</time><extensions><bad:speed>100</bad:speed></extensions></trkpt><trkpt lat="1" lon="1"><time>2026-01-01T00:00:01Z</time></trkpt></trkseg></trk></gpx>"#).unwrap();
    let t = Track::load(&path, 5.).unwrap();
    assert!(t.points[0].speed.is_none());
}
#[test]
fn untimed_planned_route_rejected() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("a.gpx");
    fs::write(
        &path,
        r#"<gpx><trk><trkseg><trkpt lat="1" lon="1"></trkpt></trkseg></trk></gpx>"#,
    )
    .unwrap();
    assert!(Track::load(&path, 5.).is_err());
}
#[test]
fn russian_long_names_are_utf8_safe() {
    assert_eq!(short_name("улица Велосипедная", true), "ул. Велосипедная");
    let name = short_name(
        "посёлок Очень Длинное Название Населённого Пункта Со Множеством Слов",
        true,
    );
    assert!(name.starts_with("пос. "));
    assert!(name.ends_with('…'));
    assert!(name.chars().count() <= 38);
    assert_eq!(truncate("A👨‍👩‍👧‍👦BC", 3), "A👨‍👩‍👧‍👦…");
}
#[test]
fn distance_format_defaults_and_override() {
    let mut c = Config::default();
    assert_eq!(distance_text(13.19, 55.2, &c), "13.2 из 56 км");
    c.language = "en".into();
    c.total_decimals = 1;
    assert_eq!(distance_text(13.1, 55.2, &c), "13.1 of 55.2 km");
}
#[test]
fn config_roundtrips_and_rejects_typos() {
    let c = Config::default();
    let text = toml::to_string_pretty(&c).unwrap();
    let c: Config = toml::from_str(&text).unwrap();
    c.validate().unwrap();
    assert!(toml::from_str::<Config>("speeed_max = 80").is_err());
}
#[test]
fn projection_roundtrip() {
    for lat in [-60., 0., 51.53, 80.] {
        let (x, y) = world(lat, -0.15, 15);
        let (a, b) = coordinate(x, y, 15);
        assert!((a - lat).abs() < 1e-8);
        assert!((b + 0.15).abs() < 1e-8);
    }
}
#[test]
fn transparent_render_is_bounded_and_bilingual() {
    for lang in ["ru", "en"] {
        let c = Config {
            width: 1280,
            height: 720,
            map_source: "demo".into(),
            language: lang.into(),
            ..Default::default()
        };
        let t = demo_track();
        let mut r = Renderer::new(c).unwrap();
        let mut m = Maps::new(None, "demo", None).unwrap();
        let p = r
            .full_frame(&t, t.at(t.start() + 50.).as_ref(), &mut m)
            .unwrap();
        assert_eq!(&p.data()[0..4], &[0, 0, 0, 0]);
        let alpha: Vec<_> = p.data().as_chunks::<4>().0.iter().map(|p| p[3]).collect();
        assert!(alpha.contains(&255));
        assert!(alpha.iter().any(|a| *a > 0 && *a < 255));
        assert!(alpha.iter().filter(|a| **a == 0).count() > alpha.len() / 2);
    }
}
#[test]
fn map_cache_reuse_and_offline_missing_error() {
    let d = tempfile::tempdir().unwrap();
    let c = Config::default();
    let track = demo_track();
    let t = tile(track.points[0].lat, track.points[0].lon, c.map_zoom);
    for n in gpx_to_video::maps::neighbors(t) {
        let p = n.path(d.path());
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, r#"{"elements":[]}"#).unwrap();
    }
    let m = Maps::new(Some(d.path().into()), "osm", None).unwrap();
    assert_eq!(
        m.prepare(&track, &[(track.start(), track.start())], &c, true)
            .unwrap(),
        0
    );
    let empty = tempfile::tempdir().unwrap();
    let m = Maps::new(Some(empty.path().into()), "osm", None).unwrap();
    assert!(
        m.prepare(&track, &[(track.start(), track.start())], &c, true)
            .is_err()
    );
}
#[test]
fn video_time_is_not_filesystem_mtime() {
    let v=parse_probe(&serde_json::json!({"streams":[{"codec_type":"video","width":3840,"height":2160,"avg_frame_rate":"60000/1001","duration":"10","codec_name":"hevc","pix_fmt":"yuv420p10le"}],"format":{}})).unwrap();
    assert!(v.creation_time.is_none());
    assert_eq!(v.codec, "hevc");
    assert!((v.fps - 59.94005994).abs() < 1e-6);
}
#[test]
fn twenty_hour_lookup_is_indexed() {
    let points: Vec<_> = (0..72001)
        .map(|i| p(i as f64, 51. + i as f64 * 1e-7, 0.))
        .collect();
    let track = Track::new(points, 5.).unwrap();
    for i in (0..720000).step_by(17) {
        let s = track.at(i as f64 / 10.).unwrap();
        assert!((s.lat - (51. + i as f64 * 1e-8)).abs() < 1e-9);
    }
}

#[test]
fn nearest_road_and_settlement_labels_have_limits() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("map.json");
    fs::write(&path,r#"{"elements":[{"tags":{"highway":"residential","name:ru":"улица Велосипедная","name:en":"Bicycle Street"},"geometry":[{"lat":51.0,"lon":1.0},{"lat":51.0,"lon":1.01}]},{"tags":{"place":"village","name:ru":"Очень Длинное Название Деревни С Дополнительными Словами"},"lat":51.001,"lon":1.005}]}"#).unwrap();
    let t = Track::new(vec![p(0., 51., 1.005), p(1., 51., 1.006)], 5.).unwrap();
    let s = t.at(0.).unwrap();
    let c = Config::default();
    let mut m = Maps::new(None, "osm", Some(&path)).unwrap();
    assert_eq!(m.location(&s, &c).unwrap(), "ул. Велосипедная");
    let mut s = s.clone();
    s.lat = 51.001;
    let label = m.location(&s, &c).unwrap();
    assert!(label.starts_with("окр. д."));
    assert!(label.ends_with('…'));
    s.lat = 52.;
    assert_eq!(m.location(&s, &c).unwrap(), "");
    let c = Config {
        language: "en".into(),
        ..c
    };
    let mut s = s.clone();
    s.lat = 51.;
    assert_eq!(m.location(&s, &c).unwrap(), "Bicycle St");
}
#[test]
fn glyphs_cover_russian_and_latin_labels() {
    let noto = fontdue::Font::from_bytes(
        include_bytes!("../assets/fonts/NotoSans.ttf") as &[u8],
        fontdue::FontSettings::default(),
    )
    .unwrap();
    for ch in "изкмчВЫСОТАНЕТGPS0123456789°±—…".chars() {
        assert_ne!(noto.lookup_glyph_index(ch), 0, "{ch}");
    }
}
