rootdir := ''
prefix := '/usr'

name := 'clipboard-manager'
export APPID := 'io.github.wiiznokes.clipboard-manager'

base-dir := absolute_path(clean(rootdir / prefix))

bin-src := 'target' / 'release' / name
bin-dst := base-dir / 'bin' / name

desktop-src := 'res' / 'desktop_entry.desktop'
desktop-dst := clean(rootdir / prefix) / 'share' / 'applications' / APPID + '.desktop'

metainfo-src := 'res' / 'metainfo.xml'
metainfo-dst := clean(rootdir / prefix) / 'share' / 'metainfo' / APPID + '.metainfo.xml'

res-src := 'res'
res-dst := clean(rootdir / prefix) / 'share'

# Default recipe which runs `just build-release`
default: build-release


# Compiles with debug profile
build-debug *args:
  cargo build {{args}}

# Compiles with release profile
build-release *args:
  cargo build --release {{args}}

install:
  install -Dm0755 {{bin-src}} {{bin-dst}}
  install -Dm0644 {{desktop-src}} {{desktop-dst}}
  install -Dm0644 {{res-src}}/app_icon.svg {{res-dst}}/icons/hicolor/scalable/apps/{{APPID}}.svg

# Uninstalls installed files
uninstall:
  rm {{bin-dst}}
  rm {{desktop-dst}}
  rm {{res-dst}}/icons/hicolor/scalable/apps/{{APPID}}.svg


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
