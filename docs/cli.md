# The `hooker` CLI

```sh
hooker             # status: Git hooks, projects, hooks of the next commit
hooker init        # create hooker.config.json (pick hooks to start with)
hooker init -t auto   # with the hooks found for the project (rust, javascript, python...)
hooker fix         # run the hooks fixing files on the changed files, without staging
hooker add         # add a hook (an assistant asks what's missing)
hooker install     # install the Git hooks in .git/hooks (or core.hooksPath)
hooker run pre-commit   # run manually the hooks of a trigger
hooker run pre-commit -v   # also show the output of the hooks that passed
hooker run pre-commit --dry-run   # show the hooks that would run, and on which files
hooker list        # list the configurations and their hooks
hooker update      # update the hooks loaded from git repositories
hooker search rust # find hooks shared on GitHub and add them
hooker ci          # run the hooks on the files changed by a pull request (CI)
hooker cache-clear # forget the results of the hooks that passed
hooker history     # last runs and slowest hooks
hooker history --export csv -o hooks.csv   # totals by hook, for the team
hooker trust       # review and trust the hooks loaded from other repositories
hooker outdated    # hooks of other repositories with a newer version
hooker pin         # pin them to the commit in use
hooker vars        # variables shared between hooks: value, writers, readers
hooker import      # convert a husky / lint-staged, lefthook or pre-commit config
hooker schema      # print the JSON schema of the configuration
hooker completion zsh > ~/.zfunc/_hooker   # shell completion
```

`hooker add` also works without a terminal:

```sh
hooker add -n "ESLint" -t pre-commit -i "src/**/*.ts" -c "eslint {files}" -p packages/web
```

Hooks show a spinner while running, then a summary. Only the output of the failed hooks is shown (all of them with `-v`).

`hooker install` installs the Git hooks of the triggers your configs use, plus pre-commit, which adds or removes the others when the configs change. It doesn't overwrite hooks it didn't install, use `--force` for that.

`HOOKER_SKIP=1 git commit` skips the hooks, `HOOKER_SKIP=lint,test git commit` only these ones (by name, `project:name` or `hook/step`); what waits for them still runs.

A run started by Git that takes 10 seconds or more ends with a desktop notification (a long pre-push, say). `HOOKER_NOTIFY=0` turns it off, `HOOKER_NOTIFY=60` only notifies after a minute.

## Watch

`hooker watch` runs the pre-commit hooks each time files are staged, so `git commit` takes their results from the cache and finishes at once (and you know a hook fails before you commit):

```
$ hooker watch
👀 Watching the staged files · 2 hooks: clippy, test
✔ clippy 5.61s  ✔ test 8.02s  ready to commit
```

Only the hooks that the commit would take from the cache run ahead: cached, with `include`, not fixing files (`stage_fixed`), without `needs`, `inputs` or variables. They run on the files as they are, so a staged file that also has unstaged changes is waited for. Nothing is staged or put aside.

## Doctor

`hooker doctor` checks the setup and says how to fix what it finds:

- Git hooks not installed, overwritten by hand, or bypassed by `core.hooksPath`
- tools the commands use that aren't installed
- `include` patterns that match no file (the hook never runs)
- variables nobody reads or writes, and denied accesses
- hooks of other repositories that aren't trusted
- from the history: slow pre-commit hooks (cache them or move them to pre-push) and hooks that fail most of the time

It exits with an error when it finds one, so it can run in CI. The app shows the same checks on its **Doctor** page.

## CI

`hooker ci` runs the hooks of a trigger (`--trigger`, `pre-commit` by default) on the files changed since a base commit (`--base`, by default the base of the pull request or `origin/HEAD`). Nothing is staged: a formatter that changes a file fails, the file should have been fixed before the commit. Hooks of other repositories are trusted, since the CI runs the code of the repository anyway.

On GitHub, use the action (it installs `hooker` from the releases and annotates the failed hooks):

```yaml
on: [pull_request, push]
jobs:
  hooks:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: zuygui/goat-hooker@main
        with:
          trigger: pre-commit # default
```

## Cache

A hook that already passed with the same files (their content), commands, inputs and variables isn't run again: its output, outputs and variables are replayed. Hooks with `include` are cached by default, `"cache": true` / `false` chooses for a hook. Formatters are cached on the fixed files, so unformatted files always go through them. `hooker run --no-cache` runs everything, `hooker cache-clear` forgets the results.
