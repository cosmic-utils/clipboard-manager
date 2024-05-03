name := 'cosmic-clipboard-manager'
export APPID := 'com.wiiznokes.CosmicClipboardManager'

rootdir := ''
prefix := '/usr'

base-dir := absolute_path(clean(rootdir / prefix))

export INSTALL_DIR := base-dir / 'share'

bin-src := 'target' / 'release' / name
bin-dst := base-dir / 'bin' / name

desktop := APPID + '.desktop'
desktop-src := 'res' / desktop
desktop-dst := clean(rootdir / prefix) / 'share' / 'applications' / desktop

metainfo := APPID + '.metainfo.xml'
metainfo-src := 'res' / metainfo
metainfo-dst := clean(rootdir / prefix) / 'share' / 'metainfo' / metainfo

res-src := 'res'
res-dst := clean(rootdir / prefix) / 'share' / APPID

# Default recipe which runs `just build-release`
default: build-release

clean:
  cargo clean



# Compiles with debug profile
build-debug *args:
  cargo build {{args}}

# Compiles with release profile
build-release *args:
  cargo build --release {{args}}

install:
  install -Dm0755 {{bin-src}} {{bin-dst}}
  install -Dm0644 {{desktop-src}} {{desktop-dst}}
  install -Dm0644 {{res-src}}/icons/assignment24.svg {{res-dst}}/icons/assignment24.svg

# Uninstalls installed files
uninstall:
  rm {{bin-dst}}
  rm {{desktop-dst}}
  rm -r {{res-dst}}

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
