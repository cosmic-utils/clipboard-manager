rootdir := ''
prefix := '/usr'
debug := '0'
name := 'cosmic-ext-applet-clipboard-manager'
appid := 'io.github.wiiznokes.' + name
cargo-target-dir := env('CARGO_TARGET_DIR', 'target')
bin-src := cargo-target-dir / if debug == '1' { 'debug' / name } else { 'release' / name }
base-dir := absolute_path(clean(rootdir / prefix))
share-dst := base-dir / 'share'
bin-dst := base-dir / 'bin' / name
desktop-dst := share-dst / 'applications' / appid + '.desktop'
metainfo-dst := share-dst / 'metainfo' / appid + '.metainfo.xml'
icon-dst := share-dst / 'icons/hicolor/scalable/apps' / appid + '-symbolic.svg'
env-dst := rootdir / 'etc/profile.d' / name + '.sh'
schema-dst := share-dst / 'configurator' / appid + '.json'

default: build-release

mold:
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=clang RUSTFLAGS="-C link-arg=-fuse-ld=/usr/bin/mold" just build-debug

build-debug *args:
    cargo build {{ args }}

build-release *args:
    cargo build --release {{ args }}

install:
    install -Dm0755 {{ bin-src }} {{ bin-dst }}
    install -Dm0644 res/desktop_entry.desktop {{ desktop-dst }}
    install -Dm0644 res/metainfo.xml {{ metainfo-dst }}
    install -Dm0644 res/app_icon.svg {{ icon-dst }}

install-env:
    install -Dm0644 res/env.sh {{ env-dst }}

install-schema:
    install -Dm0644 res/config_schema.json {{ schema-dst }}

uninstall:
    rm {{ bin-dst }} || true
    rm {{ desktop-dst }} || true
    rm {{ icon-dst }} || true
    rm {{ env-dst }} || true
    rm {{ schema-dst }} || true
    rm {{ metainfo-dst }} || true

clean:
    cargo clean

###################  Format / Test

pull: fmt prettier fix test

test:
    cargo test --workspace --all-features

fix:
    cargo clippy --workspace --all-features --fix --allow-dirty --allow-staged

fmt:
    cargo fmt --all

prettier:
    # install on Debian: sudo snap install node --classic
    # npx is the command to run npm package, node is the runtime
    npx prettier -w .

metainfo-check:
    appstreamcli validate --pedantic --explain --strict res/metainfo.xml

################### Other

git-cache:
    git rm -rf --cached .
    git add .

expand:
    cargo expand

setup:
    rm -rf flatpak-builder-tools
    git clone https://github.com/flatpak/flatpak-builder-tools
    pip install aiohttp toml

sources-gen:
    python3 flatpak-builder-tools/cargo/flatpak-cargo-generator.py ./Cargo.lock -o cargo-sources.json

sdk-version := "24.08"

install-sdk:
    flatpak remote-add --if-not-exists --user flathub https://flathub.org/repo/flathub.flatpakrepo
    flatpak install --noninteractive --user flathub \
        org.freedesktop.Platform//{{ sdk-version }} \
        org.freedesktop.Sdk//{{ sdk-version }} \
        org.freedesktop.Sdk.Extension.rust-stable//{{ sdk-version }} \
        org.freedesktop.Sdk.Extension.llvm18//{{ sdk-version }}

uninstall-f:
    flatpak uninstall {{ appid }} -y || true

# deps: flatpak-builder git-lfs
build-and-install: uninstall-f
    rm -rf flatpak-out || true
    flatpak-builder \
      --verbose \
      --ccache \
      --user --install \
      --install-deps-from=flathub \
      --repo=repo \
      flatpak-out \
      {{ appid }}.json

run:
    RUST_LOG="warn,cosmic_ext_applet_clipboard_manager=debug" flatpak run {{ appid }}
