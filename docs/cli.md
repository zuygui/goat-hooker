# CLI reference

```sh
hooker [COMMAND]
```

Without a command, `hooker` shows the status of the repository (like `hooker status`). Every command works from any folder of the repository.

## Setting up

### `hooker init`

Creates `hooker.toml`, starting from hooks of the [catalogue](shared-hooks.md#the-catalogue).

| Option | |
|--------|-|
| `-t, --template <NAMES>` | `rust`, `javascript`, `python`, `checks`, `commits`, separated by commas, or `auto` (picked from the files of the project). Asked in a terminal when missing |
| `-w <DIR>` | Folder of the config (default: the current one) |

### `hooker install`

Installs the Git hooks of the triggers the configs use, plus `pre-commit` (which adds or removes the others when the configs change), in `.git/hooks` or the folder of `core.hooksPath`.

| Option | |
|--------|-|
| `-f, --force` | Also replace the hooks Goat Hooker didn't install (another tool's, hand-written ones) |

### `hooker import`

Converts the configs of husky + lint-staged, lefthook and pre-commit to `hooker.toml` (see [Coming from another tool](migrating.md)).

| Option | |
|--------|-|
| `--dry-run` | Show the converted hooks without writing them |
| `-f, --force` | Replace the existing hooker configs |

### `hooker add`

Adds a hook. In a terminal, asks what's missing; without one, takes everything from the options:

```sh
hooker add -n "ESLint" -t pre-commit -i "src/**/*.ts" -c "eslint {files}" -p packages/web
```

| Option | |
|--------|-|
| `-n, --name <NAME>` | Hook name |
| `-t, --trigger <TRIGGER>` | Git hook that runs it |
| `-c, --cmd <CMD>` | Command (repeat for several) |
| `-i, --include <GLOB>` | Files (repeat, or separate with commas) |
| `-e, --exclude <GLOB>` | Ignored files |
| `--stage-fixed` | Stage the files again after the hook |
| `-p, --project <DIR>` | Project of the mono-repo (default: the one of the current folder) |

## Running

### `hooker run <TRIGGER> [-- ARGS...]`

Runs the hooks of a trigger, like Git does. `ARGS` are the arguments Git would give (`$1`, `$2`...).

| Option | |
|--------|-|
| `--dry-run` | Show the hooks that would run, their files, their order and why, without running them |
| `-v, --verbose` | Show the output of every hook, not only the failed ones |
| `--no-cache` | Run every hook, even those that already passed with the same files |
| `-c <FILE>` | Another config file |

### `hooker watch`

Runs the cached pre-commit hooks each time files are staged, so the commit takes their results at once. See [Running hooks](running.md#ahead-of-the-commit-watch).

### `hooker fix [FILES...]`

Runs the hooks fixing files (`stage_fixed`) on the files changed since the last commit, or on `FILES`, without staging them.

| Option | |
|--------|-|
| `-v, --verbose` | Show the output of every hook |

### `hooker ci`

Runs the hooks on the files changed by a pull request or a push. See [Continuous integration](ci.md).

| Option | |
|--------|-|
| `-t, --trigger <TRIGGER>` | Trigger whose hooks run (default: `pre-commit`) |
| `--base <REF>` | Commit or branch to compare with (default: the base of the pull request, or `origin/HEAD`) |
| `-v, --verbose` | Show the output of every hook |

## Looking

### `hooker status`

The Git hooks, the config in use, the projects and their hooks, and what the next commit will run.

### `hooker list`

Every config and its hooks, as they are read: shared hooks and other tools' configs included.

| Option | |
|--------|-|
| `-c <FILE>` | Another config file |

### `hooker doctor`

Checks the setup and says how to fix what it finds; exits with an error when something is wrong. See [Running hooks](running.md#doctor).

### `hooker history`

The last runs and the slowest hooks.

| Option | |
|--------|-|
| `-n, --limit <N>` | Number of runs (default: 20) |
| `--export <csv\|json>` | Totals by hook, for the team (every run) |
| `-o, --output <FILE>` | File to write the export to (default: the standard output) |

### `hooker vars`

The variables shared between hooks: value, writers, readers.

| Option | |
|--------|-|
| `--reset <NAME>` | Forget the value of a variable (back to its default) |

## Shared hooks

### `hooker search [WORDS...]`

Searches the hooks shared on GitHub and adds the one you pick. See [Finding hooks](shared-hooks.md#finding-hooks).

| Option | |
|--------|-|
| `-p, --project <DIR>` | Project to add them to (default: the root) |

### `hooker trust`

Shows the hooks of other repositories that aren't trusted yet (or changed), their commands and permissions, and asks. See [Trust](security.md#trust).

| Option | |
|--------|-|
| `--all` | Trust them all without asking |

### `hooker outdated`

The hooks of other repositories that have a newer version.

### `hooker update`

Fetches the last version of the repositories of the shared hooks (`from` + `rev`). Review the changes with `hooker trust`.

| Option | |
|--------|-|
| `-c <FILE>` | Another config file |

### `hooker pin [NAME]`

Sets `rev` to the commit in use, for every hook loading another repository, or only `NAME`.

## Other commands

### `hooker cache-clear`

Forgets the results of the hooks that passed: they all run again. See [Cache](cache.md).

### `hooker schema`

Prints the JSON schema of the configuration, used by editors for autocompletion.

### `hooker completion [SHELL]`

Prints the shell completion: `bash` (default), `zsh`, `fish`, `elvish`, `powershell`. See [Installation](installation.md#shell-completion).

## Exit status

`0` when every hook passed (or there was nothing to run), `1` when a hook failed or was denied, or on an error (an invalid config, a missing repository), `130` when a question was cancelled. Git stops the commit or the push on anything else than `0`.
