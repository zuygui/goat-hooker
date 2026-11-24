# Coming from another tool

Already using lefthook, pre-commit, prek or husky + lint-staged? Keep your config: Goat Hooker runs it as it is.

```sh
hooker install --force   # replaces the Git hooks of the other tool
git commit               # runs your lefthook.yml / .pre-commit-config.yaml / .husky
```

Nothing is written or converted, so teammates who still use the other tool aren't affected. When you want Goat Hooker's own features (cache, graph, sandbox, mono-repo configs), `hooker import` writes the same hooks as a `hooker.toml`.

## How it works

Without a `hooker.toml` (or `hooker.config.json`), Goat Hooker reads, in the root of the repository:

| Tool | Files |
|------|-------|
| lefthook | `lefthook.yml` (`.yaml`, `.json`, `.lefthook.yml`, `.config/lefthook.yml`), merged with `lefthook-local.yml` |
| pre-commit, prek | `.pre-commit-config.yaml` |
| husky | `.husky/<git hook>` scripts (husky 5 and later), or `husky.hooks` in `package.json` (husky 4) |
| lint-staged | `lint-staged` in `package.json`, `.lintstagedrc`, `.lintstagedrc.json`, `.lintstagedrc.yaml` |

The config is converted in memory on each run (it costs nothing measurable), then runs like a `hooker.toml`. A `hooker.toml` always wins.

```sh
hooker status    # Config  lefthook.yml · read as it is, nothing to migrate
hooker list      # the hooks, as Goat Hooker reads them
hooker doctor    # what can't be read exactly
```

## lefthook

| lefthook | Goat Hooker |
|----------|-------------|
| `commands`, `jobs`, `scripts` | one hook each |
| `run` with `{staged_files}`, `{push_files}`, `{files}` | `{files}` |
| `{0}`, `{1}`... | `"$@"`, `"$1"`... |
| `glob` | `include` (a pattern without `/` matches at any depth) |
| `exclude` (globs or a regex) | `exclude` / `exclude_regex` |
| `root` | the hook goes to the project of that folder |
| `parallel: true` | `parallel = true` |
| `piped: true` | each command `needs` the previous one |
| `stage_fixed` | `stage_fixed` |
| `env` | variables exported before the command |
| `fail_text` | shown when the command fails |
| `skip: true` | the command is left out |
| `skip` / `only` with `ref:` | `skip_branches` / `branches` |
| `lefthook-local.yml` | merged over the config |

Not converted (and listed by `hooker doctor`): `tags`, `files` (a custom command listing files), `interactive`, `use_stdin`, `skip` / `only` conditions other than branches (`merge`, `rebase`), job groups, `extends` and `remotes` (use [shared hooks](shared-hooks.md) instead).

## pre-commit and prek

Every hook runs with Goat Hooker itself, pre-commit isn't needed anymore:

- hooks of other repositories become [hooks of the pre-commit ecosystem](precommit-hooks.md) (`from` + `rev` + `id`), with their `args`, `additional_dependencies` and files, and are reviewed once with `hooker trust`
- `repo: local` hooks run their `entry`, with their language: `system` and `script` as they are, `fail` and `pygrep` too, the other languages with the installed tool first
- `files`, `exclude` (top-level ones too), `types`, `types_or`, `exclude_types`, `stages`, `default_stages`, `always_run`, `pass_filenames`, `args`

`repo: meta` hooks (they check the pre-commit config itself) are skipped.

Differences to know:

- the tools installed on your computer are used first; pre-commit always builds its own environments (`isolate = true` does the same)
- the files a hook fixes are staged again, where pre-commit fails and leaves them unstaged
- hooks of the same trigger run one after the other, like pre-commit, unless `parallel = true` in a `hooker.toml`

## husky and lint-staged

- each `.husky/<git hook>` script becomes a hook of that trigger
- a line running lint-staged is replaced by the lint-staged config: one hook per command, in order, on the files of its pattern, with the fixed files staged again (like lint-staged)
- `core.hooksPath` (set by husky): `hooker install` installs into it; to go back to `.git/hooks`, run `git config --unset core.hooksPath` first

A JavaScript lint-staged config (`lint-staged.config.js`) can't be read: write its commands in a `hooker.toml`.

## Switching for good

```sh
hooker import --dry-run   # show the converted hooks
hooker import             # write hooker.toml
hooker install --force
```

Then remove the old config and the old tool when the team is ready. What can't be converted exactly is listed.

| You used | Goat Hooker equivalent |
|----------|------------------------|
| `SKIP=ruff git commit` (pre-commit) | `HOOKER_SKIP=ruff git commit` |
| `LEFTHOOK=0 git commit`, `HUSKY=0 git commit` | `HOOKER_SKIP=1 git commit` |
| `pre-commit autoupdate` | `hooker outdated` lists the newer versions, then change `rev` (or `hooker update` for a branch) |
| `lefthook run pre-commit` | `hooker run pre-commit` |
| lefthook `remotes` | [shared hooks](shared-hooks.md) with `from` + `path` |
