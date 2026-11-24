# Security

Hooks run commands on your computer each time you commit. The hooks of your repository are yours; the hooks of other repositories (`from`) are someone else's code, so they are reviewed before they run and restricted while they run.

## Trust

A hook loaded from another repository (a hooks file with `from` + `path`, or a hook of the pre-commit ecosystem with `from` + `id`) runs only once you trusted it:

```console
$ hooker trust
📦 rust.json from https://github.com/acme/hooks @ v1
   pre-commit clippy read files · no network · basic env
     $ cargo clippy -- -D warnings
? Trust these hooks? (y/N)
```

- `hooker trust` shows the commands and permissions of each one, and asks.
- When it changes (a new `rev`, `hooker update`, other parameters), it must be trusted again, and `hooker trust` shows what changed.
- Until then, the hook fails with a message saying to run `hooker trust`, and `hooker doctor` lists it.
- `hooker trust --all` trusts everything without asking.
- `HOOKER_TRUST_ALL=1` skips the check: CI runs the code of the repository anyway (`hooker ci` sets it).

What you trusted is recorded in `.git/goat-hooker/trusted.json`, for your clone only. The app lists the hooks to review on its **To review** page.

## Permissions

```toml
[[hooks]]
name = "license check"
trigger = "pre-commit"
cmd = "./check-licenses"
permissions = { files = "read", network = false, env = ["LICENSE_KEY"] }
```

| Permission | Values | Default for a hook of the repository | Default for a hook of another repository |
|------------|--------|-----------------------------------|------------------------------------------|
| `files` | `"read"`, `"write"` | `write` | `read` (`write` for the hooks of the pre-commit ecosystem, which fix files) |
| `network` | `true`, `false` | `true` | `false` |
| `env` | list of variable names | every variable | the basic ones only |

- **`files = "read"`**: the targeted files are checked after the hook. If it changed one, the change is reverted and the hook fails. `stage_fixed` needs `write`.
- **`network = false`**: the hook runs without network where the system allows it: a Linux user namespace (`unshare`), or the macOS sandbox. Elsewhere it runs normally, and its output says the network couldn't be blocked.
- **`env`**: the hook only gets these variables, plus the basic ones: `PATH`, `HOME`, `USER`, `SHELL`, `TERM`, `LANG`, the temporary folders, the Windows system ones, and every `GIT_*` and `HOOKER_*` variable.

A hook of your repository can lower its own permissions. A hook of another repository gets more only when its own hooks file asks for them, and `hooker trust` shows them before you accept.

## Denied hooks

A hook without the permission it needs is **denied**: it doesn't run (not trusted, a variable it may not read) or fails (it changed a read-only file, wrote an undeclared variable). The hooks after it, through `needs` or variables, are skipped; the other hooks still run, and the commit is stopped.

## Variables

Shared variables say who may write and read them (`writers`, `readers`): any other hook is refused, and a hook writing a variable it didn't declare fails. See [Variables](graph.md#variables).

## What Goat Hooker itself does

- It runs the commands of your configs, and nothing else from the network: shared repositories are fetched with `git` into `~/.cache/goat-hooker` when a config needs them, and updated only with `hooker update`.
- Tools of the pre-commit ecosystem are built with the toolchains of your computer (pip, npm, cargo...), which download the packages the hook declares; no language runtime is downloaded.
- `hooker search` reads GitHub's API, with your GitHub token if you have one.
