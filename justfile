rootdir := ''
prefix := '/usr'
debug := '0'
name := 'cosmic-ext-applet-clipboard-manager'
appid := 'io.github.cosmic_utils.' + name
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
    rm -f {{ bin-dst }}
    rm -f {{ desktop-dst }} 
    rm -f {{ icon-dst }}
    rm -f {{ env-dst }}
    rm -f {{ schema-dst }}
    rm -f {{ metainfo-dst }}

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

################### Flatpak

run:
    RUST_LOG="warn,cosmic_ext_applet_clipboard_manager=debug" flatpak run {{ appid }}

uninstall-f:
    flatpak uninstall {{ appid }} -y || true

update-flatpak-all: setup-update-flatpak update-flatpak commit-update-flatpak

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

sdk-version := "24.08"

install-sdk:
    flatpak remote-add --if-not-exists --user flathub https://flathub.org/repo/flathub.flatpakrepo
    flatpak install --noninteractive --user flathub \
        org.freedesktop.Platform//{{ sdk-version }} \
        org.freedesktop.Sdk//{{ sdk-version }} \
        org.freedesktop.Sdk.Extension.rust-stable//{{ sdk-version }} \
        org.freedesktop.Sdk.Extension.llvm18//{{ sdk-version }}

# pip install aiohttp toml
setup-update-flatpak:
    rm -rf cosmic-flatpak
    git clone https://github.com/wiiznokes/cosmic-flatpak.git
    git -C cosmic-flatpak remote add upstream https://github.com/pop-os/cosmic-flatpak.git
    git -C cosmic-flatpak fetch upstream
    git -C cosmic-flatpak checkout master
    git -C cosmic-flatpak rebase upstream/master master
    git -C cosmic-flatpak push origin master

    git -C cosmic-flatpak branch -D update || true
    git -C cosmic-flatpak push origin --delete update || true
    git -C cosmic-flatpak checkout -b update
    git -C cosmic-flatpak push origin update

    rm -rf flatpak-builder-tools
    git clone https://github.com/flatpak/flatpak-builder-tools

update-flatpak:
    python3 flatpak-builder-tools/cargo/flatpak-cargo-generator.py Cargo.lock -o cosmic-flatpak/app/{{ appid }}/cargo-sources.json
    cp flatpak_schema.json cosmic-flatpak/app/{{ appid }}/{{ appid }}.json
    sed -i "s/###commit###/$(git rev-parse HEAD)/g" cosmic-flatpak/app/{{ appid }}/{{ appid }}.json

commit-update-flatpak:
    git -C cosmic-flatpak add .
    git -C cosmic-flatpak commit -m "Update clipboard manager"
    git -C cosmic-flatpak push origin update
    xdg-open https://github.com/pop-os/cosmic-flatpak/compare/master...wiiznokes:update?expand=1

################### Other

git-cache:
    git rm -rf --cached .
    git add .
