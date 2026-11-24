# Shared hooks

Hooks can be written once and used by several projects and repositories: in a hooks file, loaded with `path`, from your repository or from another Git repository.

```toml
[[hooks]]
name = "local rules"
path = "tools/hooks/rust.toml"

[[hooks]]
name = "team rules"
from = "https://github.com/acme/hooks"
rev = "v1"
path = "rust.toml"
```

A hooks file is a config with `hooks` (TOML or JSON). Its hooks:

- run from the project that loads them, on its files
- can be changed with [local changes](local-changes.md) by their name
- when they come from another repository, run only once you [trusted](security.md#trust) them, with restricted [permissions](security.md#permissions)

The tools of the pre-commit ecosystem have their own page: [Hooks of the pre-commit ecosystem](precommit-hooks.md).

## Repositories

`from` is a Git URL (HTTPS or SSH, anything `git clone` takes), `rev` a branch, a tag or a commit (the default branch without it). The repository is cloned once in `~/.cache/goat-hooker/repos` (`$HOOKER_CACHE_DIR` to put it elsewhere), and never fetched again by itself:

```sh
hooker outdated   # hooks whose repository has a newer version
hooker update     # fetch the last version of each `rev`
hooker trust      # review what changed before it runs
hooker pin        # set `rev` to the commit in use, so it never changes
hooker pin rust   # only the `rust` hook
```

## Parameters

A hooks file can declare `params`, used as `{{name}}` in its hooks and set with `with` by the hook loading it. It makes reusable hooks:

```toml
# eslint.toml, in the repository acme/hooks
description = "ESLint on the staged files"

[params]
files = { description = "Files to lint" }
config = { default = ".eslintrc.json" }

[[hooks]]
name = "eslint"
trigger = "pre-commit"
include = ["{{files}}"]
cmd = "eslint -c {{config}} {files}"
```

```toml
[[hooks]]
name = "lint"
from = "https://github.com/acme/hooks"
rev = "v1"
path = "eslint.toml"
with = { files = ["src/**/*.ts", "src/**/*.tsx"] }
```

- `{{name}}` (two braces) is replaced when the config is loaded; `{files}` and the inputs (one brace) when the hook runs
- a string that is exactly `{{name}}` takes the value as is, so a list fills an array
- a parameter without `default` must be set with `with`
- a hooks file can load other hooks files itself, relative to it

## The catalogue

Goat Hooker shares ready-to-use hooks in [`hooks/`](../hooks), the ones `hooker init` offers:

| File | Hooks |
|------|-------|
| `rust.json` | rustfmt, clippy, cargo test |
| `javascript.json` | prettier, eslint |
| `python.json` | ruff format, ruff check |
| `commits.json` | conventional commit messages |
| `checks.json` | conflict markers, large files, trailing whitespace, protected branch |

```toml
[[hooks]]
name = "rust"
from = "https://github.com/zuygui/goat-hooker"
path = "hooks/rust.json"
with = { clippy_args = "--all-targets" }
```

`hooker init -t rust,commits` starts a config with them, `hooker init -t auto` picks them from the files of the project (`Cargo.toml`, `package.json`, `pyproject.toml`...).

## Finding hooks

```sh
hooker search            # every shared hooks file
hooker search eslint     # by words
hooker search rust -p packages/api   # add to a project of the mono-repo
```

`hooker search` lists the hooks files shared on GitHub, shows their hooks and adds the one you pick to your config. The app does the same on its **Browse hooks** page.

GitHub's code search needs a token: `GITHUB_TOKEN`, `GH_TOKEN`, or the one of the GitHub CLI (`gh auth login`). Without it, only the catalogue is listed. `HOOKER_HUB_REPOS=owner/name,owner/other@branch` adds repositories the search can't find (private ones, branches).

## Share your hooks

Add `share` to the root `hooker.toml` of a repository, and `hooker search` finds it:

```toml
[share]
description = "Hooks of the Acme team"
tags = ["go", "lint"]
hooks = ["hooks/*.toml"]
```

Give each hooks file a `description`, `params` with descriptions, and tag releases: people pin a `rev`.
