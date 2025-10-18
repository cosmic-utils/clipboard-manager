# Clipboard manager for COSMIC™

![screenshot of the applet](https://media.githubusercontent.com/media/cosmic-utils/clipboard-manager/master/res/screenshots/main_popup.png)

The goal is to make a simple yet fast clipboard history, with a focus on UX, rapidity and security.

There is a quick settings popup when you right click the icon.

## Install

Use the flatpak version in the cosmic store.

You will need to enable the [data control protocol](https://wayland.app/protocols/ext-data-control-v1). It allow any privilegied client to access the clipboard, without any action from the user. It is thus kinda insecure.

The protocol is by default disabled on the COSMIC DE, but can be enabled with this command:

```sh
echo 'export COSMIC_DATA_CONTROL_ENABLED=1' | sudo tee /etc/profile.d/data_control_cosmic.sh > /dev/null
```

Restart the session for the `COSMIC_DATA_CONTROL_ENABLED` environment variable to take effect.

You can disable it with

```sh
sudo rm -f /etc/profile.d/data_control_cosmic.sh
```

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

## Contributing

Contributions are welcome

### Installing development libraries

You need to have the development version of `libxkbcommon` to build the crate
`smithay-client-toolkit`

```sh
dnf install -y libxkbcommon-devel # RPM-based
apt install -y libxkbcommon-dev # Debian-based
```

### Building from source

To build and install the debug build

```sh
just build-debug && sudo just debug=1 install && pkill cosmic-panel
```
