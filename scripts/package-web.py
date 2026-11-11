#!/usr/bin/env python3
"""Package the web build with content-addressed assets for atomic Pages deployments."""
import hashlib
import json
from pathlib import Path
import re
import subprocess


def package(source, destination, revision):
    html = (source / 'index.html').read_text()
    # Each release gets its own directory; old local output is never recursively removed.
    if destination.exists() and any(destination.iterdir()):
        raise ValueError('Release destination must be empty; use a fresh directory')
    assets = destination / 'assets'
    assets.mkdir(parents=True, exist_ok=True)
    names = {}
    hashes = {}
    for name in ('fatherload.js', 'fatherload.wasm'):
        data = (source / name).read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        filename = f'{Path(name).stem}.{digest}{Path(name).suffix}'
        (assets / filename).write_bytes(data)
        names[name] = 'assets/' + filename
        hashes[names[name]] = digest
    # Emscripten's loader asks locateFile for its WASM path before fetching it.
    html = html.replace('canvas, noInitialRun:true,',
        'canvas, noInitialRun:true,\n  locateFile(path) { return ' + json.dumps(names) + '[path] || path; },')
    html, count = re.subn(r'src="fatherload.js"', 'src="' + names['fatherload.js'] + '"', html)
    if count != 1 or 'locateFile(path)' not in html:
        raise ValueError('Web loader contract changed; cannot safely package release')
    (destination / 'index.html').write_text(html)
    (destination / '404.html').write_text('<!doctype html><title>Not found</title><h1>Not found</h1>')
    (destination / '_headers').write_text('''/*
  X-Content-Type-Options: nosniff
  Referrer-Policy: strict-origin-when-cross-origin
  X-Frame-Options: DENY
  Content-Security-Policy: frame-ancestors 'none'
/
  Cache-Control: no-cache
/index.html
  Cache-Control: no-cache
/release.json
  Cache-Control: no-cache
/assets/*
  Cache-Control: public, max-age=31536000, immutable
''')
    manifest = {'revision': revision, 'assets': hashes,
                'index_sha256': hashlib.sha256(html.encode()).hexdigest()}
    (destination / 'release.json').write_text(json.dumps(manifest, indent=2) + '\n')


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, default=Path('target/web'))
    parser.add_argument('--destination', type=Path, required=True)
    args = parser.parse_args()
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    package(args.source, args.destination, revision)
    print('Packaged content-addressed web release')
