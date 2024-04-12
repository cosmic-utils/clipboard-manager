
APP_ID := "com.wiiznokes.CosmicClipboardManager"

install:
	cargo build -r
	sudo install -Dm0755 ./target/release/cosmic-clipboard-manager /usr/bin/cosmic-clipboard-manager
	sudo install -Dm0644 resources/{{APP_ID}}.desktop /usr/share/app_IDlications/com.wiiznokes.CosmicClipboardManager.desktop
	sudo install -Dm0644 resources/icons/assignment24.svg /usr/share/{{APP_ID}}/icons/assignment24.svg


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
