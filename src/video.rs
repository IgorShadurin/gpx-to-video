use crate::{
    config::Config,
    maps::Maps,
    render::Renderer,
    telemetry::{Track, timestamp},
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};

#[derive(Clone, Debug, Serialize)]
pub struct Video {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub frame_rate: String,
    pub nominal_frame_rate: String,
    pub duration: f64,
    pub codec: String,
    pub pixel_format: String,
    pub creation_time: Option<String>,
    pub transfer: String,
    pub primaries: String,
    pub space: String,
    pub bit_rate: u64,
    pub rotation: i64,
    pub time_base: String,
    pub start_time: f64,
}
pub fn probe(path: &Path) -> Result<Video> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .context("Install FFmpeg (ffprobe is required)")?;
    if !output.status.success() {
        bail!("ffprobe failed for {}", path.display())
    }
    parse_probe(&serde_json::from_slice(&output.stdout)?)
}
pub fn parse_probe(v: &Value) -> Result<Video> {
    let s = v["streams"]
        .as_array()
        .context("No video streams")?
        .iter()
        .find(|s| {
            s["codec_type"] == "video"
                && s["disposition"]["attached_pic"].as_i64().unwrap_or(0) == 0
        })
        .context("No video stream")?;
    let string = |key: &str| s[key].as_str().unwrap_or("").to_string();
    let num = |key: &str| s[key].as_str().and_then(|s| s.parse::<f64>().ok());
    let rate = string("avg_frame_rate");
    let fps = rate
        .split_once('/')
        .and_then(|(a, b)| Some(a.parse::<f64>().ok()? / b.parse::<f64>().ok()?))
        .unwrap_or(0.);
    let duration = num("duration")
        .or_else(|| {
            v["format"]["duration"]
                .as_str()
                .and_then(|x| x.parse().ok())
        })
        .context("Missing video duration")?;
    let creation_time = s["tags"]["creation_time"]
        .as_str()
        .or_else(|| v["format"]["tags"]["com.apple.quicktime.creationdate"].as_str())
        .or_else(|| v["format"]["tags"]["creation_time"].as_str())
        .map(String::from);
    let rotation = s["side_data_list"]
        .as_array()
        .and_then(|arr| arr.iter().find_map(|x| x["rotation"].as_i64()))
        .unwrap_or(0);
    let video = Video {
        width: s["width"].as_u64().unwrap_or(0) as u32,
        height: s["height"].as_u64().unwrap_or(0) as u32,
        fps,
        frame_rate: rate,
        nominal_frame_rate: string("r_frame_rate"),
        duration,
        codec: string("codec_name"),
        pixel_format: string("pix_fmt"),
        creation_time,
        transfer: string("color_transfer"),
        primaries: string("color_primaries"),
        space: string("color_space"),
        bit_rate: num("bit_rate").unwrap_or(0.) as u64,
        rotation,
        time_base: string("time_base"),
        start_time: num("start_time").unwrap_or(0.),
    };
    if !fps.is_finite()
        || fps <= 0.
        || !duration.is_finite()
        || duration <= 0.
        || video.width == 0
        || video.height == 0
    {
        bail!("Invalid video timing or dimensions")
    }
    Ok(video)
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timing {
    pub start: Option<String>,
    #[serde(default)]
    pub offset_seconds: f64,
    #[serde(default)]
    pub drift_ppm: f64,
}
#[derive(Clone, Debug)]
pub struct Clip {
    pub path: Option<PathBuf>,
    pub relative: PathBuf,
    pub video: Option<Video>,
    pub start: f64,
    pub rate: f64,
    pub duration: f64,
    pub timing_source: String,
}
pub struct Options {
    pub input: Option<PathBuf>,
    pub output: PathBuf,
    pub start: Option<String>,
    pub timings: Option<PathBuf>,
    pub offset: f64,
    pub drift: f64,
    pub duration: Option<f64>,
    pub overlay: Option<String>,
    pub software: bool,
    pub overwrite: bool,
    pub color_mode: String,
}
fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let e = entry?;
        if e.file_type()?.is_symlink() {
            continue;
        }
        let p = e.path();
        if p.is_dir() {
            walk(root, &p, out)?
        } else if p
            .extension()
            .and_then(|v| v.to_str())
            .is_some_and(|x| ["mp4", "mov", "mkv", "m4v"].contains(&x.to_lowercase().as_str()))
        {
            out.push(p.strip_prefix(root)?.to_owned());
        }
    }
    Ok(())
}
pub fn plan(track: &Track, opts: &Options) -> Result<Vec<Clip>> {
    if !opts.offset.is_finite() || !opts.drift.is_finite() || opts.drift.abs() > 10000. {
        bail!("Invalid time offset or drift (limit ±10000 ppm)")
    }
    let manifest: BTreeMap<String, Timing> = opts
        .timings
        .as_ref()
        .map(|p| -> Result<_> { Ok(serde_json::from_slice(&fs::read(p)?)?) })
        .transpose()?
        .unwrap_or_default();
    let mut clips = vec![];
    if let Some(input) = &opts.input {
        let directory = input.is_dir();
        if directory && opts.start.is_some() {
            bail!("--start is for one clip. Use --timings for a directory.")
        }
        let paths = if directory {
            let mut p = vec![];
            walk(input, input, &mut p)?;
            p.sort();
            p
        } else {
            vec![PathBuf::from(
                input.file_name().context("Invalid input filename")?,
            )]
        };
        if paths.is_empty() {
            bail!("No videos found")
        };
        for rel in paths {
            let path = if directory {
                input.join(&rel)
            } else {
                input.clone()
            };
            let video = probe(&path)?;
            let key = rel.to_string_lossy().replace('\\', "/");
            let timing = manifest.get(&key).cloned().unwrap_or_default();
            let explicit = timing.start.as_ref().or(opts.start.as_ref());
            let start=explicit.or(video.creation_time.as_ref()).with_context(||format!("{} has no capture timestamp. Supply --start RFC3339 or a --timings manifest. File modification dates are not capture times.",rel.display()))?;
            let start = timestamp(start)? + opts.offset + timing.offset_seconds;
            let rate = 1. + (opts.drift + timing.drift_ppm) / 1_000_000.;
            if !start.is_finite() || !rate.is_finite() || !(0.99..=1.01).contains(&rate) {
                bail!("Invalid manifest clock correction")
            };
            let duration = opts.duration.unwrap_or(video.duration).min(video.duration);
            clips.push(Clip {
                path: Some(path),
                relative: rel,
                video: Some(video),
                start,
                rate,
                duration,
                timing_source: if explicit.is_some() {
                    "explicit"
                } else {
                    "container creation_time (verify camera clock)"
                }
                .into(),
            });
        }
    } else {
        if opts.overlay.is_none() {
            bail!("Without --video, choose --overlay-only webm or prores")
        };
        let duration = opts
            .duration
            .context("Without video, provide --duration seconds")?;
        clips.push(Clip {
            path: None,
            relative: "overlay".into(),
            video: None,
            start: opts
                .start
                .as_ref()
                .map(|s| timestamp(s))
                .transpose()?
                .unwrap_or(track.start())
                + opts.offset,
            rate: 1. + opts.drift / 1_000_000.,
            duration,
            timing_source: "explicit/route start".into(),
        });
    }
    for c in &clips {
        if !c.duration.is_finite() || c.duration <= 0. {
            bail!("Duration must be positive")
        };
        if c.start > track.end() || c.start + c.duration * c.rate < track.start() {
            bail!(
                "No route overlap for {}; fix camera time with --offset or --timings",
                c.relative.display()
            )
        }
        if c.start < track.start() || c.start + c.duration * c.rate > track.end() {
            eprintln!(
                "Warning: {} partly outside route; missing data will show dashes",
                c.relative.display()
            )
        }
    }
    clips.sort_by(|a, b| a.start.total_cmp(&b.start));
    Ok(clips)
}
pub fn output_path(clip: &Clip, opts: &Options) -> PathBuf {
    let mut p = opts.output.join(&clip.relative);
    if let Some(ref overlay) = opts.overlay {
        p.set_extension(if overlay == "webm" {
            "overlay.webm"
        } else {
            "overlay.mov"
        });
    }
    p
}
pub fn preflight(clips: &[Clip], opts: &Options) -> Result<()> {
    let out = std::path::absolute(&opts.output)?;
    if let Some(input) = &opts.input {
        let input = fs::canonicalize(input)?;
        if input.is_dir() && out.starts_with(&input) {
            bail!("Output directory must be outside input directory")
        }
    }
    for c in clips {
        let p = output_path(c, opts);
        if p.exists() && !opts.overwrite {
            bail!(
                "Output exists: {} (use --overwrite intentionally)",
                p.display()
            )
        }
        if let Some(source) = &c.path
            && p.exists()
            && fs::canonicalize(&p)? == fs::canonicalize(source)?
        {
            bail!("Output would overwrite source")
        }
        if opts.overlay.is_none() {
            let v = c.video.as_ref().unwrap();
            if !["h264", "hevc"].contains(&v.codec.as_str()) {
                bail!(
                    "Unsupported source codec {}; supported: H.264 and HEVC",
                    v.codec
                )
            }
            if let Some((n, d)) = v.nominal_frame_rate.split_once('/') {
                let nominal = n.parse::<f64>().unwrap_or(0.) / d.parse::<f64>().unwrap_or(1.);
                if nominal.is_finite() && nominal > 0. && (nominal - v.fps).abs() > 0.001 {
                    bail!(
                        "Variable/ambiguous frame rate: export --overlay-only prores, or normalize the source to constant frame rate first"
                    )
                }
            }
            if !["yuv420p", "yuvj420p", "nv12", "yuv420p10le", "p010le"]
                .contains(&v.pixel_format.as_str())
            {
                bail!(
                    "Unsupported source pixel format {}; use a separate overlay to preserve source precision",
                    v.pixel_format
                )
            }
            if v.rotation % 360 != 0 {
                bail!("Rotated source: normalize orientation first or export a separate overlay")
            }
            if ["smpte2084", "arib-std-b67"].contains(&v.transfer.as_str()) {
                bail!(
                    "HDR source: export --overlay-only prores for color-managed compositing. Direct HDR compositing is not supported."
                )
            }
            if opts.color_mode == "log" {
                bail!(
                    "D-Log M needs grading before burn-in; use --overlay-only prores or grade the source first"
                )
            }
        }
    }
    Ok(())
}
struct Partial(PathBuf);
impl Drop for Partial {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
pub fn render_clip(
    track: &Track,
    clip: &Clip,
    opts: &Options,
    base: &Config,
    maps: &mut Maps,
) -> Result<Value> {
    let mut cfg = base.clone();
    if let Some(v) = &clip.video {
        cfg.width = v.width;
        cfg.height = v.height;
    }
    let mut renderer = Renderer::new(cfg.clone())?;
    let b = renderer.bounds;
    let output = output_path(clip, opts);
    fs::create_dir_all(output.parent().unwrap())?;
    let ext = output.extension().and_then(|x| x.to_str()).unwrap_or("mp4");
    let temporary = output.with_extension(format!("part.{ext}"));
    if temporary.exists() {
        bail!("Incomplete output already exists: {}", temporary.display())
    }
    let _guard = Partial(temporary.clone());
    let log = output.with_extension(format!("{ext}.ffmpeg.log"));
    let log_file = fs::File::create(&log)?;
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-hide_banner",
        "-loglevel",
        "warning",
        "-nostdin",
        "-y",
        "-filter_complex_threads",
        &cfg.encoding.filter_threads.to_string(),
    ]);
    if let Some(path) = &clip.path
        && opts.overlay.is_none()
    {
        cmd.arg("-noautorotate").arg("-i").arg(path);
    }
    cmd.args([
        "-thread_queue_size",
        "4",
        "-f",
        "rawvideo",
        "-pixel_format",
        "rgba",
        "-video_size",
        &format!("{}x{}", b.width, b.height),
        "-framerate",
        &cfg.overlay_fps.to_string(),
        "-i",
        "pipe:0",
    ]);
    let encoder: String;
    if let Some(ref kind) = opts.overlay {
        cmd.args([
            "-vf",
            &format!(
                "pad={}:{}:{}:{}:color=black@0",
                cfg.width, cfg.height, b.x, b.y
            ),
            "-an",
        ]);
        if kind == "webm" {
            encoder = "libvpx-vp9".into();
            cmd.args([
                "-c:v",
                &encoder,
                "-pix_fmt",
                "yuva420p",
                "-b:v",
                "0",
                "-crf",
                &cfg.encoding.webm_crf.to_string(),
                "-deadline",
                "realtime",
                "-cpu-used",
                &cfg.encoding.webm_cpu_used.to_string(),
                "-row-mt",
                "1",
                "-auto-alt-ref",
                "0",
            ]);
        } else {
            encoder = "prores_ks".into();
            cmd.args([
                "-c:v",
                &encoder,
                "-profile:v",
                "4",
                "-pix_fmt",
                "yuva444p10le",
                "-alpha_bits",
                "16",
            ]);
        }
    } else {
        let v = clip.video.as_ref().unwrap();
        let ten = v.pixel_format.contains("10") || v.pixel_format.contains("12");
        let format = if ten { "yuv420p10" } else { "yuv420" };
        let filter = format!(
            "[0:v:0]setpts=PTS-STARTPTS[base];[base][1:v]overlay=x={}:y={}:format={}:alpha=straight:eof_action=repeat[out]",
            b.x, b.y, format
        );
        cmd.args([
            "-filter_complex",
            &filter,
            "-map",
            "[out]",
            "-map",
            "0:a?",
            "-c:a",
            "copy",
            "-map_metadata",
            "0",
            "-fps_mode",
            "cfr",
            "-r",
            &v.frame_rate,
        ]);
        let hw = cfg!(target_os = "macos") && !opts.software;
        encoder = match (v.codec.as_str(), hw) {
            ("hevc", true) => "hevc_videotoolbox",
            ("h264", true) => "h264_videotoolbox",
            ("hevc", false) => "libx265",
            _ => "libx264",
        }
        .into();
        cmd.args(["-c:v", &encoder]);
        if hw {
            let source_rate = if v.bit_rate > 0 {
                v.bit_rate as f64
            } else {
                v.width as f64 * v.height as f64 * v.fps * 0.08
            };
            let target_rate = (source_rate * cfg.encoding.bitrate_scale).max(500_000.) as u64;
            cmd.args([
                "-allow_sw",
                "1",
                "-realtime",
                "1",
                "-b:v",
                &target_rate.to_string(),
            ]);
        } else {
            cmd.args([
                "-preset",
                &cfg.encoding.software_preset,
                "-crf",
                &cfg.encoding.software_crf.to_string(),
            ]);
        }
        cmd.args([
            "-pix_fmt",
            if ten && hw {
                "p010le"
            } else if ten {
                "yuv420p10le"
            } else {
                "yuv420p"
            },
        ]);
        if v.codec == "hevc" {
            cmd.args(["-tag:v", "hvc1"]);
        }
        for (flag, value) in [
            ("-color_primaries", &v.primaries),
            ("-color_trc", &v.transfer),
            ("-colorspace", &v.space),
        ] {
            if !value.is_empty() && value != "unknown" {
                cmd.args([flag, value]);
            }
        }
        if ["mp4", "mov", "m4v"].contains(&ext) {
            cmd.args(["-movflags", "+faststart"]);
        }
    }
    cmd.args(["-t", &clip.duration.to_string()])
        .arg(&temporary)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log_file));
    let timer = Instant::now();
    let mut child = cmd.spawn().context("Cannot start FFmpeg")?;
    let mut input = child.stdin.take().unwrap();
    let frames = (clip.duration * cfg.overlay_fps).ceil() as usize;
    let mut missing = 0;
    let result = (|| -> Result<()> {
        for i in 0..frames {
            let time = clip.start + i as f64 / cfg.overlay_fps * clip.rate;
            let sample = track.at(time);
            if sample.is_none() {
                missing += 1
            }
            let mut frame = renderer.frame(track, sample.as_ref(), maps)?;
            // tiny-skia stores premultiplied RGBA; FFmpeg/alpha exports receive straight RGBA.
            for pixel in frame.data_mut().chunks_exact_mut(4) {
                let a = pixel[3] as u32;
                if a > 0 && a < 255 {
                    for c in &mut pixel[..3] {
                        *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                    }
                }
            }
            input.write_all(frame.data())?;
            if i % ((cfg.overlay_fps * 10.) as usize).max(1) == 0 {
                eprintln!(
                    "{}: {:0.1}/{:0.1} s",
                    clip.relative.display(),
                    i as f64 / cfg.overlay_fps,
                    clip.duration
                );
            }
        }
        Ok(())
    })();
    drop(input);
    if let Err(e) = result {
        let _ = child.kill();
        let _ = child.wait();
        bail!("Rendering failed: {e}; see {}", log.display())
    }
    let status = child.wait()?;
    if !status.success() {
        bail!("FFmpeg failed; see {}", log.display())
    }
    let rendered = probe(&temporary)?;
    if rendered.width != cfg.width || rendered.height != cfg.height {
        bail!("Output dimensions differ")
    };
    if (rendered.duration - clip.duration).abs() > 0.15f64.max(2. / rendered.fps) {
        bail!(
            "Output duration differs: {} vs {}",
            rendered.duration,
            clip.duration
        )
    }
    if opts.overlay.is_none() && rendered.codec != clip.video.as_ref().unwrap().codec {
        bail!("Output codec differs")
    }
    if output.exists() {
        if !opts.overwrite {
            bail!("Output appeared during rendering; refusing to replace it")
        }
        fs::remove_file(&output)?;
    }
    fs::rename(&temporary, &output)?;
    let elapsed = timer.elapsed().as_secs_f64();
    let report = serde_json::json!({"output":output.file_name(),"source":clip.relative,"source_video":clip.video,"capture_start_utc_seconds":clip.start,"clock_rate":clip.rate,"timing_source":clip.timing_source,"duration_seconds":clip.duration,"render_wall_seconds":elapsed,"speed_x":clip.duration/elapsed,"overlay_fps":cfg.overlay_fps,"overlay_frames":frames,"missing_gps_frames":missing,"encoder":encoder,"canvas":[cfg.width,cfg.height],"rendered_region":[b.x,b.y,b.width,b.height],"source_codec_preserved":opts.overlay.is_none(),"map_source":cfg.map_source,"map_credit":"© OpenStreetMap contributors (except synthetic demo maps)","output_video":rendered});
    fs::write(
        output.with_extension(format!("{ext}.json")),
        serde_json::to_vec_pretty(&report)?,
    )?;
    eprintln!(
        "Saved {} ({elapsed:0.1}s, {:0.2}× real time)",
        output.display(),
        clip.duration / elapsed
    );
    Ok(report)
}
