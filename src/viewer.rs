use crate::{
    config::Config,
    maps::Maps,
    render::Renderer,
    telemetry::{Sample, Track},
};
use anyhow::Result;
use serde::Deserialize;
use std::{io::Read, path::PathBuf};
use tiny_http::{Header, Method, Response, Server};
const HTML: &str = include_str!("../assets/viewer.html");
#[derive(Deserialize)]
struct Preview {
    config: Config,
    seconds: f64,
    speed: Option<f64>,
    distance: Option<f64>,
}
pub fn demo_sample(track: &Track, seconds: f64) -> Sample {
    let mut s = track.at(track.start() + seconds.clamp(0., 600.)).unwrap();
    s.distance = 13100.;
    s.speed = Some(12. / 3.6);
    s
}
pub fn serve(
    cfg: Config,
    track: Track,
    cache: Option<PathBuf>,
    osm: Option<PathBuf>,
    port: u16,
    dummy: bool,
) -> Result<()> {
    let server = Server::http(("127.0.0.1", port)).map_err(|e| anyhow::anyhow!("{e}"))?;
    eprintln!("Layer viewer: http://127.0.0.1:{port} (Ctrl-C to stop)");
    let mut maps = Maps::new(cache, &cfg.map_source, osm.as_deref())?;
    for mut req in server.incoming_requests() {
        let host = req
            .headers()
            .iter()
            .find(|h| h.field.equiv("Host"))
            .map(|h| h.value.as_str());
        if host != Some(format!("127.0.0.1:{port}").as_str()) {
            let _ = req.respond(Response::from_string("Host rejected").with_status_code(403));
            continue;
        }

        let origin = req
            .headers()
            .iter()
            .find(|h| h.field.equiv("Origin"))
            .map(|h| h.value.as_str());
        if origin.is_some_and(|v| v != format!("http://127.0.0.1:{port}")) {
            let _ = req.respond(Response::from_string("Origin rejected").with_status_code(403));
            continue;
        }
        let url = req.url().to_owned();
        let result = (|| -> Result<(Vec<u8>, &str)> {
            match (req.method(), url.as_str()) {
                (&Method::Get, "/") => Ok((HTML.as_bytes().to_vec(), "text/html; charset=utf-8")),
                (&Method::Get, "/config") => Ok((serde_json::to_vec(&cfg)?, "application/json")),
                (&Method::Post, "/preview") | (&Method::Post, "/toml") => {
                    let mut body = String::new();
                    req.as_reader()
                        .take(1024 * 1024 + 1)
                        .read_to_string(&mut body)?;
                    if body.len() > 1024 * 1024 {
                        anyhow::bail!("Request too large")
                    };
                    let input: Preview = serde_json::from_str(&body)?;
                    input.config.validate()?;
                    if url == "/toml" {
                        return Ok((
                            toml::to_string_pretty(&input.config)?.into_bytes(),
                            "text/plain; charset=utf-8",
                        ));
                    }
                    if !input.seconds.is_finite() {
                        anyhow::bail!("Invalid preview time")
                    };
                    if input.config.map_source != cfg.map_source
                        || input.config.map_zoom != cfg.map_zoom
                    {
                        anyhow::bail!(
                            "Change map source/zoom in the config and restart viewer to prepare the map cache"
                        )
                    };
                    let mut r = Renderer::new(input.config)?;
                    let mut s = if dummy {
                        Some(demo_sample(&track, input.seconds))
                    } else {
                        track.at(track.start() + input.seconds)
                    };
                    if dummy && let Some(s) = s.as_mut() {
                        if let Some(v) = input.speed {
                            if !v.is_finite() || !(0. ..=500.).contains(&v) {
                                anyhow::bail!("Invalid demo speed")
                            };
                            s.speed = Some(v / 3.6)
                        }
                        if let Some(v) = input.distance {
                            if !v.is_finite() || v < 0. {
                                anyhow::bail!("Invalid demo distance")
                            };
                            s.distance = v * 1000.;
                        }
                    }
                    let p = r.full_frame(&track, s.as_ref(), &mut maps)?;
                    Ok((p.encode_png()?, "image/png"))
                }
                _ => anyhow::bail!("Not found"),
            }
        })();
        match result {
            Ok((bytes, mime)) => {
                let mut response = Response::from_data(bytes);
                response.add_header(Header::from_bytes("Content-Type", mime).unwrap());
                response.add_header(Header::from_bytes("Cache-Control", "no-store").unwrap());
                response
                    .add_header(Header::from_bytes("X-Content-Type-Options", "nosniff").unwrap());
                let _ = req.respond(response);
            }
            Err(e) => {
                let _ = req.respond(Response::from_string(e.to_string()).with_status_code(400));
            }
        }
    }
    Ok(())
}
