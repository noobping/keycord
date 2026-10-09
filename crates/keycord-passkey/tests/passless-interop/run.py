#!/usr/bin/env python3
"""Test against an unmodified, pinned Passless decoder. No daemon or device is needed.

The upstream module and its test-only dependencies live in a temporary cache.
Use --generate to regenerate synthetic Keycord fixtures (never real credentials).
"""
import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess
import tempfile
import urllib.request

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
REVISION = '4e420e081c115122ac16bb13824984dc78ba6513'
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--generate', action='store_true')
args = parser.parse_args()
cache = Path(tempfile.gettempdir()) / 'keycord-passless-interop'
(cache / 'src').mkdir(parents=True, exist_ok=True)
source = cache / 'src/credential.rs'
if not source.exists():
    urllib.request.urlretrieve(f'https://raw.githubusercontent.com/pando85/passless/{REVISION}/cmd/passless/src/storage/credential.rs', source)
# Pin the source contents as well as the revision.
expected = 'cbc0115cc6160b1d8fc545f7b32fe61a034d5bcee06a966725649d2753cb671f'
if hashlib.sha256(source.read_bytes()).hexdigest() != expected:
    raise SystemExit('Unexpected Passless reference source checksum')
shutil.copyfile(HERE / 'interop.rs', cache / 'src/main.rs')
(cache / 'Cargo.toml').write_text('''[package]
name = "keycord-passless-reference"
version = "0.0.0"
edition = "2024"
[dependencies]
soft-fido2 = { version = "=0.17.0", default-features = false }
soft-fido2-ctap = "=0.17.0"
soft-fido2-crypto = "=0.17.0"
serde = { version = "1", features = ["derive"] }
openssl = "0.10"
''')
fixtures = HERE.parent / 'fixtures'
runner = ['cargo', 'run', '--manifest-path', str(cache / 'Cargo.toml'), '--']
if args.generate:
    subprocess.run(runner + ['generate', str(fixtures)], check=True)
with tempfile.TemporaryDirectory(prefix='keycord-passless-output-') as output:
    subprocess.run(['cargo', 'run', '--locked', '-p', 'keycord-passkey', '--no-default-features', '--features', 'passless', '--example', 'passless_interop', '--', str(fixtures), output], cwd=ROOT, check=True)
    subprocess.run(runner + ['verify', str(fixtures), output], check=True)
