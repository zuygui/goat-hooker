# Writing commands

`cmd` is the command of a hook; `cmds` lists several, run in order until one fails:

```toml
[[hooks]]
name = "build"
trigger = "pre-push"
cmds = ["npm ci", "npm run build", "npm test"]
```

A hook with `steps` has a command per step instead (see [Hooks graph and variables](graph.md)).

## Where and how it runs

- From the folder of the config that declares the hook (the project of a mono-repo).
- A command made of plain words and placeholders (`cargo fmt -- {files}`) starts directly, without a shell: it is faster.
- Anything else (quotes, `$VAR`, `&&`, `|`, redirections, globs, `cd`...) runs with `sh -c`. On Windows, every command runs with the `sh` of Git for Windows.
- The hook fails when a command exits with a status other than 0.

## Placeholders

| Placeholder | Replaced with |
|-------------|---------------|
| `{files}` | The files the hook targets, relative to its folder, quoted for the shell (see [Choosing the files](files.md)) |
| `{name}` | The value of the input `name` (see [inputs](graph.md)) |
| `{{name}}` | A parameter of a shared hooks file, replaced when the config is loaded (see [Shared hooks](shared-hooks.md)) |

## Arguments and standard input

What Git gives to the hook is given to each command:

- arguments as `$1`, `$2`... (`commit-msg`: `$1` is the message file)
- standard input: the refs being pushed for `pre-push`, the rewritten commits for `post-rewrite`

```toml
[[hooks]]
name = "no WIP push"
trigger = "pre-push"
cmd = '''while read local_ref local_sha remote_ref remote_sha; do
  git log --format=%s "$remote_sha..$local_sha" 2>/dev/null | grep -q '^WIP' && { echo "WIP commits"; exit 1; }
done; exit 0'''
```

## Environment

Besides your environment, a command gets:

| Variable | Value |
|----------|-------|
| `HOOKER_FILES` | The targeted files, one per line |
| `HOOKER_OUTPUT` | File to write outputs to: `echo "name=value" >> $HOOKER_OUTPUT` |
| `HOOKER_VARS` | File to write variables to: `echo "name=value" >> $HOOKER_VARS` |
| `<input>` / `<variable>` | The value of each input and variable the hook reads |
| `FORCE_COLOR=1`, `CLICOLOR_FORCE=1` | When the output goes to a terminal: tools keep their colors |
| `GIT_*` | What Git sets for hooks (`GIT_INDEX_FILE`...) |

A hook with `permissions.env` only gets the variables it lists, plus the basic ones (`PATH`, `HOME`, `LANG`...; see [Security](security.md)).

## Output

The output of a hook (stdout and stderr, in order) is shown when it fails, or always with `hooker run -v`. It is kept in the history and in the app.

## Formatters

A formatter changes the files it gets: `stage_fixed = true` stages them again so that the fixes are committed.

```toml
[[hooks]]
name = "prettier"
trigger = "pre-commit"
include = ["**/*.ts", "**/*.tsx"]
cmd = "npx prettier --write {files}"
stage_fixed = true
```

`hooker fix` runs these hooks on your changed files without committing or staging, to fix them ahead.

## Timeout

`timeout = "30s"` stops the hook (and every process it started) after that time and fails it. Units: `ms`, `s`, `m`, `h`.

## Tips

- Prefer the tools of the project (`npx eslint`, `cargo clippy`, `uv run ruff`) to global ones: everyone gets the same version.
- Make slow hooks `pre-push` hooks, and keep `pre-commit` under a second or two: `hooker doctor` points out the slow ones from the history.
- Use `'single quotes'` in TOML for commands with backslashes or double quotes, and `'''triple quotes'''` for scripts on several lines.
