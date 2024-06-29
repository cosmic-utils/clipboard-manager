# Clipboard manager for COSMIC™

![screenshot of the applet](https://media.githubusercontent.com/media/wiiznokes/clipboard-manager/master/res/screenshots/main_popup.png)

The goal is to make a simple yet fast clipboard history, with a focus on UX, rapidity and security.

There is a quick settings popup when you right click the icon.

## Install

### Fedora

You can use this [copr](https://copr.fedorainfracloud.org/coprs/wiiznokes/cosmic-applets-unofficial/).

```sh
sudo dnf copr enable wiiznokes/cosmic-applets-unofficial
sudo dnf install clipboard-manager
```

### Other distros

```sh
git clone https://github.com/wiiznokes/clipboard-manager.git
cd clipboard-manager
sudo apt install libsqlite3-dev # build deps
just build-release
sudo apt install sqlite3 # runtime deps
sudo just install
```

You curently need to activate a setting of the compositor:

```sh
sudo nano /etc/cosmic-comp/config.ron
```

And change `data_control_enabled: false` to `true` at the end of the file.

Obiously, a better integration is planned, maybe with a portal that ask the user if they want to activate this protocol (which is insecure since its let an app access the clipboard without receiving an event for it (like ctrl-c) or being focused).

Finally, you will need to set up the applet in cosmic-settings.

## Logs

```sh
journalctl -p 3 -xb --user _EXE=/usr/bin/clipboard-manager | less
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
