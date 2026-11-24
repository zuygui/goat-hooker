# Introduction

Goat Hooker is a Git hooks manager: it runs your formatters, linters, tests and checks when you commit or push, on the files that changed, for every project of your repository.

```toml
# hooker.toml
[[hooks]]
name = "fmt"
trigger = "pre-commit"
include = ["**/*.rs"]
cmd = "rustfmt {files}"
stage_fixed = true
```

```console
$ git commit -m "feat: faster"
🐐 pre-commit · 1 hook
✔ fmt  root  81ms

✔ 1 hook passed in 84ms
```

## What it does

- **Only what changed.** A hook runs on the staged files it targets, and sees exactly what will be committed: unstaged changes are put aside while it runs. A hook that already passed on the same files is taken from the cache.
- **Mono-repos.** Each project has its own config, its hooks run from its folder on its own files, and a change in a library also runs the hooks of the projects using it.
- **Fast.** One native binary that reads the repository itself instead of starting git, and hooks in parallel. A commit with one hook costs 9 ms on top of `git commit`.
- **Your existing setup.** It runs lefthook, pre-commit and husky + lint-staged configs as they are, and any hook of the pre-commit ecosystem.
- **Hooks that work together.** Hooks wait for each other and pass values, like nodes in a graph.
- **Safe with other people's hooks.** Hooks from other repositories run once you reviewed them, with read-only files, no network and a filtered environment.
- **A desktop app** to see, run, edit and wire the hooks.

## How it works

```
git commit ─▶ .git/hooks/pre-commit ─▶ hooker run pre-commit
                                          │
             hooker.toml (+ projects) ◀───┤ 1. loads the configs
             staged files ◀───────────────┤ 2. lists the changed files
                                          │ 3. picks the hooks whose files changed
                                          │ 4. puts the unstaged changes aside
                                          │ 5. runs the hooks (in order, in parallel)
                                          │ 6. stages what formatters fixed, restores the rest
                                          ▼
                              commit goes on, or stops on a failure
```

`hooker install` writes small scripts in `.git/hooks` that call `hooker run <trigger>`. Everything else is in your `hooker.toml`, committed with your code: everyone gets the same hooks.

## Where to go next

- [Quickstart](quickstart.md): hooks on your next commit, in a minute
- [Coming from another tool](migrating.md): lefthook, pre-commit, husky + lint-staged, prek
- [Configuration](configuration.md): how a config is organized
- [Configuration reference](config-reference.md) and [CLI reference](cli.md): every field and every command
- [FAQ](faq.md): when something doesn't go as expected
