# Cache

A hook that already passed with the same inputs isn't run again: its output, outputs and variables are replayed, and it shows as `cached`.

```console
$ git commit -m "fix: typo"
🐐 pre-commit · 2 hooks
✔ fmt     root  cached
✔ clippy  root  cached
```

It makes a retried commit instant: after a rejected commit message, or a failing hook you fixed elsewhere, the hooks that passed don't run again.

## What is compared

A result is reused when all of these are the same:

- the content of the targeted files (not their dates)
- the commands of the hook, and the arguments Git gave
- the values of its inputs and of the variables it reads

Anything else (an environment variable, a file the hook reads but doesn't target, the version of a tool) isn't compared: a hook depending on them should have `cache = false`, or target these files too.

## Which hooks

- Hooks with `include` and files to check are cached by default.
- `cache = true` caches any hook, `cache = false` never.
- Formatters (`stage_fixed`) are cached on the files once fixed: files that aren't formatted always go through them.
- Failed runs are never cached.

## Commands

```sh
hooker run pre-commit --no-cache   # run everything once
hooker cache-clear                 # forget every result
```

The results stay in `.git/goat-hooker/cache.json`, for your clone only. `hooker watch` fills the cache while you stage, so the commit finds everything ready (see [Running hooks](running.md#ahead-of-the-commit-watch)).
