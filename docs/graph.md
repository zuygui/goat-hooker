# Hooks graph and variables

Hooks can wait for each other, and pass values to each other, like nodes connected by wires:

```toml
[[hooks]]
name = "codegen"
trigger = "pre-commit"
outputs = ["schema"]
cmd = 'npm run codegen && echo "schema=gen/schema.ts" >> $HOOKER_OUTPUT'

[[hooks]]
name = "lint"
trigger = "pre-commit"

[[hooks.steps]]
id = "eslint"
cmd = "eslint {files}"

[[hooks.steps]]
id = "types"
needs = ["eslint"]
inputs = { schema = "codegen.schema" }
cmd = "tsc --noEmit {schema}"

[[hooks]]
name = "e2e"
trigger = "pre-commit"
needs = ["lint", "packages/api:build"]
cmd = "npm run e2e"
```

| Field | Description |
|-------|-------------|
| `needs` | Hooks to wait for: `hook`, `hook/step`, or `project:hook` for a hook of another project of the mono-repo (`root:hook` for the root). Same trigger only |
| `steps` | Steps of a hook (`id`, `cmd`, and their own `needs` / `inputs` / `outputs`). A step can wait for the other steps of its hook by their `id` |
| `outputs` | Values a hook writes with `echo "name=value" >> $HOOKER_OUTPUT` |
| `inputs` | Values from other hooks: `{ "name": "hook.output" }`, available as `{name}` (shell-escaped) and `$name` in the commands. Every hook also has the `files`, `stdout` and `status` outputs |

- every hook starts as soon as the hooks it waits for have passed (in parallel with `parallel = true`)
- when a hook fails, the hooks waiting for it are skipped
- a hook a matching hook waits for runs too, even if none of the staged files match it
- `hooker run pre-commit --dry-run` shows the graph: order, dependencies, inputs and files

## Variables

Variables are values shared between hooks, kept between commits (in `.git/goat-hooker`):

```toml
[variables]
version = { description = "Next version", default = "0.1.0", writers = ["bump"], readers = ["*"] }

[[hooks]]
name = "bump"
trigger = "pre-commit"
writes = ["version"]
cmd = "echo version=0.2.0 >> $HOOKER_VARS"

[[hooks]]
name = "changelog"
trigger = "pre-commit"
reads = ["version"]
cmd = 'echo "## $version" >> CHANGELOG.md'
```

- a hook declares what it `writes` (`echo "name=value" >> $HOOKER_VARS`) and `reads` (`$name`)
- readers run after the writers of the same trigger
- `writers` and `readers` (`name`, `project:name`, `project:*` or `*`) say who may use it, any other hook is refused
- a hook writing a variable it didn't declare fails
- the values are saved when the whole run passes; `hooker vars --reset name` goes back to the default
