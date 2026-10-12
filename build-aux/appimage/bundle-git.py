#!/usr/bin/env python3
"""Bundle Git and its helpers without exporting glibc to host child processes."""
import hashlib
import pathlib
import shutil
import subprocess
import sys

root = pathlib.Path(sys.argv[1]).resolve()
real = root / "usr/libexec/keycord-tools"
helpers = root / "usr/libexec/git-core"
real.mkdir(parents=True, exist_ok=True)
helpers.mkdir(parents=True, exist_ok=True)
source = pathlib.Path(subprocess.check_output(["git", "--exec-path"], text=True).strip())

def install_tool(origin, destination):
    content = origin.read_bytes()
    if not content.startswith(b"\x7fELF"):
        shutil.copy2(origin, destination, follow_symlinks=True)
        return
    name = hashlib.sha256(content).hexdigest()
    binary = real / name
    if not binary.exists():
        shutil.copy2(origin, binary, follow_symlinks=True)
    relative_root = "/".join([".."] * len(destination.parent.relative_to(root).parts))
    destination.write_text(f'''#!/bin/sh
set -eu
bundle=$(CDPATH= cd -- "$(dirname -- "$0")/{relative_root}" && pwd -P)
case "$(uname -m)" in
  x86_64) loader="$bundle/usr/lib/ld-linux-x86-64.so.2" ;;
  aarch64) loader="$bundle/usr/lib/ld-linux-aarch64.so.1" ;;
  *) exit 1 ;;
esac
unset LD_LIBRARY_PATH APPDIR
export GIT_EXEC_PATH="$bundle/usr/libexec/git-core"
export GIT_TEMPLATE_DIR="$bundle/usr/share/git-core/templates"
export GIT_SSL_CAINFO="${{GIT_SSL_CAINFO:-$bundle/etc/ssl/certs/ca-certificates.crt}}"
exec "$loader" --library-path "$bundle/usr/lib" --argv0 "$0" "$bundle/usr/libexec/keycord-tools/{name}" "$@"
''')
    destination.chmod(0o755)

install_tool(pathlib.Path(shutil.which("git")), root / "usr/bin/git")
for path in source.iterdir():
    if path.is_file():
        install_tool(path, helpers / path.name)
    elif path.is_dir():
        shutil.copytree(path, helpers / path.name, symlinks=False)
shutil.copytree("/usr/share/git-core", root / "usr/share/git-core", symlinks=False)
