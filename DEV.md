## DEPS

#### Fedora
```sh
sudo dnf install libxkbcommon-devel -y
```

#### Logs

```
journalctl --user _EXE=/usr/bin/cosmic-session -r -S -10m | grep cosmic_clipboard_manager > ~/log.txt && code ~/log.txt
```
## INfO

clipboard crates:
- https://crates.io/crates/wl-clipboard-rs
- https://crates.io/crates/arboard


wayland protocol / official impl:
- https://wayland.app/protocols/wlr-data-control-unstable-v1
- https://github.com/bugaevc/wl-clipboard/tree/master/src


other app:
- https://github.com/mohamadzoh/clipop



discussions:
- https://github.com/pop-os/cosmic-epoch/issues/175
- https://www.reddit.com/r/pop_os/comments/194bugz/comment/khfxyoo/?utm_source=share&utm_medium=web3x&utm_name=web3xcss&utm_term=1&utm_content=share_button



links:
- https://github.com/pop-os/cosmic-launcher/blob/master/src/app.rs
- https://github.com/pop-os/cosmic-applets