# Validation

All published fixtures are synthetic. Checks ran on macOS on an Apple M1 Max; Linux, Windows, and original Osmo Action 6 recordings have not been tested.

## Automated checks

`cargo fmt --check`, strict Clippy, and all 20 integration tests pass. Coverage includes:

- GPX/SpiderRoute CSV equivalence, fractional UTC timestamps and timezone offsets.
- Interpolation, segment boundaries, outages, invalid speed versus a genuine stop, and derived speed.
- Negative longitudes, antimeridian crossing, heading wrap, and invalid coordinates/timestamps.
- Distance formatting, long Cyrillic names, Unicode truncation, and Cyrillic font coverage.
- Configuration validation, transparent rendering, bounded drawing regions, offline map behavior, and road/settlement label selection.
- Indexed queries over a synthetic 72,001-point, 20-hour track. This is a telemetry test, not a 20-hour video-render or memory-soak measurement.

`scripts/smoke-test.py` generates disposable four-second 4K source videos and exercises the complete FFmpeg pipeline. It verifies H.264 at 30 fps and 10-bit HEVC at 60000/1001 fps; matching dimensions, codec family, bit depth, frame counts and presentation timestamps; and identical compressed audio hashes. It also checks GPX/CSV input, filenames containing spaces, batch processing, per-clip clock corrections, software H.264, missing-date rejection, HDR rejection, and overwrite protection.

Both VP9 WebM and ProRes 4444 exports were decoded and checked for transparent, opaque, and antialiased alpha values. See [machine-readable media results](media-validation.json). Short clips include startup overhead; their throughput is not a sustained performance guarantee.

## Map and browser checks

Nine actual OSM vector cells were downloaded for the synthetic route's public park area. Repeating the preview offline produced an identical PNG without changing the cache files. Public-server errors occurred during the initial fetch; completed cells survived and the download resumed successfully. See [cache results](map-validation.json) and [the actual OSM preview](after-osm.jpg).

The local browser studio was opened and exercised with Russian/English labels, visibility toggles, dummy speed changes, and individual distance-text sizing. Each operation produced a new native-resolution transparent PNG. The displayed defaults and both language previews were visually inspected. [Studio screenshot](studio.jpg).

## Practical limits

- Capture-time matching depends on the camera's embedded clock. Verify it using `inspect`/`--dry-run`, then correct offsets or supply a timing manifest where needed.
- Burn-in supports constant-frame-rate SDR H.264/HEVC, including 10-bit SDR. HDR/D-Log M should use a separate alpha layer for composition after grading.
- Tests do not establish sustained multi-hour throughput, peak memory on every map dataset, or identical behavior on untested operating systems.
- Synthetic footage uses a generated background upscaled to 4K. Overlay primitives and typography are rasterized at native output resolution.

Reproduce with the commands in the README. Media and map caches are intentionally excluded from git; the test scripts recreate their disposable inputs.
