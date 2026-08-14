project(
  'keycord',
  version: '0.9.12',
  meson_version: '>=1.1.0',
)

cargo = find_program('cargo', required: false)
flatpak_spawn = find_program('flatpak-spawn', required: false)
shell = find_program('sh')
gnome = import('gnome')

cargo_variant = get_option('cargo_variant')
cargo_features = get_option('cargo_features')
setup = get_option('setup')
docs = get_option('docs')
logging = get_option('logging')
audit = get_option('audit')
fidokey = get_option('fidokey')
passkey = get_option('passkey')
fidopin = get_option('fidopin')
smartcard = get_option('smartcard')
hardwarekey = get_option('hardwarekey')
project_binary = 'keycord'
cargo_command = 'cargo build --package keycord'

effective_cargo_features = cargo_features
foreach feature_name, enabled : {
  'setup': setup,
  'docs': docs,
  'logging': logging,
  'audit': audit,
  'fidokey': fidokey,
  'passkey': passkey,
  'fidopin': fidopin,
  'smartcard': smartcard,
  'hardwarekey': hardwarekey,
}
  if enabled
    if effective_cargo_features == ''
      effective_cargo_features = feature_name
    else
      effective_cargo_features += ' ' + feature_name
    endif
  endif
endforeach

if cargo_variant == 'release' or cargo_variant == 'translation'
  cargo_command = 'cargo build --package keycord --release'
endif

message('Configured Meson wrapper variant "@0@" -> @1@'.format(cargo_variant, cargo_command))
if effective_cargo_features != ''
  message('Configured Meson wrapper features "@0@"'.format(effective_cargo_features))
endif

native_cargo = ''
flatpak_spawn_path = ''
if cargo.found()
  native_cargo = cargo.full_path()
  message('Configured Meson wrapper cargo source "native cargo" -> @0@'.format(native_cargo))
elif flatpak_spawn.found()
  flatpak_spawn_path = flatpak_spawn.full_path()
  message('Configured Meson wrapper cargo source "flatpak-spawn --host toolbox run cargo"')
else
  error('Neither cargo nor flatpak-spawn was found. Install cargo in the build environment or provide flatpak-spawn access to host toolbox.')
endif

if flatpak_spawn.found()
  flatpak_spawn_path = flatpak_spawn.full_path()
endif

cargo_wrapper_script = '''
set -eu

native_cargo=$1
flatpak_spawn=$2
source_root=$3
build_root=$4
variant=$5
features=$6
output=$7

profile_dir=debug

if [ "$variant" = "release" ] || [ "$variant" = "translation" ]; then
  profile_dir=release
fi

if [ -n "$native_cargo" ]; then
  set -- "$native_cargo" build "--package" "keycord" "--manifest-path" "$source_root/Cargo.toml" "--target-dir" "$build_root/cargo-target" "--locked" "--no-default-features"
else
  set -- "$flatpak_spawn" --host toolbox run cargo build "--package" "keycord" "--manifest-path" "$source_root/Cargo.toml" "--target-dir" "$build_root/cargo-target" "--locked" "--no-default-features"
fi

if [ "$profile_dir" = "release" ]; then
  set -- "$@" --release
fi

if [ -n "$features" ]; then
  set -- "$@" --features "$features"
fi

"$@"

mkdir -p "$(dirname "$output")"
cp "$build_root/cargo-target/$profile_dir/keycord" "$output"
chmod +x "$output"
'''

cargo_wrapper_args = [
  shell,
  '-c',
  cargo_wrapper_script,
  'meson-cargo-toolbox',
  native_cargo,
  flatpak_spawn_path,
  meson.project_source_root(),
  meson.project_build_root(),
]

custom_target(
  'keycord-default-build',
  output: project_binary,
  command: cargo_wrapper_args + [
    cargo_variant,
    effective_cargo_features,
    '@OUTPUT@',
  ],
  build_always_stale: true,
  build_by_default: true,
  console: true,
  install: true,
  install_dir: get_option('bindir'),
)

foreach variant : ['developer', 'release', 'translation']
  run_target(
    variant + '-build',
    command: cargo_wrapper_args + [
      variant,
      effective_cargo_features,
      join_paths(meson.project_build_root(), 'artifacts', variant, project_binary),
    ],
  )
endforeach

lifecycle_metadata = configuration_data()
lifecycle_metadata.set('APP_ID', 'io.github.noobping.keycord')
lifecycle_metadata.set('BUS_NAME', 'io.github.noobping.keycord.SearchProvider')
lifecycle_metadata.set('OBJECT_PATH', '/io/github/noobping/keycord/SearchProvider')
lifecycle_metadata.set('DISPLAY_NAME', 'Keycord')
lifecycle_metadata.set('COMMENT', 'Browse and edit password stores')
lifecycle_metadata.set('EXECUTABLE', project_binary)
if passkey
  lifecycle_metadata.set('OPEN_ARGUMENT', ' %f')
  lifecycle_metadata.set('MIME_TYPES', 'MimeType=application/vnd.keycord.passkey-request+json;application/vnd.keycord.passkey+json;')
else
  lifecycle_metadata.set('OPEN_ARGUMENT', '')
  lifecycle_metadata.set('MIME_TYPES', '')
endif

desktop_file = configure_file(
  input: 'crates/keycord-lifecycle/data/keycord.desktop.in',
  output: 'keycord.desktop',
  configuration: lifecycle_metadata,
)

search_provider_file = configure_file(
  input: 'crates/keycord-lifecycle/data/keycord-search-provider.ini.in',
  output: 'keycord-search-provider.ini',
  configuration: lifecycle_metadata,
)

search_provider_service = configure_file(
  input: 'crates/keycord-lifecycle/data/keycord-search-provider.service.in',
  output: 'keycord-search-provider.service',
  configuration: lifecycle_metadata,
)

install_data(
  'crates/keycord-lifecycle/data/branding/scalable/apps/io.github.noobping.keycord.svg',
  install_dir: join_paths(get_option('datadir'), 'icons', 'hicolor', 'scalable', 'apps'),
)

install_data(
  'crates/keycord-lifecycle/data/branding/scalable/apps/io.github.noobping.keycord-beta.svg',
  install_dir: join_paths(get_option('datadir'), 'icons', 'hicolor', 'scalable', 'apps'),
)

install_data(
  'crates/keycord-lifecycle/data/branding/256x256/apps/io.github.noobping.keycord.png',
  install_dir: join_paths(get_option('datadir'), 'icons', 'hicolor', '256x256', 'apps'),
)

install_data(
  'crates/keycord-lifecycle/data/branding/256x256/apps/io.github.noobping.keycord-beta.png',
  install_dir: join_paths(get_option('datadir'), 'icons', 'hicolor', '256x256', 'apps'),
)

install_subdir(
  'crates/keycord-shell/data/symbolic/apps',
  install_dir: join_paths(get_option('datadir'), 'icons', 'hicolor', 'symbolic'),
)

install_subdir(
  'crates/keycord-lifecycle/data/branding/symbolic/apps',
  install_dir: join_paths(get_option('datadir'), 'icons', 'hicolor', 'symbolic'),
)

install_data(
  'crates/keycord-git/data/symbolic/apps/git-symbolic.svg',
  install_dir: join_paths(get_option('datadir'), 'icons', 'hicolor', 'symbolic', 'apps'),
)

install_data(
  desktop_file,
  rename: 'io.github.noobping.keycord.desktop',
  install_dir: join_paths(get_option('datadir'), 'applications'),
)

if passkey
  install_data(
    'crates/keycord-passkey/data/io.github.noobping.keycord-passkey.xml',
    install_dir: join_paths(get_option('datadir'), 'mime', 'packages'),
  )
endif

install_data(
  search_provider_file,
  rename: 'io.github.noobping.keycord.search-provider.ini',
  install_dir: join_paths(get_option('datadir'), 'gnome-shell', 'search-providers'),
)

install_data(
  search_provider_service,
  rename: 'io.github.noobping.keycord.SearchProvider.service',
  install_dir: join_paths(get_option('datadir'), 'dbus-1', 'services'),
)

install_data(
  'crates/keycord-preferences/data/io.github.noobping.keycord.gschema.xml',
  rename: 'io.github.noobping.keycord.gschema.xml',
  install_dir: join_paths(get_option('datadir'), 'glib-2.0', 'schemas'),
)

install_data(
  'crates/keycord-lifecycle/data/metainfo.xml',
  rename: 'io.github.noobping.keycord.metainfo.xml',
  install_dir: join_paths(get_option('datadir'), 'metainfo'),
)

gnome.post_install(
  glib_compile_schemas: true,
  update_mime_database: passkey,
)
