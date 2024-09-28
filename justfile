rootdir := ''
prefix := '/usr'
debug := '0'


export NAME := 'cosmic-ext-applet-clipboard-manager'
export APPID := 'io.github.wiiznokes.' + NAME 

cargo-target-dir := env('CARGO_TARGET_DIR', 'target')
bin-src := cargo-target-dir / if debug == '1' { 'debug' / NAME } else { 'release' / NAME }

base-dir := absolute_path(clean(rootdir / prefix))
share-dst := base-dir / 'share'

bin-dst := base-dir / 'bin' / NAME
desktop-dst := share-dst / 'applications' / APPID + '.desktop'
icon-dst := share-dst / 'icons/hicolor/scalable/apps' / APPID + '-symbolic.svg'
env-dst := rootdir / 'etc/profile.d' / NAME + '.sh'
migrations-dst := share-dst / NAME / 'migrations'


default: build-release

build-debug *args:
  cargo build {{args}}

build-release *args:
  cargo build --release {{args}}

install: install-migrations
  install -Dm0755 {{bin-src}} {{bin-dst}}
  install -Dm0644 res/desktop_entry.desktop {{desktop-dst}}
  install -Dm0644 res/app_icon.svg {{icon-dst}}
  install -Dm0644 res/env.sh {{env-dst}}

install-migrations:
  #!/usr/bin/env sh
  set -ex
  for file in ./migrations/*; do
    install -Dm0644 $file "{{migrations-dst}}/$(basename "$file")"
  done
  

uninstall:
  rm {{bin-dst}}
  rm {{desktop-dst}}
  rm {{icon-dst}}
  rm {{env-dst}}
  rm -r {{share-dst}}/{{NAME}}

clean:
  cargo clean


pull: fmt prettier fix test
	

###################  Test

test:
	cargo test --workspace --all-features

###################  Format

fix:
	cargo clippy --workspace --all-features --fix --allow-dirty --allow-staged

fmt:
	cargo fmt --all

prettier:
	# install on Debian: sudo snap install node --classic
	# npx is the command to run npm package, node is the runtime
	npx prettier -w .




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

install-sdk:
  flatpak remote-add --if-not-exists --user flathub https://flathub.org/repo/flathub.flatpakrepo
  flatpak install --noninteractive --user flathub \
    org.freedesktop.Platform//23.08 \
    org.freedesktop.Sdk//23.08 \
    org.freedesktop.Sdk.Extension.rust-stable//23.08 \
    org.freedesktop.Sdk.Extension.llvm17//23.08

uninstall-f:
  flatpak uninstall io.github.wiiznokes.cosmic-ext-applet-clipboard-manager -y || true

# deps: flatpak-builder git-lfs
build-and-install: uninstall-f
  flatpak-builder \
    --force-clean \
    --verbose \
    --ccache \
    --user --install \
    --install-deps-from=flathub \
    --repo=repo \
    flatpak-out \
    io.github.wiiznokes.cosmic-ext-applet-clipboard-manager.json

run:
  flatpak run io.github.wiiznokes.cosmic-ext-applet-clipboard-manager
