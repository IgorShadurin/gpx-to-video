use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use gpx_to_video::{
    config::Config,
    maps::Maps,
    render::Renderer,
    telemetry::{Track, demo_track},
    video::{self, Options},
    viewer,
};
use std::{fs, path::PathBuf};
#[derive(Parser)]
#[command(
    version,
    about = "Local bicycle telemetry overlays from timed GPX/CSV and video"
)]
struct Cli {
    #[command(subcommand)]
    command: Action,
}
#[derive(Args)]
struct Common {
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    route: Option<PathBuf>,
    #[arg(long)]
    cache: Option<PathBuf>,
    #[arg(long)]
    osm_data: Option<PathBuf>,
    #[arg(long)]
    offline: bool,
    #[arg(long,value_parser=["ru","en"])]
    language: Option<String>,
    #[arg(long,value_parser=["osm","demo","none"])]
    map_source: Option<String>,
}
#[derive(Subcommand)]
enum Action {
    /// Save all editable defaults as TOML.
    Init {
        #[arg(long, default_value = "bicycle.toml")]
        output: PathBuf,
    },
    /// Draw a transparent PNG. Without --route uses synthetic demo data.
    Preview {
        #[command(flatten)]
        common: Common,
        #[arg(long, default_value = "overlay.png")]
        output: PathBuf,
        #[arg(long, default_value_t = 90.)]
        seconds: f64,
    },
    /// Local browser configurator. No video is encoded.
    View {
        #[command(flatten)]
        common: Common,
        #[arg(long, default_value_t = 8787)]
        port: u16,
    },
    /// Inspect capture metadata without rendering.
    Inspect { video: PathBuf },
    /// Process a video or a directory; or render only a transparent layer.
    Render {
        #[command(flatten)]
        common: Common,
        #[arg(long)]
        video: Option<PathBuf>,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        start: Option<String>,
        #[arg(long)]
        timings: Option<PathBuf>,
        #[arg(long, default_value_t = 0., allow_hyphen_values = true)]
        offset: f64,
        #[arg(long, default_value_t = 0., allow_hyphen_values = true)]
        drift_ppm: f64,
        #[arg(long)]
        duration: Option<f64>,
        #[arg(long,value_parser=["webm","prores"])]
        overlay_only: Option<String>,
        #[arg(long)]
        software: bool,
        #[arg(long)]
        overwrite: bool,
        #[arg(long,value_parser=["sdr","log"],default_value="sdr")]
        color_mode: String,
        #[arg(long)]
        dry_run: bool,
    },
}
fn load(c: &Common) -> Result<(Config, Track, bool)> {
    let mut cfg = Config::load(c.config.as_deref())?;
    if let Some(l) = &c.language {
        cfg.language = l.clone()
    }
    let dummy = c.route.is_none();
    if let Some(s) = &c.map_source {
        cfg.map_source = s.clone()
    } else if dummy {
        cfg.map_source = "demo".into()
    };
    if dummy && cfg.total_km.is_none() {
        cfg.total_km = Some(55.)
    };
    cfg.validate()?;
    let track = c
        .route
        .as_ref()
        .map(|p| Track::load(p, cfg.max_gap_seconds))
        .transpose()?
        .unwrap_or_else(demo_track);
    Ok((cfg, track, dummy))
}
fn main() -> Result<()> {
    match Cli::parse().command {
        Action::Init { output } => {
            if output.exists() {
                anyhow::bail!("File exists: {}", output.display())
            }
            fs::write(&output, toml::to_string_pretty(&Config::default())?)?;
            println!("Saved {}", output.display());
        }
        Action::Inspect { video } => {
            println!("{}", serde_json::to_string_pretty(&video::probe(&video)?)?)
        }
        Action::Preview {
            common,
            output,
            seconds,
        } => {
            let (cfg, track, dummy) = load(&common)?;
            let mut maps = Maps::new(common.cache, &cfg.map_source, common.osm_data.as_deref())?;
            let t = track.start() + seconds;
            maps.prepare(&track, &[(t, t)], &cfg, common.offline)?;
            let s = if dummy {
                Some(viewer::demo_sample(&track, seconds))
            } else {
                track.at(t)
            };
            let mut renderer = Renderer::new(cfg)?;
            if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
                fs::create_dir_all(parent)?;
            }
            renderer
                .full_frame(&track, s.as_ref(), &mut maps)?
                .save_png(&output)?;
            println!("Saved {}", output.display());
        }
        Action::View { common, port } => {
            let (cfg, track, dummy) = load(&common)?;
            let maps = Maps::new(
                common.cache.clone(),
                &cfg.map_source,
                common.osm_data.as_deref(),
            )?;
            maps.prepare(
                &track,
                &[(track.start(), track.end())],
                &cfg,
                common.offline,
            )?;
            viewer::serve(cfg, track, common.cache, common.osm_data, port, dummy)?;
        }
        Action::Render {
            common,
            video,
            output,
            start,
            timings,
            offset,
            drift_ppm,
            duration,
            overlay_only,
            software,
            overwrite,
            color_mode,
            dry_run,
        } => {
            if common.route.is_none() {
                anyhow::bail!("render requires --route GPX/CSV")
            };
            let (cfg, track, _) = load(&common)?;
            let opts = Options {
                input: video,
                output,
                start,
                timings,
                offset,
                drift: drift_ppm,
                duration,
                overlay: overlay_only,
                software,
                overwrite,
                color_mode,
            };
            let clips = video::plan(&track, &opts)?;
            video::preflight(&clips, &opts)?;
            for c in &clips {
                println!(
                    "{} · {:0.2}s · start {:0.3} · {}",
                    c.relative.display(),
                    c.duration,
                    c.start,
                    c.timing_source
                );
            }
            if dry_run {
                return Ok(());
            }
            let mut maps = Maps::new(common.cache, &cfg.map_source, common.osm_data.as_deref())?;
            maps.prepare(
                &track,
                &clips
                    .iter()
                    .map(|c| (c.start, c.start + c.duration * c.rate))
                    .collect::<Vec<_>>(),
                &cfg,
                common.offline,
            )?;
            for clip in &clips {
                video::render_clip(&track, clip, &opts, &cfg, &mut maps)?;
            }
        }
    }
    Ok(())
}
