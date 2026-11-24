# Configuration

The hooks of a repository live in `hooker.toml`, at its root, committed with the code.

```toml
#:schema https://raw.githubusercontent.com/zuygui/goat-hooker/main/schema.json

parallel = true

[[hooks]]
name = "fmt"
trigger = "pre-commit"
include = ["src/**/*.rs"]
exclude = ["src/generated/**"]
cmd = "rustfmt {files}"
stage_fixed = true

[[hooks]]
name = "clippy"
trigger = "pre-commit"
include = ["**/*.rs", "Cargo.toml"]
cmd = "cargo clippy -- -D warnings"

[[hooks]]
name = "commit message"
trigger = "commit-msg"
cmd = "grep -qE '^(feat|fix|docs|chore)(\\(.+\\))?: ' \"$1\""
```

## The files

| File | What it is |
|------|------------|
| `hooker.toml` | The config, committed. `hooker init` creates it |
| `hooker.config.json` | The same in JSON: still read, `hooker.toml` wins when a folder has both |
| `hooker.local.toml` | Changes for your computer only, not committed (see [Local changes](local-changes.md)) |
| `lefthook.yml`, `.pre-commit-config.yaml`, `.husky/` | Without a hooker config, read as they are (see [Coming from another tool](migrating.md)) |

In a mono-repo, each project can have its own `hooker.toml`, loaded by the root one with `imports` (see [Mono-repos](monorepos.md)).

## A hook

A hook is a `[[hooks]]` table. Three fields make a working hook:

- `name`: how it is shown, and how other hooks and `HOOKER_SKIP` refer to it
- `trigger`: the Git hook that runs it (`pre-commit`, `commit-msg`, `pre-push`...; see [Triggers](#triggers))
- `cmd` (or `cmds`, several commands in order): what it runs, from the folder of the config (see [Writing commands](commands.md))

Then, optionally:

- **which files**: `include`, `exclude`, regexes, file kinds (see [Choosing the files](files.md))
- **what it changes**: `stage_fixed` stages again the files a formatter fixed
- **when**: `branches`, `skip_branches`, `timeout`, `cache` (see [Running hooks](running.md) and [Cache](cache.md))
- **with other hooks**: `needs`, `inputs`, `outputs`, `steps`, variables (see [Hooks graph and variables](graph.md))
- **from elsewhere**: `path`, `from`, `id` (see [Shared hooks](shared-hooks.md) and [Hooks of the pre-commit ecosystem](precommit-hooks.md))
- **what it may do**: `permissions` (see [Security](security.md))

Every field is in the [configuration reference](config-reference.md).

## The root keys

| Key | What it does |
|-----|--------------|
| `parallel` | `true` runs the hooks of a trigger in parallel (root config only). Hooks still wait for the ones they `needs` |
| `imports` | Folders of the sub-projects, each with its own `hooker.toml`. Globs work: `packages/*` |
| `discover` | `true` loads every `hooker.toml` of the repository (ignored files skipped) |
| `depends_on` | Projects this one depends on, besides the ones read from the manifests (see [Affected projects](monorepos.md#affected-projects)) |
| `variables` | Values shared between hooks (see [Variables](graph.md#variables)) |
| `share` | Shares the hooks files of the repository (see [Share your hooks](shared-hooks.md#share-your-hooks)) |

## Triggers

| Trigger | When | Files | Arguments (`$1`...) |
|---------|------|-------|---------------------|
| `pre-commit` | Before a commit is created | Staged files | — |
| `prepare-commit-msg` | Before the message editor opens | — | Message file, source, commit |
| `commit-msg` | To check the message | — | Message file |
| `post-commit` | After a commit | — | — |
| `pre-push` | Before a push | Files of the pushed commits | Remote name, URL (refs on stdin) |
| `pre-rebase` | Before a rebase | — | Upstream, branch |
| `post-rewrite` | After an amend or a rebase | — | `amend` or `rebase` (rewritten commits on stdin) |
| `post-checkout` | After a checkout or a switch | — | Previous HEAD, new HEAD, branch flag |
| `post-merge` | After a merge (and `git pull`) | — | Squash flag |
| `applypatch-msg`, `pre-applypatch`, `post-applypatch` | During `git am` | — | Message file (`applypatch-msg`) |
| `pre-auto-gc` | Before `git gc --auto` | — | — |

Only `pre-commit` and `pre-push` have files: their hooks run when one of their files changed. The hooks of the other triggers always run.

## Editor support

The `#:schema` line gives autocompletion, documentation on hover and errors in TOML editors: Taplo, Even Better TOML for VS Code, Zed, JetBrains IDEs. In a `hooker.config.json`, the same URL goes in `"$schema"`. `hooker schema` prints the schema.

## Checking a config

```sh
hooker list                      # every hook, as it is read (shared ones included)
hooker run pre-commit --dry-run  # what the next commit would run, on which files, and why
hooker doctor                    # what could go wrong, and how to fix it
```
