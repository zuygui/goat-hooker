![Goat Hooker is a open-source Git hooks manager](https://raw.githubusercontent.com/zuygui/goat-hooker/refs/heads/main/.github/assets/banner-light.svg#gh-light-mode-only)
![Goat Hooker is a open-source Git hooks manager](https://raw.githubusercontent.com/zuygui/goat-hooker/refs/heads/main/.github/assets/banner-dark.svg#gh-dark-mode-only)

# Goat Hooker

> Goat-Hooker is a free, open-source and easy-to-use Git Hooks Manager to enhance your projects.

> [!IMPORTANT] 
> The project still in development and is not ready to use in production.

## Features 🚀

- [x] Easy to write/read configuration files
- [x] Mono-repos support: each sub-project owns its config, with glob imports (`packages/*`) or automatic discovery
- [x] Shell commands support
- [x] All types of Git Hooks supported
- [x] Run `pre-commit` hooks only on the staged files they target, `pre-push` hooks only on the pushed files they target
- [x] Pass the targeted files to commands (`{files}` / `$HOOKER_FILES`)
- [x] Pre-commit hooks only see what is going to be committed (unstaged changes are put aside)
- [x] Re-stage files fixed by formatters (`stage_fixed`)
- [x] Run hooks in parallel
- [x] Shared hooks files, local or from a git repository (`path` / `from`)
- [x] JSON schema for autocompletion in editors
- [x] A desktop app for those who prefer not to use the CLI

## Installation 📦

```sh
cargo install --path .        # the `hooker` CLI
cargo install --path gui      # the `goat-hooker-gui` desktop app
```

On Linux, the desktop app needs `libxkbcommon`, `wayland`, `vulkan`, `fontconfig` and `xcb` development packages to build.

## Usage 🐐

```sh
hooker init        # create hooker.config.json
hooker install     # install the Git hooks in .git/hooks (or core.hooksPath)
hooker run pre-commit   # run manually the hooks of a trigger
hooker run pre-commit --dry-run   # show the hooks that would run, and on which files
hooker list        # list the configurations and their hooks
hooker update      # update the hooks loaded from git repositories
hooker schema      # print the JSON schema of the configuration
hooker completion zsh > ~/.zfunc/_hooker   # shell completion
```

`hooker install` doesn't overwrite hooks it didn't install, use `--force` for that.

`HOOKER_SKIP=1 git commit` skips the hooks.

## Desktop app 🖥️

`goat-hooker-gui [repository]` opens a repository in a desktop app (built with [GPUI](https://www.gpui.rs) and [GPUI Kit](https://gpui-kit.com)):

- browse the hooks by trigger or by project of the mono-repo
- see which hooks will run on the next commit, and on which staged files
- run a hook, or the whole pre-commit, and read its output
- add, edit and delete hooks without touching the JSON
- install the Git hooks

## Configuration ⚙️

`hooker.config.json` at the root of the repository:

```json
{
  "$schema": "https://raw.githubusercontent.com/zuygui/goat-hooker/main/schema.json",
  "imports": ["api", "front"],
  "hooks": [
    {
      "name": "fmt",
      "trigger": "pre-commit",
      "include": ["src/**/*.rs"],
      "exclude": ["src/generated/*.rs"],
      "cmd": "rustfmt --check {files}"
    },
    {
      "name": "commit message",
      "trigger": "commit-msg",
      "cmds": ["grep -qE '^(feat|fix|chore|docs|refactor|test|ci)' \"$1\""]
    }
  ]
}
```

| Field     | Description |
|-----------|-------------|
| `imports` | Sub-projects folders, each one with its own `hooker.config.json`. Globs are supported (`packages/*`) |
| `discover` | `true` to load every `hooker.config.json` of the repository (files ignored by git are skipped) |
| `name`    | Hook name |
| `trigger` | Git hook: `pre-commit`, `prepare-commit-msg`, `commit-msg`, `post-commit`, `pre-rebase`, `post-rewrite`, `pre-push`, `applypatch-msg`, `pre-applypatch`, `post-applypatch`, `pre-auto-gc`, `post-checkout`, `post-merge` |
| `include` | Glob patterns, relative to the config folder. Without it, the hook targets all files |
| `exclude` | Glob patterns of files to ignore |
| `cmd` / `cmds` | Shell command(s) to run, from the config folder. Git arguments are available as `$1`, `$2`... |
| `stage_fixed` | `true` to re-stage the targeted files after the hook (formatters) |
| `path` | Hooks file to load (`{ "hooks": [...] }`), relative to the config folder |
| `from` / `rev` | Git repository (and branch or tag) to load `path` from, cloned once in `~/.cache/goat-hooker` |
| `parallel` | Root config only: `true` to run hooks in parallel |

`*` doesn't cross folders, use `**` for that.

## Mono-repos 🗂️

Each sub-project keeps its own `hooker.config.json` next to its code:

```
my-repo/
├── hooker.config.json          { "imports": ["packages/*"] }
└── packages/
    ├── api/hooker.config.json
    └── front/hooker.config.json
```

Or let goat-hooker find them all with `{ "discover": true }`.

For each sub-project:

- `include` / `exclude` patterns are relative to its folder
- commands run from its folder
- `pre-commit` hooks only run if a staged file belongs to it, `pre-push` hooks only if a pushed commit changed one of its files
- `{files}` in a command is replaced by the targeted files (relative to its folder, shell-escaped), also available in `$HOOKER_FILES` (one per line)

```json
{ "name": "lint", "trigger": "pre-commit", "include": ["src/**/*.ts"], "cmd": "eslint {files}" }
```

Other triggers (`commit-msg`, `post-merge`...) run all their hooks.

## Shared hooks 📦

Hooks can be shared between projects in a hooks file (`{ "hooks": [...] }`), loaded with `path`, from the repository or from another git repository:

```json
{
  "hooks": [
    { "name": "local rules", "path": "tools/hooks/rust.json" },
    { "name": "team rules", "from": "https://github.com/acme/hooks", "rev": "v1", "path": "rust.json" }
  ]
}
```

Loaded hooks run from the project that loads them. Repositories are cloned once, run `hooker update` to fetch their new version.

## Thanks to all contributors 🧡
<p align="center">
  <img src="https://contrib.rocks/image?repo=zuygui/goat-hooker" />
</p>
