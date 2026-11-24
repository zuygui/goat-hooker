# Quickstart

From nothing to hooks running on your next commit, in a minute.

## 1. Install

```sh
curl -fsSL https://raw.githubusercontent.com/zuygui/goat-hooker/main/install.sh | sh
```

Other ways (Homebrew, Cargo, the desktop app) are in the [installation guide](installation.md).

## 2. Create a config

In your repository:

```sh
hooker init
```

It offers the hooks of the catalogue, with the ones matching your project already picked (Rust, JavaScript, Python, common checks, conventional commits). `hooker init -t auto` picks them without asking. Already using husky, lefthook or pre-commit? `hooker import` converts their config instead.

You get a `hooker.toml`:

```toml
#:schema https://raw.githubusercontent.com/zuygui/goat-hooker/main/schema.json

[[hooks]]
name = "fmt"
trigger = "pre-commit"
include = ["**/*.rs"]
cmd = "rustfmt {files}"
stage_fixed = true

[[hooks]]
name = "clippy"
trigger = "pre-commit"
include = ["**/*.rs"]
cmd = "cargo clippy -- -D warnings"
cache = true
```

Each hook runs on the staged files matching `include`, `{files}` is replaced with them, and `stage_fixed` stages again what a formatter changed.

## 3. Plug it into Git

```sh
hooker install
```

## 4. Commit

```console
$ git commit -m "feat: add numbers"
🐐 pre-commit · 2 hooks
✔ fmt     root  81ms
✔ clippy  root  995ms

✔ 2 hooks passed in 1.10s
```

`hooker` alone shows what the next commit will run, and `hooker run pre-commit --dry-run` explains why each hook runs. To skip a hook once: `HOOKER_SKIP=clippy git commit`.

## 5. Open the app (optional)

```sh
goat-hooker-gui .
```

The app shows the hooks of the next commit, runs them with their output live, and edits them without touching the config file.

## Next

- [Configuration](configuration.md): every field of a hook
- [Mono-repos](monorepos.md): a config per project
- [The `hooker` CLI](cli.md): `watch`, `fix`, `doctor`, CI
