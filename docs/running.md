# Running hooks

Once `hooker install` is done, Git runs the hooks by itself: `git commit` runs the `pre-commit` and `commit-msg` hooks, `git push` the `pre-push` ones, and so on.

```console
$ git commit -m "feat: numbers"
🐐 pre-commit · 3 hooks
✔ fmt     root  81ms
✔ clippy  root  cached
✘ test    api   2.41s

✘ test · api
  $ cargo test
  │ test add ... FAILED

✘ 1 failed, 2 passed in 2.52s
  hint: fix the errors above, or skip them once with `HOOKER_SKIP=test git ...`
```

A failing hook stops the commit (or the push). Only the output of the failed hooks is shown.

## Installing the Git hooks

```sh
hooker install           # the Git hooks of the triggers your configs use
hooker install --force   # also replace the hooks of another tool
```

It writes small scripts in `.git/hooks` (or in the folder of `core.hooksPath`) that call `hooker run <trigger>`. When your configs start using a new trigger, the next commit adds its Git hook. Hooks written by hand, or by another tool, are kept unless `--force`.

## Running by hand

```sh
hooker run pre-commit               # the hooks of the staged files
hooker run pre-commit -v            # with the output of every hook
hooker run pre-commit --dry-run     # what would run, on which files, and why
hooker run pre-commit --no-cache    # without the cache
hooker run commit-msg -- .git/COMMIT_EDITMSG   # arguments of the Git hook after --
```

`hooker` alone shows the status of the repository and what the next commit will run.

## Skipping hooks

```sh
HOOKER_SKIP=1 git commit            # no hook at all
HOOKER_SKIP=test,lint git commit    # these hooks only
HOOKER_SKIP=api:test git commit     # the test hook of the api project
HOOKER_SKIP=lint/types git commit   # one step of a hook
git commit --no-verify              # Git's own way, skips every hook
```

The hooks waiting for a skipped hook still run. To turn a hook off on your computer for good, use [local changes](local-changes.md).

## Branches

```toml
[[hooks]]
name = "changelog"
trigger = "pre-push"
branches = ["main", "release/*"]
cmd = "./check-changelog.sh"

[[hooks]]
name = "e2e"
trigger = "pre-push"
skip_branches = ["wip/*"]
cmd = "npm run e2e"
```

`branches` runs a hook only on these branches, `skip_branches` never on these. On a detached HEAD, hooks with `branches` don't run.

## Parallel hooks

`parallel = true` at the root of the config runs the hooks of a trigger at the same time. Hooks that `needs` others wait for them; hooks writing the same files should wait for each other.

## Ahead of the commit: watch

`hooker watch` runs the pre-commit hooks each time files are staged, so `git commit` takes their results from the cache and finishes at once:

```console
$ hooker watch
👀 Watching the staged files · 2 hooks: clippy, test
✔ clippy 5.61s  ✔ test 8.02s  ready to commit
```

Only the hooks the commit would take from the cache run ahead: cached, with `include`, not fixing files (`stage_fixed`), without `needs`, `inputs` or variables. They run on the files as they are, so a staged file that also has unstaged changes is waited for. Nothing is staged or put aside.

## Fixing files: fix

`hooker fix` runs the hooks that fix files (`stage_fixed`) on the files changed since the last commit, or on the files you give, without staging anything:

```sh
hooker fix                 # the changed files
hooker fix src/main.rs     # these files
```

## Notifications

A run started by Git that takes 10 seconds or more ends with a desktop notification, so you can do something else during a long `pre-push`. `HOOKER_NOTIFY=0` turns it off, `HOOKER_NOTIFY=60` only notifies after a minute.

## History

```sh
hooker history                         # the last runs and the slowest hooks
hooker history -n 100
hooker history --export csv -o hooks.csv   # totals by hook, to share with the team
```

The history stays in `.git/goat-hooker/history.jsonl`. The app shows it on its **History** page.

## Doctor

`hooker doctor` checks the setup and says how to fix what it finds:

- Git hooks not installed, overwritten, or bypassed by `core.hooksPath`
- tools the commands use that aren't installed
- `include` patterns that match no file
- variables nobody reads or writes, and denied accesses
- hooks of other repositories that aren't trusted yet
- the parts of a lefthook, pre-commit or husky config that can't be read exactly
- from the history: slow pre-commit hooks, and hooks that fail most of the time

It exits with an error when something is wrong, so it can run in CI.
