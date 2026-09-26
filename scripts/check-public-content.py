#!/usr/bin/env python3
"""Check publishable files for common accidental private paths and credentials."""
import pathlib, re, subprocess, sys
root=pathlib.Path(__file__).resolve().parents[1]
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=root).decode().split('\0')
patterns=[r'/' r'Users' r'/[^/\s]+/',r'/' r'home' r'/[^/\s]+/',r'AKIA[0-9A-Z]{16}',r'gh[pousr]_[A-Za-z0-9]{30,}',r'github_pat_[A-Za-z0-9_]{30,}',r'sk-[A-Za-z0-9_-]{30,}',r'-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----']
errors=[]
for name in sorted(set(paths)):
    if not name: continue
    p=root/name
    if not p.is_file(): continue
    if p.suffix.lower() in ['.mp4','.mov','.mkv','.webm','.p12','.mobileprovision','.key','.pem']: errors.append(name+': private/media file must not be tracked')
    if p.stat().st_size>15_000_000: errors.append(name+': unexpectedly large public file')
    try: text=p.read_text()
    except UnicodeDecodeError: continue
    for pattern in patterns:
        if re.search(pattern,text): errors.append(name+': potential private content')
if errors:
    print('\n'.join(errors));sys.exit(1)
print('Public-content scan passed. Review images and staged content separately; regex checks are not a security audit.')
