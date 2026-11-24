# Local changes

`hooker.local.toml`, next to a `hooker.toml`, changes the config on your computer only: turn off a slow hook, give one a timeout, add your own. Add it to `.gitignore`; `hooker doctor` warns when one is committed.

```toml
# hooker.local.toml
parallel = true

[[hooks]]
name = "e2e"
disabled = true

[[hooks]]
name = "clippy"
timeout = "2m"

[[hooks]]
name = "notes"
trigger = "post-commit"
cmd = "./my-notes.sh"
```

- a hook with the name of an existing one changes the fields it sets
- `disabled = true` turns a hook off
- another name adds a hook
- the other keys (`parallel`...) replace the ones of the config

It applies to the hooks loaded from hooks files (`path`) and to the configs of other tools read as they are. `hooker.local.json` works too, and a project of a mono-repo can have its own next to its `hooker.toml`.

The desktop app writes this file when you turn a hook off "on this computer only".

## lefthook-local.yml

A lefthook config read as it is also gets its `lefthook-local.yml`, merged over it like lefthook does.
