#!/usr/bin/env python3
"""Verify HTTPS release identity, integrity, MIME and cache/security headers."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import time
import urllib.request


def verify(origin, revision, local):
    def fetch(name):
        with urllib.request.urlopen(origin.rstrip('/') + '/' + name, timeout=30) as response:
            if not response.url.startswith(origin.rstrip('/') + '/'):
                raise ValueError('Unexpected cross-origin redirect')
            return response.read(), response.headers
    raw, headers = fetch('release.json')
    manifest = json.loads(raw)
    if manifest['revision'] != revision:
        raise ValueError('Live revision does not match requested commit')
    if local and raw != (local / 'release.json').read_bytes():
        raise ValueError('Live manifest does not match tested artifact')
    html, headers = fetch('index.html')
    if hashlib.sha256(html).hexdigest() != manifest['index_sha256']:
        raise ValueError('HTML integrity mismatch')
    if 'no-cache' not in headers.get('Cache-Control', ''):
        raise ValueError('HTML must revalidate')
    if headers.get('X-Content-Type-Options') != 'nosniff':
        raise ValueError('Missing nosniff header')
    if "frame-ancestors 'none'" not in headers.get('Content-Security-Policy', ''):
        raise ValueError('Missing framing restriction')
    if len(manifest['assets']) != 2:
        raise ValueError('Expected JS and WASM assets')
    for name, digest in manifest['assets'].items():
        if not re.fullmatch(r'assets/fatherload\.[a-f0-9]{64}\.(js|wasm)', name):
            raise ValueError('Unsafe asset name')
        data, headers = fetch(name)
        if hashlib.sha256(data).hexdigest() != digest:
            raise ValueError('Asset integrity mismatch')
        if 'immutable' not in headers.get('Cache-Control', ''):
            raise ValueError('Versioned assets must be immutable')
        if name.endswith('.wasm') and headers.get_content_type() != 'application/wasm':
            raise ValueError('Incorrect WASM MIME type')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('origin')
    parser.add_argument('--revision', required=True)
    parser.add_argument('--artifact', type=Path)
    parser.add_argument('--attempts', type=int, default=1)
    args = parser.parse_args()
    if not args.origin.startswith('https://') or args.attempts < 1:
        parser.error('HTTPS origin and positive attempts required')
    for attempt in range(args.attempts):
        try:
            verify(args.origin, args.revision, args.artifact)
            print('PASS: live release identity, asset integrity, MIME and headers')
            break
        except (OSError, ValueError, KeyError):
            if attempt + 1 == args.attempts:
                raise SystemExit('Live release verification failed; inspect deployment/DNS/TLS/cache state') from None
            time.sleep(20)
