# clipboard manager for cosmic

![screenshot-2024-03-24-22-01-21](https://github.com/wiiznokes/cosmic-clipboard-manager/assets/78230769/db504da6-38d8-460e-afef-27ba5fa6101c)

The goal is to make a simple yet fast clipboard history, with a focus on UX, rapidity and security.

Currently support storing the history on disk, search, delete

Todo:

- image support
- auto remove old entries
- ...

Maybe take inspiration on this gnome extension: https://github.com/oae/gnome-shell-pano

## Install

```
git clone https://github.com/wiiznokes/cosmic-clipboard-manager.git
cd cosmic-clipboard-manager
just install
```

You curently need to activate a setting of the compositor:

```
sudo nano /etc/cosmic-comp/config.ron
```

And change `data_control_enabled: false` to `true` at the end of the file. Note that you may need to reset the setting at each update you make (at least, i noticied it on Fedora).

Obiously, a better integration is planned, maybe with a portal that ask the user if they want to activate this protocol (which is insecure since its let an app access the clipboard without receiving an event for it (like ctrl-c) or being focused).

Finally, you will need to set up the applet in cosmic-settings.

## Logs

```
journalctl -p 3 -xb --user _EXE=/usr/bin/cosmic-panel | grep com.wiiznokes.CosmicClipboardManager
```

-p 3 means priority error
-x add information
b means since last boot

## Contributing

Contributions are welcome
