# Installation

**CLI** (`hooker`):

```sh
brew install zuygui/tap/goat-hooker        # macOS, Linux
curl -fsSL https://raw.githubusercontent.com/zuygui/goat-hooker/main/install.sh | sh
cargo binstall goat-hooker                  # prebuilt, with cargo-binstall
cargo install goat-hooker                   # from the sources
```

or download the archive of your platform from the [latest release](https://github.com/zuygui/goat-hooker/releases/latest).

**Desktop app** with Homebrew: `brew install --cask zuygui/tap/goat-hooker`. The app tells when a new version is out.

**Desktop app**: download the installer of your platform from the [latest release](https://github.com/zuygui/goat-hooker/releases/latest):

| Platform | Installer | CLI included |
|----------|-----------|--------------|
| macOS (Apple Silicon / Intel) | `.dmg` | `Goat Hooker.app/Contents/MacOS/hooker` |
| Windows | `-setup.exe` | `hooker.exe` next to the app |
| Linux | `.deb` / `.AppImage` | `/usr/bin/hooker` (`.deb`) |

> [!NOTE]
> The installers aren't signed yet: on macOS, right click on the app → Open the first time; on Windows, SmartScreen → More info → Run anyway.

From the sources:

```sh
cargo install --path .        # the `hooker` CLI
cargo install --path gui      # the `goat-hooker-gui` desktop app
```

On Linux, the desktop app needs `libxkbcommon`, `wayland`, `vulkan`, `fontconfig` and `xcb` development packages to build.
