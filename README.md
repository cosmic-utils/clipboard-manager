# Clipboard manager for COSMIC™

![screenshot of the applet](https://media.githubusercontent.com/media/cosmic-utils/clipboard-manager/master/res/screenshots/main_popup.png)

The goal is to make a simple yet fast clipboard history, with a focus on UX, rapidity and security.

There is a quick settings popup when you right click the icon.

## Install

Use the flatpak version in the cosmic store.


> [!NOTE]
> The applet is not currently displayed on the cosmic-store for some reason. Use this command to install it manually: 
> ```sh
> flatpak install cosmic io.github.cosmic_utils.cosmic-ext-applet-clipboard-manager
> ```

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

## Contributing

Contributions are welcome

To build and install the debug build

```sh
just build-debug && sudo just debug=1 install && pkill cosmic-panel
```
