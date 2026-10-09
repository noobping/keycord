#!/usr/bin/env python3
"""Compile the untouched Android credential implementation outside its checkout.

Needs Java 21+ and Cargo. Downloads pinned JVM test tools to a temporary cache.
Only --generate rewrites the synthetic Keycord fixtures; Android is always read-only.
"""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile
import urllib.request

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--android', type=Path, default=ROOT / 'other_projects/Android-Password-Store')
parser.add_argument('--generate', action='store_true')
args = parser.parse_args()
cache = Path(tempfile.gettempdir()) / 'keycord-android-interop-tools'
cache.mkdir(exist_ok=True)
# These standalone tools compile the reference source; no Android build files are changed.
artifacts = [
    ('org.jetbrains.kotlin', 'kotlin-compiler-embeddable', '2.2.0'),
    ('org.jetbrains.kotlin', 'kotlin-stdlib', '2.2.0'),
    ('org.jetbrains.kotlin', 'kotlin-script-runtime', '2.2.0'),
    ('org.jetbrains.kotlin', 'kotlin-reflect', '1.6.10'),
    ('org.jetbrains.kotlin', 'kotlin-daemon-embeddable', '2.2.0'),
    ('org.jetbrains.kotlin', 'kotlin-serialization-compiler-plugin-embeddable', '2.2.0'),
    ('org.jetbrains.kotlinx', 'kotlinx-coroutines-core-jvm', '1.8.0'),
    ('org.jetbrains.kotlinx', 'kotlinx-serialization-core-jvm', '1.9.0'),
    ('org.jetbrains.kotlinx', 'kotlinx-serialization-cbor-jvm', '1.9.0'),
    ('org.jetbrains.kotlinx', 'kotlinx-serialization-json-jvm', '1.9.0'),
    ('org.jetbrains', 'annotations', '13.0'),
    ('com.michael-bull.kotlin-result', 'kotlin-result-jvm', '2.1.0'),
    ('org.bouncycastle', 'bcprov-jdk18on', '1.81'),
    ('org.bouncycastle', 'bcpkix-jdk18on', '1.81'),
    ('org.bouncycastle', 'bcutil-jdk18on', '1.81'),
]
jars = []
for group, artifact, version in artifacts:
    filename = f'{artifact}-{version}.jar'
    jar = cache / filename
    if not jar.exists():
        url = f'https://repo.maven.apache.org/maven2/{group.replace(".", "/")}/{artifact}/{version}/{filename}'
        print(f'Downloading {filename}', flush=True)
        temporary = jar.with_suffix('.download')
        urllib.request.urlretrieve(url, temporary)
        temporary.replace(jar)
    jars.append(str(jar))
classpath = os.pathsep.join(jars)
source = args.android.resolve() / 'app/src/main/java/app/passwordstore/util/passkey'
fixtures = HERE.parent / 'fixtures'
with tempfile.TemporaryDirectory(prefix='keycord-android-interop-') as temporary:
    build = Path(temporary)
    output = build / 'output'
    output.mkdir()
    subprocess.run([
        'java', '-cp', classpath, 'org.jetbrains.kotlin.cli.jvm.K2JVMCompiler',
        '-no-stdlib', '-no-reflect', '-jvm-target', '17', '-classpath', classpath,
        f'-Xplugin={cache / "kotlin-serialization-compiler-plugin-embeddable-2.2.0.jar"}',
        '-d', str(build / 'interop.jar'), str(source / 'PasskeyCredential.kt'),
        str(source / 'Algorithm.kt'), str(HERE / 'Wipe.kt'), str(HERE / 'Interop.kt'),
    ], check=True)
    java = ['java', '-cp', str(build / 'interop.jar') + os.pathsep + classpath, 'InteropKt']
    if args.generate:
        subprocess.run(java + ['generate', str(fixtures)], check=True)
    subprocess.run(['cargo', 'run', '--locked', '-p', 'keycord-passkey', '--features', 'passkey',
                    '--example', 'android_interop', '--', str(fixtures), str(output)], cwd=ROOT, check=True)
    subprocess.run(java + ['verify', str(fixtures), str(output)], check=True)
