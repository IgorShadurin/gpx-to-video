#!/usr/bin/env python3
"""End-to-end media checks on synthetic 4K footage. Requires FFmpeg and a release build."""
import json, subprocess, pathlib, time, sys
ROOT=pathlib.Path(__file__).resolve().parents[1]
BIN=ROOT/'target/release/gpx-to-video'
BASE=ROOT/'tmp/smoke'; BASE.mkdir(parents=True,exist_ok=True)
RUN=BASE/str(time.time_ns()); RUN.mkdir()
SRC=RUN/'source'; SRC.mkdir()
OUT=RUN/'rendered'

def run(args, capture=False, check=True):
    result=subprocess.run([str(x) for x in args],cwd=ROOT,stdout=subprocess.PIPE if capture else subprocess.DEVNULL,stderr=subprocess.PIPE)
    if check and result.returncode: raise RuntimeError(result.stderr.decode(errors='replace')[-4000:])
    return result

def probe(p): return json.loads(run(['ffprobe','-v','error','-show_streams','-show_format','-of','json',p],True).stdout)
def vstream(p): return next(s for s in probe(p)['streams'] if s['codec_type']=='video')
def digest_audio(p): return run(['ffmpeg','-v','error','-i',p,'-map','0:a:0','-c','copy','-f','hash','-'],True).stdout

def make(name,codec,fps,ten=False):
    output=SRC/name
    encoder=('hevc_videotoolbox' if codec=='hevc' else 'h264_videotoolbox') if sys.platform=='darwin' else ('libx265' if codec=='hevc' else 'libx264')
    args=['ffmpeg','-hide_banner','-loglevel','error','-y','-loop','1','-framerate',str(fps),'-i',ROOT/'assets/demo-background.png','-f','lavfi','-i','sine=frequency=440:sample_rate=48000','-vf','scale=3840:2160','-t','4','-c:v',encoder,'-b:v','18M','-pix_fmt','p010le' if ten and sys.platform=='darwin' else 'yuv420p10le' if ten else 'yuv420p','-c:a','aac','-metadata','creation_time=2026-01-01T12:00:10Z','-color_primaries','bt709','-color_trc','bt709','-colorspace','bt709',output]
    run(args)
    return output

sources=[make('clip one.mp4','h264',30),make('clip two.mp4','hevc','60000/1001',True)]
start=time.monotonic()
run([BIN,'render','--route','examples/synthetic-ride.gpx','--video',SRC,'--output',OUT,'--map-source','demo'])
results=[]
for p in sources:
    out=OUT/p.name
    a,b=vstream(p),vstream(out)
    assert (b['width'],b['height'])==(3840,2160)
    assert a['codec_name']==b['codec_name']
    assert a['avg_frame_rate']==b['avg_frame_rate'],(a['avg_frame_rate'],b['avg_frame_rate'])
    assert abs(float(a['duration'])-float(b['duration']))<.05
    assert digest_audio(p)==digest_audio(out),'compressed audio changed'
    if '10' in a['pix_fmt']: assert '10' in b['pix_fmt'],'bit depth lost'
    def pts(path):
        data=json.loads(run(['ffprobe','-v','error','-select_streams','v:0','-show_entries','frame=best_effort_timestamp_time','-of','json',path],True).stdout)
        return [float(f['best_effort_timestamp_time']) for f in data['frames']]
    aa,bb=pts(p),pts(out)
    assert len(aa)==len(bb),(len(aa),len(bb))
    assert max(abs((x-aa[0])-(y-bb[0])) for x,y in zip(aa,bb))<.0001
    report=json.loads(out.with_suffix('.mp4.json').read_text())
    results.append({k:report[k] for k in ['duration_seconds','render_wall_seconds','speed_x','encoder','canvas','overlay_fps','missing_gps_frames']})
# Both transparent codecs, with real alpha and unpainted upper frame.
for kind in ['webm','prores']:
    dest=RUN/kind
    run([BIN,'render','--route','examples/synthetic-ride.csv','--output',dest,'--overlay-only',kind,'--duration','1','--map-source','demo'])
    path=dest/('overlay.overlay.webm' if kind=='webm' else 'overlay.overlay.mov')
    args=['ffmpeg','-v','error']+(['-c:v','libvpx-vp9'] if kind=='webm' else [])+['-i',path,'-vf','alphaextract','-frames:v','1','-pix_fmt','gray','-f','rawvideo','-']
    alpha=run(args,True).stdout
    assert len(alpha)==3840*2160
    assert max(alpha[:3840*500])==0,'transparent area is opaque'
    assert max(alpha)>240,'painted areas lost alpha'
    assert any(0<x<255 for x in alpha),'antialiased alpha lost'
# Missing creation date fails rather than guessing from file times.
untimed=RUN/'untimed.mp4'
run(['ffmpeg','-v','error','-i',sources[0],'-map_metadata','-1','-c','copy',untimed])
failure=run([BIN,'render','--route','examples/synthetic-ride.csv','--video',untimed,'--output',RUN/'bad','--map-source','none','--dry-run'],capture=True,check=False)
assert failure.returncode and b'no capture timestamp' in failure.stderr
run([BIN,'render','--route','examples/synthetic-ride.csv','--video',untimed,'--output',RUN/'explicit','--map-source','none','--start','2026-01-01T12:00:20Z','--dry-run'])
# Explicit per-file timing and the portable software path, tested on macOS.
small=RUN/'small.mp4'
run(['ffmpeg','-v','error','-i',sources[0],'-vf','scale=1280:720','-t','1','-map_metadata','-1','-c:v','libx264','-preset','ultrafast','-c:a','copy',small])
manifest=RUN/'timings.json'
manifest.write_text(json.dumps({'small.mp4':{'start':'2026-01-01T12:00:20Z','offset_seconds':-1.0,'drift_ppm':10}}))
run([BIN,'render','--route','examples/synthetic-ride.csv','--video',small,'--output',RUN/'software','--timings',manifest,'--software','--map-source','demo'])
sw=json.loads((RUN/'software/small.mp4.json').read_text())
from datetime import datetime,timezone
assert sw['capture_start_utc_seconds']==datetime(2026,1,1,12,0,19,tzinfo=timezone.utc).timestamp()
assert abs(sw['clock_rate']-1.00001)<1e-12
assert sw['encoder']=='libx264'
# Do not silently burn an SDR graphic into HDR footage.
hdr=RUN/'hdr-tagged.mp4'
run(['ffmpeg','-v','error','-i',sources[1],'-c','copy','-bsf:v','hevc_metadata=transfer_characteristics=18:colour_primaries=9:matrix_coefficients=9',hdr])
failed=run([BIN,'render','--route','examples/synthetic-ride.csv','--video',hdr,'--output',RUN/'hdr-output','--start','2026-01-01T12:00:10Z','--dry-run'],capture=True,check=False)
assert failed.returncode and b'HDR source' in failed.stderr
# Overwrite refusal and negative offsets are checked before rendering.
assert run([BIN,'render','--route','examples/synthetic-ride.csv','--video',SRC,'--output',OUT,'--map-source','none','--dry-run'],capture=True,check=False).returncode
run([BIN,'render','--route','examples/synthetic-ride.csv','--video',sources[0],'--output',RUN/'offset','--offset','-5','--dry-run'])
(ROOT/'docs').mkdir(exist_ok=True)
summary={'synthetic_only':True,'platform_tested':sys.platform,'machine_class':'Apple M1 Max' if sys.platform=='darwin' else 'unspecified','checks':['4K H.264 30fps','4K HEVC 10-bit 60000/1001fps','same codecs/dimensions/frame timestamps','compressed audio unchanged','directory and spaces in filenames','GPX and CSV','4K VP9 alpha','4K ProRes 4444 alpha','missing capture time rejected','explicit start and negative offset','overwrite protection','per-clip start/offset/drift','software H.264','HDR rejection'],'renders':results,'scratch_directory':str(RUN.relative_to(ROOT))}
(ROOT/'docs/media-validation.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps(summary,indent=2))
