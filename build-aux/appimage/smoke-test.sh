#!/usr/bin/env bash
set -euo pipefail
image=$(realpath "$1")
work=$(mktemp -d)
app_pid=
cleanup() {
  if [[ -n "$app_pid" ]]; then
    kill "$app_pid" 2>/dev/null || true
    wait "$app_pid" 2>/dev/null || true
  fi
  cat "$work/app.log" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT
cd "$work"
timeout 60s "$image" --appimage-extract >/dev/null
# Relocation, including whitespace, must not break resource or helper paths.
mv squashfs-root 'Keycord AppDir'
bundle="$work/Keycord AppDir"
mkdir -p "$work/config" "$work/data" "$work/cache" "$work/gnupg"
chmod 700 "$work/gnupg"
export XDG_CONFIG_HOME="$work/config" GNUPGHOME="$work/gnupg"
export XDG_DATA_HOME="$work/data" XDG_CACHE_HOME="$work/cache"
export GDK_BACKEND=x11 GSETTINGS_BACKEND=memory

# The clean test container has no Git installation. Exercise the bundled Git
# and its subprocesses, including a file-protocol clone and a real commit.
git="$bundle/usr/bin/git"
"$git" --version
"$git" init "$work/store"
"$git" -C "$work/store" config user.name 'AppImage test'
"$git" -C "$work/store" config user.email 'appimage-test@example.invalid'
printf 'fixture\n' > "$work/store/fixture.txt"
"$git" -C "$work/store" add fixture.txt
"$git" -C "$work/store" commit -m 'Test bundled Git'
"$git" clone "file://$work/store" "$work/clone"
cmp "$work/store/fixture.txt" "$work/clone/fixture.txt"
test -s "$bundle/usr/share/locale/nl/LC_MESSAGES/keycord.mo"
test -s "$bundle/usr/share/glib-2.0/schemas/gschemas.compiled"
"$bundle/AppRun" > "$work/app.log" 2>&1 &
app_pid=$!
for attempt in {1..30}; do
  kill -0 "$app_pid" || { echo 'Keycord exited before showing its window' >&2; exit 1; }
  if timeout 5s xwininfo -root -tree | grep -qi keycord; then
    sleep 12
    kill -0 "$app_pid"
    if grep -Ei 'Failed to load icon|Unrecognized image file format|symbol lookup error|GLib-GIO-ERROR|Gtk-ERROR' "$work/app.log"; then
      echo 'AppImage startup failed' >&2
      exit 1
    fi
    echo 'Keycord AppImage window, resources and Git smoke test passed'
    exit 0
  fi
  sleep 1
done
echo 'No Keycord window appeared' >&2
exit 1
