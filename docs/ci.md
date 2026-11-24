# Continuous integration

A hook can be skipped on a computer (`--no-verify`, `HOOKER_SKIP`, a teammate without `hooker install`). Running the same hooks in CI makes sure they passed.

`hooker ci` runs the hooks of a trigger on the files changed since a base commit:

```sh
hooker ci                       # pre-commit hooks, files changed since the base of the pull request
hooker ci --trigger pre-push    # another trigger
hooker ci --base origin/main    # another base
hooker ci -v                    # with the output of every hook
```

- The base is, by default, the base branch of the pull request (`GITHUB_BASE_REF` on GitHub), else `origin/HEAD`, else the previous commit. The history must be there: fetch it whole (`fetch-depth: 0`).
- Nothing is staged and nothing is put aside: a formatter that changes a file fails, the file should have been formatted before the commit.
- Hooks of other repositories are trusted: the CI runs the code of the repository anyway.
- It exits with an error when a hook fails.

## GitHub Actions

The action installs `hooker` from the releases, runs `hooker ci` and annotates the failed hooks:

```yaml
name: Hooks
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
          trigger: pre-commit   # default
          # base: origin/main   # default: the base of the pull request
          # version: 0.5.0      # default: latest
          # verbose: true
```

The tools of your hooks (Rust, Node, Python...) must be installed by the steps before, as for any build.

## Other CI services

Install `hooker` (see [Installation](installation.md)) and run it with the base branch:

```yaml
# GitLab CI
hooks:
  image: rust:latest
  script:
    - curl -fsSL https://raw.githubusercontent.com/zuygui/goat-hooker/main/install.sh | sh
    - git fetch origin "$CI_MERGE_REQUEST_TARGET_BRANCH_NAME"
    - hooker ci --base "origin/$CI_MERGE_REQUEST_TARGET_BRANCH_NAME"
  rules:
    - if: $CI_PIPELINE_SOURCE == "merge_request_event"
```

## Checking the setup

`hooker doctor` exits with an error when something is wrong (a pattern matching nothing, a tool missing): it can run in CI too.
