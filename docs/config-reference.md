# Configuration reference

Every field of `hooker.toml` (and `hooker.config.json`), generated from the [JSON schema](../schema.json). The [configuration guide](configuration.md) explains how they fit together.

## The config file

| Field | Type | Description |
|-------|------|-------------|
| `description` | string | What this hooks file does, shown when searching hooks |
| `share` | [share](#share) | Share hooks files of this repository: `hooker search` finds the root configs with this field on GitHub (root config only) |
| `params` | table of [param](#param) | Parameters of a reusable hooks file, used as `{{name}}` in its hooks and set with `with` by the hook loading it |
| `imports` | list of string | Sub-projects folders, each one with its own hooker.toml. Globs are supported (packages/*) |
| `discover` | boolean | Load every hooker config of the repository (files ignored by git are skipped) |
| `parallel` | boolean | Run hooks in parallel (root config only) |
| `depends_on` | list of string | Folders of the projects this one depends on (relative to it), besides the ones found in Cargo.toml, package.json, go.mod and pyproject.toml. Hooks with `affected` run when they change |
| `variables` | table of [variable](#variable) | Variables shared between hooks: a hook declares the ones it `writes` (`echo "name=value" >> $HOOKER_VARS`) and `reads` (`$name`). Their values are kept between runs |
| `hooks` | list of [hook](#hooks) | Hooks of this project |

## Hooks

Each `[[hooks]]` table.

### What it runs

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Hook name (the `id` by default for a hook of the pre-commit ecosystem) |
| `trigger` | [trigger](#triggers) | Git hook that runs this hook (not used with `path`) |
| `cmd` | string | Shell command, run from the config folder. {files} is replaced by the targeted files |
| `cmds` | list of string | Shell commands, run in order |
| `steps` | list of [step](#step) | Steps of this hook, each one can wait for the others |

### Which files

| Field | Type | Description |
|-------|------|-------------|
| `include` | list of string | Glob patterns, relative to the config folder. Without it, the hook targets all files |
| `exclude` | list of string | Glob patterns of files to ignore |
| `include_regex` | string | Regex the path of a file (relative to the config folder, with /) must contain, like `files` in pre-commit. Combined with `include` |
| `exclude_regex` | string | Regex of the paths of the files to ignore, like `exclude` in pre-commit |
| `types` | list of string | Kinds of files the hook targets, all of them must match: text, binary, executable, non-executable, symlink, file (like the file `types` of pre-commit) |
| `always_run` | boolean | Runs even when none of the files it targets changed (`always_run` in pre-commit) |
| `stage_fixed` | boolean | Re-stage the targeted files after the hook (for formatters) |

### When it runs

| Field | Type | Description |
|-------|------|-------------|
| `cache` | boolean | Skip the hook when it already passed with the same files, commands, inputs and variables. Default: true for hooks with `include` |
| `branches` | list of string | Run the hook only on these branches (globs: `main`, `release/*`) |
| `skip_branches` | list of string | Don't run the hook on these branches (globs: `wip/*`) |
| `affected` | boolean | Also run the hook when a project this one depends on changes (Cargo path dependencies, workspace packages, go.mod replace, `depends_on`) |
| `timeout` | string | Stop the hook after this time and fail it: `30s`, `2m`, `500ms` |

### Hooks working together

| Field | Type | Description |
|-------|------|-------------|
| `needs` | list of string | Hooks this one waits for: `name`, `name/step` or `project:name` (same trigger) |
| `inputs` | table of string | Values from other hooks, available as `{name}` and `$name` in commands: `{ "name": "hook.output" }` (outputs: files, stdout, status or a declared output) |
| `outputs` | list of string | Outputs this hook writes with `echo "name=value" >> $HOOKER_OUTPUT` |
| `reads` | list of string | Variables this hook reads, available as `$name` (it runs after the hooks writing them) |
| `writes` | list of string | Variables this hook writes with `echo "name=value" >> $HOOKER_VARS` |

### Hooks of other files and repositories

| Field | Type | Description |
|-------|------|-------------|
| `path` | string | Hooks file to load, relative to the config folder (or to the root of `from`) |
| `from` | string | Git repository to load a hooks file (`path`) or a hook of its .pre-commit-hooks.yaml (`id`) from |
| `rev` | string | Branch or tag of `from` (default branch if missing) |
| `with` | table | Values of the `params` of the hooks file loaded with `path` |
| `id` | string | Id of a hook of the `from` repository's .pre-commit-hooks.yaml: any hook of the pre-commit ecosystem (ruff, black, eslint, shellcheck...). Its command, files and tool come from there |
| `args` | list of string | Arguments of the `id` hook, replacing the ones of its definition |
| `additional_dependencies` | list of string | More packages installed with the tool of the `id` hook (pip, npm, gem, go or `cli:` cargo packages) |
| `isolate` | boolean | Tool of the `id` hook: `true` always builds it in its own environment (with the toolchain of the computer), `false` only uses the one installed. Default: the installed one, else its own environment |

### Permissions

| Field | Type | Description |
|-------|------|-------------|
| `permissions` | [permissions](#permissions) | What this hook is allowed to do. Without it, a hook of the repository can do everything and a hook loaded with `from` can only read files |

## Triggers

The Git hook that runs a hook: `pre-commit`, `prepare-commit-msg`, `commit-msg`, `post-commit`, `pre-rebase`, `post-rewrite`, `pre-push`, `applypatch-msg`, `pre-applypatch`, `post-applypatch`, `pre-auto-gc`, `post-checkout`, `post-merge`.

## Step

A step of a hook (`[[hooks.steps]]`), see [Hooks graph and variables](graph.md).

| Field | Type | Description |
|-------|------|-------------|
| `id` (required) | string | Step id, unique in the hook |
| `cmd` (required) | string | Shell command of the step |
| `needs` | list of string | Steps (or hooks) this one waits for |
| `inputs` | table of string | Values from other steps or hooks: `{ "name": "step.output" }` |
| `outputs` | list of string | Outputs this step writes to $HOOKER_OUTPUT |

## Permissions

What a hook is allowed to do. See [Security](security.md).

| Field | Type | Description |
|-------|------|-------------|
| `files` | `"read"` or `"write"` | `read`: the targeted files can't be modified (changes are reverted and the hook fails), `write`: they can |
| `network` | boolean | Network access. When false, it is blocked where the system allows it (Linux user namespaces, macOS sandbox) |
| `env` | list of string | Environment variables the hook gets, besides the basic ones (PATH, HOME, GIT_*...). Without it, a hook of the repository gets all of them |

## Variable

A variable of `[variables]`, see [Hooks graph and variables](graph.md#variables).

| Field | Type | Description |
|-------|------|-------------|
| `description` | string | What the variable holds |
| `default` | string | Value before any hook writes it |
| `writers` | list of string | Hooks allowed to write it: `name`, `project:name` or `*` (default: every hook) |
| `readers` | list of string | Hooks allowed to read it: `name`, `project:name` or `*` (default: every hook) |

## Param

A parameter of a reusable hooks file (`[params]`), see [Shared hooks](shared-hooks.md).

| Field | Type | Description |
|-------|------|-------------|
| `description` | string | What the parameter is for |
| `default` | any | Value used when `with` doesn't set it (required without it) |

## Share

The hooks a repository shares (`[share]`), found by `hooker search`.

| Field | Type | Description |
|-------|------|-------------|
| `description` | string | What these hooks are for |
| `tags` | list of string | Keywords to find them (rust, lint, commits...) |
| `hooks` (required) | list of string | Shared hooks files, relative to the repository root. Globs are supported (hooks/*.json) |

## Local changes

`hooker.local.toml` has the same fields as `hooker.toml`, plus `disabled = true` on a hook to turn it off. See [Local changes](local-changes.md).
