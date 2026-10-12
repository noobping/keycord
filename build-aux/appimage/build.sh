#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
arch=$(uname -m)
case "$arch" in x86_64|aarch64) ;; *) exit 1 ;; esac
version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["workspace"]["package"]["version"])')
version=${APPIMAGE_VERSION:-v$version}
app_id=io.github.noobping.keycord
appdir="$PWD/AppDir"
builddir="$PWD/target/appimage-build"
tools_dir="$PWD/target/appimage-tools"
mkdir -p "$tools_dir"

# Match the standalone feature set without its installer/updater: an AppImage
# must not copy its inner executable to the host or update a mounted bundle.
meson setup "$builddir" --prefix=/usr --libdir=lib \
  -Dcargo_variant=release -Dcargo_features=docs,audit,smartcard,passkey,passless,logging \
  -Dsetup=false
meson compile -C "$builddir"
rm -rf "$appdir"
DESTDIR="$appdir" meson install -C "$builddir" --no-rebuild
install -m755 build-aux/appimage/AppRun "$appdir/AppRun"
ln -s "usr/share/applications/$app_id.desktop" "$appdir/$app_id.desktop"
ln -s "usr/share/icons/hicolor/scalable/apps/$app_id.svg" "$appdir/$app_id.svg"
ln -s "$app_id.svg" "$appdir/.DirIcon"
# These services cannot activate a relocatable image after its mount is gone.
rm -rf "$appdir/usr/share/dbus-1" "$appdir/usr/share/gnome-shell/search-providers"

multiarch=$(gcc -print-multiarch)
mkdir -p "$appdir/usr/lib/gio/modules" "$appdir/usr/lib/gdk-pixbuf-loaders"
cp -a /usr/lib/"$multiarch"/gio/modules/. "$appdir/usr/lib/gio/modules/"
cp -a /usr/lib/"$multiarch"/gdk-pixbuf-2.0/2.10.0/loaders/*.so "$appdir/usr/lib/gdk-pixbuf-loaders/"
install -m755 /usr/lib/"$multiarch"/gdk-pixbuf-2.0/gdk-pixbuf-query-loaders "$appdir/usr/bin/"
cp /usr/share/glib-2.0/schemas/*.xml "$appdir/usr/share/glib-2.0/schemas/"
glib-compile-schemas --strict "$appdir/usr/share/glib-2.0/schemas"
cp -a /usr/share/fontconfig "$appdir/usr/share/"
cp -a /usr/share/mime/. "$appdir/usr/share/mime/"
update-mime-database "$appdir/usr/share/mime"
mkdir -p "$appdir/etc/ssl/certs"
cp -rL /etc/fonts "$appdir/etc/"
install -m644 /etc/ssl/certs/ca-certificates.crt "$appdir/etc/ssl/certs/"
python3 build-aux/appimage/bundle-git.py "$appdir"
python3 build-aux/appimage/bundle-libraries.py "$appdir"

tool="$tools_dir/appimagetool-$arch.AppImage"
if [[ ! -f "$tool" ]]; then
  curl --fail --location --retry 3 \
    "https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-$arch.AppImage" -o "$tool"
  chmod +x "$tool"
fi
output="keycord-$version-$arch.AppImage"
ARCH="$arch" VERSION="$version" APPIMAGE_EXTRACT_AND_RUN=1 "$tool" \
  --comp zstd \
  --updateinformation "gh-releases-zsync|noobping|keycord|latest|keycord-*-$arch.AppImage.zsync" \
  "$appdir" "$PWD/$output"
sha256sum "$output" > "$output.sha256"
printf 'Built %s\n' "$output"
