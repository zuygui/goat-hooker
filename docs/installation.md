# Installation

Goat Hooker is two programs: the `hooker` CLI, which Git runs, and an optional desktop app.

## The CLI

```sh
# macOS and Linux
curl -fsSL https://raw.githubusercontent.com/zuygui/goat-hooker/main/install.sh | sh

# Homebrew (macOS, Linux)
brew install zuygui/tap/goat-hooker

# Cargo: prebuilt with cargo-binstall, or from the sources
cargo binstall goat-hooker
cargo install goat-hooker
```

Or download the archive of your platform from the [latest release](https://github.com/zuygui/goat-hooker/releases/latest) and put `hooker` in your `PATH`. Builds exist for Linux (x86_64), macOS (Apple Silicon and Intel) and Windows (x86_64).

Check it:

```sh
hooker --version
```

`hooker` must be in the `PATH` of the programs that run Git: your terminal, but also your editor or Git client. If a Git client says `hooker: not found`, start it from a terminal or add the folder of `hooker` to its `PATH`.

## The desktop app

```sh
brew install --cask zuygui/tap/goat-hooker
```

Or download the installer of your platform from the [latest release](https://github.com/zuygui/goat-hooker/releases/latest). It includes the CLI:

| Platform | Installer | CLI included |
|----------|-----------|--------------|
| macOS (Apple Silicon / Intel) | `.dmg` | `Goat Hooker.app/Contents/MacOS/hooker` |
| Windows | `-setup.exe` | `hooker.exe` next to the app |
| Linux | `.deb` / `.AppImage` | `/usr/bin/hooker` (`.deb`) |

> [!NOTE]
> The installers aren't signed yet: on macOS, right click on the app → Open the first time; on Windows, SmartScreen → More info → Run anyway.

The app tells you when a new version is out.

## Shell completion

```sh
hooker completion zsh > ~/.zfunc/_hooker          # zsh (with fpath+=~/.zfunc)
hooker completion bash > ~/.local/share/bash-completion/completions/hooker
hooker completion fish > ~/.config/fish/completions/hooker.fish
hooker completion powershell >> $PROFILE
```

## Updating

Update `hooker` the way you installed it (`brew upgrade`, `cargo binstall goat-hooker`, the install script again). The Git hooks call `hooker` by name, so nothing has to be installed again in your repositories.

## Uninstalling

In each repository, remove the Git hooks Goat Hooker installed (they start with `# installed by goat-hooker`):

```sh
grep -l "installed by goat-hooker" .git/hooks/* | xargs rm
```

Then remove the binary, and the cache of the shared hooks and tools (`~/.cache/goat-hooker`). Your `hooker.toml` stays in the repository.

## From the sources

```sh
git clone https://github.com/zuygui/goat-hooker && cd goat-hooker
cargo install --path .        # the `hooker` CLI
cargo install --path gui      # the `goat-hooker-gui` desktop app
```

Rust 1.85 or later. On Linux, the desktop app needs the development packages of `libxkbcommon`, `wayland`, `vulkan`, `fontconfig` and `xcb` to build.
