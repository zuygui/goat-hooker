# Shared hooks

Hooks can be shared between projects in a hooks file (`{ "hooks": [...] }`), loaded with `path`, from the repository or from another git repository:

```toml
[[hooks]]
name = "local rules"
path = "tools/hooks/rust.json"

[[hooks]]
name = "team rules"
from = "https://github.com/acme/hooks"
rev = "v1"
path = "rust.json"
```

Loaded hooks run from the project that loads them. Repositories are cloned once: `hooker outdated` shows the ones with a newer version, `hooker update` fetches it (you review what changed with `hooker trust` before it runs), and `hooker pin` sets `rev` to the commit in use so it never changes. `rev` can be a branch, a tag or a commit.

A hooks file can declare `params`, used as `{{name}}` in its hooks, and set with `with` by the hook loading it. This makes reusable nodes:

```toml
# eslint.toml, in the repository acme/hooks
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

`{{name}}` (two braces) is replaced when the config is loaded; `{files}` and the inputs (one brace) when the hook runs. A string that is exactly `{{name}}` takes the value as is, so a list fills an array.

## Ready-to-use hooks

goat-hooker shares the hooks of [`hooks/`](../hooks): `rust.json`, `javascript.json`, `python.json`, `commits.json` and `checks.json`.

```toml
[[hooks]]
name = "rust"
from = "https://github.com/zuygui/goat-hooker"
path = "hooks/rust.json"
with = { clippy_args = "--all-targets" }
```

## Share your hooks

Add `share` to the root `hooker.toml` of a repository: `hooker search` (and the app) find it on GitHub.

```toml
[share]
description = "Hooks of the Acme team"
tags = ["go", "lint"]
hooks = ["hooks/*.json"]
```

A hooks file can have a `description`. GitHub's code search needs a token: `GITHUB_TOKEN`, `GH_TOKEN` or `gh auth login`. `HOOKER_HUB_REPOS=owner/name,owner/other@branch` adds repositories the search can't find (private ones, branches).

## Permissions and trust

`permissions` says what a hook may do:

```toml
[[hooks]]
name = "license check"
trigger = "pre-commit"
cmd = "..."
permissions = { files = "read", network = false, env = ["LICENSE_KEY"] }
```

- `files`: `read` (the targeted files modified by the hook are reverted and the hook fails) or `write`
- `network`: blocked when false, with Linux user namespaces (`unshare`) or the macOS sandbox
- `env`: the environment variables it gets besides the basic ones (`PATH`, `HOME`, `GIT_*`...)

Without `permissions`, a hook of the repository can do everything. A hook loaded with `from` only reads files, without network and with the basic environment, unless its hooks file asks for more.

A hook without permission is **denied**: it doesn't run (not trusted, forbidden variable) or fails (it modified a read-only file, wrote an undeclared variable), and every hook after it, through `needs` or variables, is skipped. The other hooks still run, and the commit is stopped.

Hooks loaded with `from` don't run until they are trusted: `hooker trust` (or the app) shows their commands and permissions. They must be trusted again when their file or their `with` values change. `HOOKER_TRUST_ALL=1` skips the check (CI).

## Coming from husky, lefthook or pre-commit

Without a `hooker.toml`, Goat Hooker reads the config of the hooks manager you already use, as it is: nothing to migrate, and your teammates who still use that tool aren't affected.

```sh
hooker install --force   # replaces the Git hooks of the other tool
git commit               # runs the hooks of your lefthook.yml / .pre-commit-config.yaml / .husky
```

`hooker status` shows the config in use, and `hooker doctor` lists what can't be read exactly. A `hooker.toml` always wins over the other configs.

What is read:

- lefthook: commands, jobs, scripts, `glob`, `exclude` (globs or regex), `root`, `piped`, `parallel`, `env`, `fail_text`, `stage_fixed`, `skip` / `only` with `ref:`, and `lefthook-local.yml`
- pre-commit: local hooks (`language: fail` too), `files` / `exclude` regexes (top-level ones too), `types`, `stages`, `always_run`, `pass_filenames`, the well-known hooks (pre-commit-hooks, ruff, black, prettier, eslint, Rust, conventional commits) and the other ones through `pre-commit run`
- husky (`.husky/*`, or `husky.hooks` in package.json) and lint-staged (package.json or `.lintstagedrc`): one hook per command, in order, with the fixed files staged again

## Import from husky, lefthook and pre-commit

To switch for good, `hooker import` writes the same thing as a `hooker.toml` (`--dry-run` to only show it):

- husky (`.husky/*`, or `husky.hooks` in package.json) and its lint-staged config (one hook per command, in order)
- lefthook (`lefthook.yml`): commands, jobs, scripts, `root` becomes a sub-project, `piped` becomes `needs`
- pre-commit (`.pre-commit-config.yaml`): local hooks, the well-known hooks (pre-commit-hooks, ruff, black, prettier, eslint, Rust, conventional commits), the other ones through `pre-commit run`

What can't be converted exactly (regexes, `skip` conditions...) is listed.
