# Environment and files

## Variables you set

| Variable | Effect |
|----------|--------|
| `HOOKER_SKIP` | `1` (or `true`, `all`): no hook runs. A list (`lint,test`, `api:test`, `lint/types`): these hooks don't run |
| `HOOKER_NOTIFY` | Desktop notification at the end of a long run started by Git: `0` turns it off, a number of seconds sets the minimum (10 by default) |
| `HOOKER_TRUST_ALL` | `1`: the hooks of other repositories run without being trusted (CI) |
| `HOOKER_CACHE_DIR` | Folder of the shared repositories and tool environments (default: `$XDG_CACHE_HOME/goat-hooker`, else `~/.cache/goat-hooker`) |
| `HOOKER_HUB_REPOS` | More repositories for `hooker search`: `owner/name,owner/other@branch` |
| `GITHUB_TOKEN`, `GH_TOKEN` | Token for `hooker search` (else the one of `gh auth`) |

## Variables the hooks get

| Variable | Value |
|----------|-------|
| `HOOKER_FILES` | The targeted files, one per line |
| `HOOKER_OUTPUT` | File where the hook writes its outputs (`echo "name=value" >> $HOOKER_OUTPUT`) |
| `HOOKER_VARS` | File where the hook writes variables (`echo "name=value" >> $HOOKER_VARS`) |
| `<name>` | Each input and each variable the hook reads |
| `FORCE_COLOR`, `CLICOLOR_FORCE` | `1` when the output goes to a terminal |
| `$1`, `$2`... | The arguments Git gave to the hook (see [Triggers](configuration.md#triggers)) |

The commands of Goat Hooker itself run git with `HOOKER_SKIP=1`, so that its own `git stash` or `git checkout` don't start hooks.

## Files in the repository

| File | Committed | What it is |
|------|-----------|------------|
| `hooker.toml` | yes | The config ([Configuration](configuration.md)) |
| `hooker.local.toml` | no | Your changes ([Local changes](local-changes.md)) |
| `.git/hooks/<trigger>` | no | Scripts calling `hooker run <trigger>`, written by `hooker install` |
| `.git/goat-hooker/cache.json` | no | Results of the hooks that passed ([Cache](cache.md)) |
| `.git/goat-hooker/history.jsonl` | no | Every run ([History](running.md#history)) |
| `.git/goat-hooker/trusted.json`, `trusted/` | no | Hooks of other repositories you trusted ([Trust](security.md#trust)) |
| `.git/goat-hooker/variables.json` | no | Values of the shared variables ([Variables](graph.md#variables)) |
| `.git/goat-hooker/synced` | no | What the Git hooks were installed for |

## Files on your computer

| Folder | What it is |
|--------|------------|
| `~/.cache/goat-hooker/repos` | Repositories of the shared hooks (`from`) |
| `~/.cache/goat-hooker/envs` | Tools of the [pre-commit ecosystem hooks](precommit-hooks.md) |

Both can be removed: they come back when needed.
