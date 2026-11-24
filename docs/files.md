# Choosing the files

A `pre-commit` hook runs when one of the staged files it targets changed, and gets them in `{files}`. A `pre-push` hook does the same with the files changed by the pushed commits, and `hooker ci` with the files changed since the base of the pull request.

```toml
[[hooks]]
name = "eslint"
trigger = "pre-commit"
include = ["src/**/*.ts", "src/**/*.tsx"]
exclude = ["src/generated/**"]
cmd = "eslint --fix {files}"
stage_fixed = true
```

Deleted files are never given to hooks: there is nothing to check in them.

## Globs

`include` and `exclude` are lists of globs, relative to the folder of the config:

| Pattern | Matches |
|---------|---------|
| `*.rs` | `.rs` files of the folder itself (`*` doesn't cross folders) |
| `**/*.rs` | `.rs` files at any depth |
| `src/**` | everything under `src` |
| `src/*/mod.rs` | `mod.rs` one folder under `src` |
| `*.{js,ts}` | not supported: write `*.js` and `*.ts` |
| `[ab]*.md`, `?.txt` | one character among `a` and `b`, any single character |
| `Cargo.toml` | that file only |

- Without `include`, a hook targets every file.
- A file is kept when it matches one `include` pattern and no `exclude` pattern.
- An `include` that matches no file of the repository is reported by `hooker doctor`: the hook would never run.

## Regexes

`include_regex` and `exclude_regex` match the path of a file relative to the folder of the config, with `/` between folders, like `files` and `exclude` in pre-commit. The regex is searched in the path (use `^` and `$` to anchor it), and supports lookarounds:

```toml
[[hooks]]
name = "python tests"
trigger = "pre-commit"
include = ["**/*.py"]
include_regex = "^(src|tests)/"
exclude_regex = '(_pb2\.py|/migrations/)'
cmd = "pytest -q"
```

`include_regex` must match as well as `include` when a hook has both. Write regexes between single quotes in TOML: backslashes stay as they are.

## File kinds

`types` keeps the files of some kinds, all of them must match:

| Kind | Files |
|------|-------|
| `text` | without a NUL byte in their first 8 KB (like git) |
| `binary` | the others |
| `executable` / `non-executable` | with or without the executable bit (on Windows: `.exe`, `.bat`, `.cmd`) |
| `symlink` | symbolic links |
| `file` | regular files |

```toml
[[hooks]]
name = "trailing whitespace"
trigger = "pre-commit"
types = ["text"]
cmd = "sed -i -E 's/[[:blank:]]+$//' {files}"
stage_fixed = true
```

## Hooks without files

A hook of `pre-commit` or `pre-push` only runs when one of its files changed. To run it on every commit (a check of the branch, a whole-project build), use `always_run = true`; `{files}` is then empty when none of its files changed.

The hooks of the other triggers (`commit-msg`, `post-merge`...) always run and get no files.

## In the commands

- `{files}`: the targeted files, relative to the folder of the config, quoted for the shell (one argument each)
- `$HOOKER_FILES`: the same, one per line

```toml
cmd = "prettier --write {files}"
cmds = ['echo "$HOOKER_FILES" | xargs -n 50 mypy']
```

With many files, a command can be too long for the system: `xargs` splits them.

## What the hook sees

While the `pre-commit` hooks run, the changes you didn't stage are put aside: the files on disk are exactly what will be committed. They are put back afterwards, merged with what the hooks fixed. During a merge (`MERGE_HEAD`), nothing is put aside.

`stage_fixed = true` stages again the targeted files once the hook passed, so what a formatter fixed is part of the commit. Without it, the fixes stay in your working tree, unstaged.

## Checking

```sh
hooker run pre-commit --dry-run   # the hooks of the staged files, and their files
```

The desktop app's **Impact** page shows the hooks a file triggers.
