# FAQ and troubleshooting

## My hooks don't run

1. `hooker status`: are the Git hooks installed? If not, `hooker install`.
2. `hooker doctor`: it finds the usual causes. For example, `core.hooksPath` pointing elsewhere (husky sets it), another tool's hooks, or a pattern matching no file.
3. `hooker run pre-commit --dry-run`: which hooks the staged files trigger, and why the others don't run.

A `pre-commit` hook only runs when one of its files is staged. Check its `include`: `*.rs` only matches the files of the folder itself, `**/*.rs` those at any depth.

## `hooker: not found` in my editor or Git client

The Git hooks call `hooker` by name. Your editor or Git client doesn't see the same `PATH` as your terminal. Start it from a terminal, or add the folder of `hooker` to its `PATH`. `which hooker` tells you where it is.

## A hook passes in my terminal but fails on commit

During `pre-commit`, the files on disk are exactly what will be committed: your unstaged changes are put aside. A hook can fail because a fix is in your working tree but not staged. Stage it, or use `stage_fixed` for formatters.

The environment can also differ: the hooks get the environment of the program running Git. Use the tools of the project (`npx`, `cargo`, `uv run`) rather than global ones.

## How do I skip a hook once?

`HOOKER_SKIP=name git commit`, or `HOOKER_SKIP=1` for all of them. See [Skipping hooks](running.md#skipping-hooks).

## A hook is too slow for every commit

- Narrow its files with `include`, so it only runs when they change.
- Let the [cache](cache.md) skip it when its files didn't change (hooks with `include` are cached).
- `hooker watch` runs it while you stage.
- Move it to `pre-push`.
- `parallel = true` runs the hooks together.

`hooker doctor` lists the slow pre-commit hooks from the history.

## A hook "isn't trusted yet"

It comes from another repository: review it with `hooker trust`. In CI, `hooker ci` trusts everything. See [Security](security.md).

## A pre-commit ecosystem hook can't find its tool

Install the tool (it is then used as it is), or install the toolchain of its language so Goat Hooker can build it (Python, Node, Rust, Go, Ruby). Goat Hooker never downloads language runtimes. See [The tools](precommit-hooks.md#the-tools).

## A formatter changed my files but they weren't committed

Add `stage_fixed = true` to the hook: the files it fixed are staged again.

## The hooks fixed a file I was still editing

They didn't touch your unstaged changes: those were put aside and put back afterwards, merged with the fixes. If both changed the same lines, your version is kept and the fix is left out.

## Does it work on Windows?

Yes. The commands run with the `sh` of Git for Windows, found next to `git` when it isn't in the `PATH`.

## Can I keep using lefthook, pre-commit or husky?

Yes, their configs run as they are. See [Coming from another tool](migrating.md).

## Where is the data?

In `.git/goat-hooker` for each repository, and `~/.cache/goat-hooker` for the shared repositories and tools. See [Environment and files](environment.md).
