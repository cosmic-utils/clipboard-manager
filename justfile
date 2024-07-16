rootdir := ''
prefix := '/usr'
debug := '0'


export NAME := 'cosmic-ext-applet-clipboard-manager'
export APPID := 'io.github.wiiznokes.' + NAME 

bin-src := if debug == '1' { 'target/debug' / NAME } else { 'target/release' / NAME }

base-dir := absolute_path(clean(rootdir / prefix))
share-dst := base-dir / 'share'
etc-dir := absolute_path(clean(rootdir / 'etc'))

bin-dst := base-dir / 'bin' / NAME
desktop-dst := share-dst / 'applications' / APPID + '.desktop'
icon-dst := share-dst / 'icons/hicolor/scalable/apps' / APPID + '-symbolic.svg'
env-dst := etc-dir / 'environment.d' / NAME + '.conf'


default: build-release


build-debug *args:
  cargo build {{args}}

build-release *args:
  cargo build --release {{args}}

install:
  install -Dm0755 {{bin-src}} {{bin-dst}}
  install -Dm0644 res/desktop_entry.desktop {{desktop-dst}}
  install -Dm0644 res/app_icon.svg {{icon-dst}}
  install -Dm0644 res/env.conf {{env-dst}}

uninstall:
  rm {{bin-dst}}
  rm {{desktop-dst}}
  rm {{icon-dst}}
  rm {{env-dst}}

clean:
  cargo clean


pull: fmt prettier fix
	

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
