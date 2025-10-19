### Installing development libraries

#### rpm based

```sh
sudo dnf install libxkbcommon-devel
```

#### debian based

```sh
sudo apt install libxkbcommon-dev
```

### Debug build

```sh
just build-debug && sudo just debug=1 install
```

### Release build

```sh
just build-release && sudo just install
```

To test the build in the panel, `pkill cosmic-panel`.
