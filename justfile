rootdir := ''
prefix := '/usr'
debug := '0'


export NAME := 'cosmic-ext-applet-clipboard-manager'
export APPID := 'io.github.wiiznokes.' + NAME 

bin-src := if debug == '1' { 'target/debug' / NAME } else { 'target/release' / NAME }

base-dir := absolute_path(clean(rootdir / prefix))
share-dst := base-dir / 'share'

bin-dst := base-dir / 'bin' / NAME
desktop-dst := share-dst / 'applications' / APPID + '.desktop'
icon-dst := share-dst / 'icons/hicolor/scalable/apps' / APPID + '-symbolic.svg'
env-dst := base-dir / 'lib/environment.d' / NAME + '.conf'
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
  install -Dm0644 res/env.conf {{env-dst}}

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
