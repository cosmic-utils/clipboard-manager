

install target='debug':
	sudo install -Dm0755 ./target/{{target}}/cosmic-clipboard-manager /usr/bin/cosmic-clipboard-manager
	sudo install -Dm0644 resources/com.wiiznokes.CosmicClipboardManager.desktop /usr/share/applications/com.wiiznokes.CosmicClipboardManager.desktop




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