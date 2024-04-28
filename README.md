# Clipboard manager for cosmic

![screenshot of the applet](https://media.githubusercontent.com/media/wiiznokes/cosmic-clipboard-manager/master/resources/screenshots/main_popup.png)

The goal is to make a simple yet fast clipboard history, with a focus on UX, rapidity and security.

There is a quick settings popup when you right click the icon.

## Install

```
git clone https://github.com/wiiznokes/cosmic-clipboard-manager.git
cd cosmic-clipboard-manager
just
just install
```

You curently need to activate a setting of the compositor:

```
sudo nano /etc/cosmic-comp/config.ron
```

And change `data_control_enabled: false` to `true` at the end of the file.

Obiously, a better integration is planned, maybe with a portal that ask the user if they want to activate this protocol (which is insecure since its let an app access the clipboard without receiving an event for it (like ctrl-c) or being focused).

Finally, you will need to set up the applet in cosmic-settings.

## Logs

```
journalctl -p 3 -xb --user _EXE=/usr/bin/cosmic-panel | grep com.wiiznokes.CosmicClipboardManager | less
```

-p 3 means priority error
-x add information
b means since last boot

## Contributing

Contributions are welcome
