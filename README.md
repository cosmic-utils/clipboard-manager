# Clipboard manager for COSMIC™

![screenshot of the applet](https://media.githubusercontent.com/media/wiiznokes/clipboard-manager/master/res/screenshots/main_popup.png)

The goal is to make a simple yet fast clipboard history, with a focus on UX, rapidity and security.

There is a quick settings popup when you right click the icon.

## Install

### Fedora

You can use this [copr](https://copr.fedorainfracloud.org/coprs/wiiznokes/cosmic-applets-unofficial/).

```sh
sudo dnf copr enable wiiznokes/cosmic-applets-unofficial
sudo dnf install cosmic-ext-applet-clipboard-manager
```

### Other distros

```sh
git clone https://github.com/wiiznokes/clipboard-manager.git
cd clipboard-manager
sudo apt install libsqlite3-dev sqlite3 just cargo
just build-release
sudo just install
```

_Write `COSMIC_DATA_CONTROL_ENABLED=1` in `/etc/environment`. (see [this issue](https://github.com/wiiznokes/clipboard-manager/issues/61))_

Reboot or restart the session for the `COSMIC_DATA_CONTROL_ENABLED=1` environment variable to take effect.

## Logs

```sh
journalctl -p 3 -xb --user _EXE=/usr/bin/cosmic-ext-applet-clipboard-manager | less
```

- `-p` 3 means priority error
- `-x` add information
- `b` means since last boot

## Contributing

Contributions are welcome

To build and install the debug build

```sh
just build-debug && sudo just debug=1 install
```
