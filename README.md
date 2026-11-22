![Goat Hooker is a open-source Git hooks manager](https://raw.githubusercontent.com/zuygui/goat-hooker/refs/heads/main/.github/assets/banner-light.svg#gh-light-mode-only)
![Goat Hooker is a open-source Git hooks manager](https://raw.githubusercontent.com/zuygui/goat-hooker/refs/heads/main/.github/assets/banner-dark.svg#gh-dark-mode-only)

# Goat Hooker

> Goat-Hooker is a free, open-source and easy-to-use Git Hooks Manager to enhance your projects.

> [!IMPORTANT] 
> The project still in development and is not ready to use in production.

## Features 🚀

- [x] Easy to write/read configuration files
- [x] Mono-repos support
- [x] Shell commands support
- [x] All types of Git Hooks supported
- [x] Run `pre-commit` hooks only on the staged files they target
- [ ] Remote hooks (`from`) and hooks files (`path`)

## Installation 📦

```sh
cargo install --path .
```

This installs the `hooker` binary.

## Usage 🐐

```sh
hooker init        # create hooker.config.json
hooker install     # install the Git hooks in .git/hooks (or core.hooksPath)
hooker run pre-commit   # run manually the hooks of a trigger
hooker completion zsh > ~/.zfunc/_hooker   # shell completion
```

`hooker install` doesn't overwrite hooks it didn't install, use `--force` for that.

## Configuration ⚙️

`hooker.config.json` at the root of the repository:

```json
{
  "imports": ["api", "front"],
  "hooks": [
    {
      "name": "fmt",
      "trigger": "pre-commit",
      "include": ["src/**/*.rs"],
      "exclude": ["src/generated/*.rs"],
      "cmd": "cargo fmt --check"
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
| `imports` | Sub-projects folders, each one with its own `hooker.config.json` |
| `name`    | Hook name |
| `trigger` | Git hook: `pre-commit`, `prepare-commit-msg`, `commit-msg`, `post-commit`, `pre-rebase`, `post-rewrite`, `pre-push`, `applypatch-msg`, `pre-applypatch`, `post-applypatch`, `pre-auto-gc`, `post-checkout`, `post-merge` |
| `include` | Glob patterns, relative to the config folder. Without it, the hook targets all files |
| `exclude` | Glob patterns of files to ignore |
| `cmd` / `cmds` | Shell command(s) to run, from the config folder. Git arguments are available as `$1`, `$2`... |

For `pre-commit`, a hook only runs if at least one staged file matches it. `*` doesn't cross folders, use `**` for that.

## Thanks to all contributors 🧡
<p align="center">
  <img src="https://contrib.rocks/image?repo=zuygui/goat-hooker" />
</p>
