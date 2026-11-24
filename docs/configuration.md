# Configuration

`hooker.toml` at the root of the repository (`hooker.config.json` is still read, and `hooker.toml` wins when a folder has both):

```toml
#:schema https://raw.githubusercontent.com/zuygui/goat-hooker/main/schema.json

imports = ["api", "front"]

[[hooks]]
name = "fmt"
trigger = "pre-commit"
include = ["src/**/*.rs"]
exclude = ["src/generated/*.rs"]
cmd = "rustfmt --check {files}"

[[hooks]]
name = "commit message"
trigger = "commit-msg"
cmds = ["grep -qE '^(feat|fix|chore|docs|refactor|test|ci)' \"$1\""]
```

| Field     | Description |
|-----------|-------------|
| `imports` | Sub-projects folders, each one with its own `hooker.toml`. Globs are supported (`packages/*`) |
| `discover` | `true` to load every `hooker.toml` (or `hooker.config.json`) of the repository (files ignored by git are skipped) |
| `name`    | Hook name |
| `trigger` | Git hook: `pre-commit`, `prepare-commit-msg`, `commit-msg`, `post-commit`, `pre-rebase`, `post-rewrite`, `pre-push`, `applypatch-msg`, `pre-applypatch`, `post-applypatch`, `pre-auto-gc`, `post-checkout`, `post-merge` |
| `include` | Glob patterns, relative to the config folder. Without it, the hook targets all files |
| `exclude` | Glob patterns of files to ignore |
| `cmd` / `cmds` | Shell command(s) to run, from the config folder. Git arguments are available as `$1`, `$2`... |
| `stage_fixed` | `true` to re-stage the targeted files after the hook (formatters) |
| `path` | Hooks file to load (`{ "hooks": [...] }`), relative to the config folder |
| `from` / `rev` | Git repository (and branch or tag) to load `path` from, cloned once in `~/.cache/goat-hooker` |
| `parallel` | Root config only: `true` to run hooks in parallel |
| `branches` / `skip_branches` | Run the hook only on these branches, or not on these ones (globs: `main`, `release/*`) |
| `timeout` | Stop the hook (and what it started) after this time and fail it: `30s`, `2m`, `500ms` |

`*` doesn't cross folders, use `**` for that.

The `#:schema` line gives autocompletion and errors in TOML editors (Taplo, Even Better TOML for VS Code, Zed). In a `hooker.config.json`, the same URL goes in `"$schema"`. `hooker schema` prints the schema.

## Local changes

`hooker.local.toml` (or `hooker.local.json`), next to a `hooker.toml`, holds the changes of your computer only (add it to `.gitignore`; `hooker doctor` warns when it is committed). A hook with the name of an existing one changes its fields, `disabled = true` turns it off, another name adds a hook, and the other keys replace the ones of the config:

```toml
parallel = true

[[hooks]]
name = "test"
disabled = true

[[hooks]]
name = "clippy"
timeout = "2m"

[[hooks]]
name = "notes"
trigger = "post-commit"
cmd = "./my-notes.sh"
```

It also applies to the hooks loaded from a hooks file (`path`).
