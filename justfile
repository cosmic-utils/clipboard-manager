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
env-dst := rootdir / 'etc/profile.d/data_control_cosmic.sh'
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

fmt-just:
    just --fmt --unstable

prettier:
    # install on Debian: sudo snap install node --classic
    # npx is the command to run npm package, node is the runtime
    npx prettier -w .

metainfo-check:
    appstreamcli validate --pedantic --explain --strict res/metainfo.xml

################### Flatpak

runf:
    RUST_LOG="warn,cosmic_ext_applet_clipboard_manager=debug" flatpak run {{ appid }}

uninstallf:
    flatpak uninstall {{ appid }} -y || true

update-flatpak: setup-update-flatpak update-flatpak-gen commit-update-flatpak

update-flatpak-test: setup-update-flatpak update-flatpak-gen build-and-installf runf

# deps: flatpak-builder git-lfs
build-and-installf: uninstallf
    rm -rf flatpak-out || true
    flatpak-builder \
      --verbose \
      --ccache \
      --user --install \
      --install-deps-from=flathub \
      --repo=repo \
      flatpak-out \
      {{ repo-name }}/app/{{ appid }}/{{ appid }}.json

sdk-version := "24.08"

install-sdk:
    flatpak remote-add --if-not-exists --user flathub https://flathub.org/repo/flathub.flatpakrepo
    flatpak install --noninteractive --user flathub \
        org.freedesktop.Platform//{{ sdk-version }} \
        org.freedesktop.Sdk//{{ sdk-version }} \
        org.freedesktop.Sdk.Extension.rust-stable//{{ sdk-version }} \
        org.freedesktop.Sdk.Extension.llvm18//{{ sdk-version }}

repo-name := "flatpak-repo"
branch-name := 'update-' + name

# pip install aiohttp toml
setup-update-flatpak:
    rm -rf {{ repo-name }}
    git clone https://github.com/wiiznokes/cosmic-flatpak.git {{ repo-name }}
    git -C {{ repo-name }} remote add upstream https://github.com/pop-os/cosmic-flatpak.git
    git -C {{ repo-name }} fetch upstream
    git -C {{ repo-name }} checkout master
    git -C {{ repo-name }} rebase upstream/master master
    git -C {{ repo-name }} push origin master

    git -C {{ repo-name }} branch -D {{ branch-name }} || true
    git -C {{ repo-name }} push origin --delete {{ branch-name }} || true
    git -C {{ repo-name }} checkout -b {{ branch-name }}
    git -C {{ repo-name }} push origin {{ branch-name }}

    rm -rf flatpak-builder-tools
    git clone https://github.com/flatpak/flatpak-builder-tools --branch master --depth 1

update-flatpak-gen:
    python3 flatpak-builder-tools/cargo/flatpak-cargo-generator.py Cargo.lock -o {{ repo-name }}/app/{{ appid }}/cargo-sources.json
    cp flatpak_schema.json {{ repo-name }}/app/{{ appid }}/{{ appid }}.json
    sed -i "s/###commit###/$(git rev-parse HEAD)/g" {{ repo-name }}/app/{{ appid }}/{{ appid }}.json

commit-update-flatpak:
    git -C {{ repo-name }} add .
    git -C {{ repo-name }} commit -m "Update {{ name }}"
    git -C {{ repo-name }} push origin {{ branch-name }}
    xdg-open https://github.com/pop-os/cosmic-flatpak/compare/master...wiiznokes:{{ branch-name }}?expand=1

################### Other

git-cache:
    git rm -rf --cached .
    git add .
