## You need to activate the data control Wayland protocol on your device

The [data control protocol](https://wayland.app/protocols/ext-data-control-v1) is required for this applet to work. It allow any privilegied client to access the clipboard, without any action from the user. It is thus kinda insecure.

The protocol is by default disabled on the COSMIC DE, but can be enabled with this command:

```sh
echo 'export COSMIC_DATA_CONTROL_ENABLED=1' | sudo tee /etc/profile.d/data_control_cosmic.sh > /dev/null
```


sudo rm -f /etc/profile.d/data_control_cosmic.sh