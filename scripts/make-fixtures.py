#!/usr/bin/env python3
"""Generate public, synthetic GPS fixtures. Never read a user's ride or footage."""
from pathlib import Path
from datetime import datetime, timedelta, timezone
import csv, math
root = Path(__file__).resolve().parents[1] / 'examples'
root.mkdir(exist_ok=True)
start = datetime(2026, 1, 1, 12, tzinfo=timezone.utc)
rows = []
for i in range(121):
    # Fictional eastbound ride in a public London park; not a recorded journey.
    rows.append([1, (start + timedelta(seconds=i)).isoformat(timespec='milliseconds').replace('+00:00','Z'),
                 f'{51.5315 + math.sin(i / 60) * .00025:.7f}', f'{-.158 + i * .000035:.7f}',
                 f'{37 + math.sin(i/20)*3:.2f}', f'{3.3 + math.sin(i/10)*.5:.3f}', 'cycling', i,
                 'false' if i == 65 else 'true', '4.0', '6.0', '90.0', '8.0', '0.2', 2])
rows[65][5] = ''
header='segment,timestamp,latitude,longitude,altitude_m,speed_m_s,activity,elapsed_s,speed_valid,horizontal_accuracy_m,vertical_accuracy_m,course_deg,course_accuracy_deg,speed_accuracy_m_s,schema_version'.split(',')
with (root/'synthetic-ride.csv').open('w',newline='') as f:
    writer=csv.writer(f);writer.writerow(header);writer.writerows(rows)
with (root/'synthetic-ride.gpx').open('w') as f:
    f.write('<?xml version="1.0" encoding="UTF-8"?>\n<gpx version="1.1" creator="synthetic-fixture" xmlns="http://www.topografix.com/GPX/1/1" xmlns:sr="https://yumcut.com/mobile/speedometer-gps"><trk><name>Fictional demo ride</name><trkseg>\n')
    for r in rows:
        speed=f'<sr:speed unit="m/s">{r[5]}</sr:speed>' if r[5] else ''
        f.write(f'<trkpt lat="{r[2]}" lon="{r[3]}"><ele>{r[4]}</ele><time>{r[1]}</time><extensions>{speed}<sr:speedValid>{r[8]}</sr:speedValid><sr:horizontalAccuracy unit="m">4.0</sr:horizontalAccuracy><sr:course unit="deg">90.0</sr:course></extensions></trkpt>\n')
    f.write('</trkseg></trk></gpx>\n')
