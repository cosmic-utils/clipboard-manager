# Clipboard manager for COSMIC™

![screenshot of the applet](https://media.githubusercontent.com/media/cosmic-utils/clipboard-manager/master/res/screenshots/main_popup.png)

The goal is to make a simple yet fast clipboard history, with a focus on UX, rapidity and security.

There is a quick settings popup when you right click the icon.

## Install

> [!WARNING]  
> The applet is available in the cosmic-store, however, this version will currently not work (https://github.com/cosmic-utils/clipboard-manager/issues/171).

The reason is because this applet use a wayland protocol ([data control protocol](https://wayland.app/protocols/ext-data-control-v1)) which is not available for sandboxed client.
The only way is to build from source. For this you need to install [rust](https://rust-lang.org/tools/install/), [just](https://github.com/casey/just), and follow the the [build instruction](./BUILD.md).

## Logs

```sh
journalctl -p 3 -xb --user _EXE=/usr/bin/cosmic-ext-applet-clipboard-manager | less
```

- `-p` 3 means priority error
- `-x` add information
- `b` means since last boot

## Testing bundle

```sh
# install
flatpak install --user clipboard-manager.flatpak
# run specific branch
flatpak run --branch=testing io.github.cosmic_utils.cosmic-ext-applet-clipboard-manager
# to be sure cosmic-panel will launch the wanted version
flatpak uninstall --user io.github.cosmic_utils.cosmic-ext-applet-clipboard-manager//master
# or verify the commit with
flatpak run io.github.cosmic_utils.cosmic-ext-applet-clipboard-manager -V
# uninstall testing repo and app
flatpak remote-delete --user cosmic-ext-applet-clipboard-manager-origin
```

## Build from source

Instructions are in [this file](./BUILD.md).

## Contributing

See [this file](./CONTRIBUTING.md).
