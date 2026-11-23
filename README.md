![Goat Hooker is a open-source Git hooks manager](https://raw.githubusercontent.com/zuygui/goat-hooker/refs/heads/main/.github/assets/banner-light.svg#gh-light-mode-only)
![Goat Hooker is a open-source Git hooks manager](https://raw.githubusercontent.com/zuygui/goat-hooker/refs/heads/main/.github/assets/banner-dark.svg#gh-dark-mode-only)

# Goat Hooker

> Goat-Hooker is a free, open-source and easy-to-use Git Hooks Manager to enhance your projects.

> [!IMPORTANT] 
> The project still in development and is not ready to use in production.

## Features 🚀

- [x] Easy to write/read configuration files
- [x] Mono-repos support: each sub-project owns its config, with glob imports (`packages/*`) or automatic discovery
- [x] Shell commands support
- [x] All types of Git Hooks supported
- [x] Run `pre-commit` hooks only on the staged files they target, `pre-push` hooks only on the pushed files they target
- [x] Pass the targeted files to commands (`{files}` / `$HOOKER_FILES`)
- [x] Pre-commit hooks only see what is going to be committed (unstaged changes are put aside)
- [x] Re-stage files fixed by formatters (`stage_fixed`)
- [x] Run hooks in parallel
- [x] Shared hooks files, local or from a git repository (`path` / `from`)
- [x] Hooks graph: hooks and steps that wait for each other and pass values to each other
- [x] Variables shared between hooks, with who may write and read them
- [x] Permissions: read-only files, no network, filtered environment
- [x] Hooks from other repositories run only once trusted, again when they change
- [x] Ready-to-use hooks (Rust, JavaScript, Python, conventional commits, common checks) and a search of the hooks shared on GitHub
- [x] Import from husky / lint-staged, lefthook and pre-commit
- [x] JSON schema for autocompletion in editors
- [x] A desktop app for those who prefer not to use the CLI

## Installation 📦

**CLI** (`hooker`):

```sh
brew install zuygui/tap/goat-hooker        # macOS, Linux
curl -fsSL https://raw.githubusercontent.com/zuygui/goat-hooker/main/install.sh | sh
cargo binstall goat-hooker                  # prebuilt, with cargo-binstall
cargo install goat-hooker                   # from the sources
```

or download the archive of your platform from the [latest release](https://github.com/zuygui/goat-hooker/releases/latest).

**Desktop app** with Homebrew: `brew install --cask zuygui/tap/goat-hooker`. The app tells when a new version is out.

**Desktop app**: download the installer of your platform from the [latest release](https://github.com/zuygui/goat-hooker/releases/latest):

| Platform | Installer | CLI included |
|----------|-----------|--------------|
| macOS (Apple Silicon / Intel) | `.dmg` | `Goat Hooker.app/Contents/MacOS/hooker` |
| Windows | `-setup.exe` | `hooker.exe` next to the app |
| Linux | `.deb` / `.AppImage` | `/usr/bin/hooker` (`.deb`) |

> [!NOTE]
> The installers aren't signed yet: on macOS, right click on the app → Open the first time; on Windows, SmartScreen → More info → Run anyway.

From the sources:

```sh
cargo install --path .        # the `hooker` CLI
cargo install --path gui      # the `goat-hooker-gui` desktop app
```

On Linux, the desktop app needs `libxkbcommon`, `wayland`, `vulkan`, `fontconfig` and `xcb` development packages to build.

## Releasing 🚀

Releases are automatic, from the [conventional commits](https://www.conventionalcommits.org):

1. every push on `main` opens or updates a `chore: release` pull request, with the new version and the `CHANGELOG.md`
2. merging it publishes `goat-hooker` on crates.io, creates the `vX.Y.Z` tag and the GitHub release
3. the CLI archives and the app installers are built for every platform and added to the release

The repository needs a `CARGO_REGISTRY_TOKEN` secret (crates.io token with the `publish-new` and `publish-update` scopes), and GitHub Actions allowed to create pull requests (Settings → Actions → General).

The Homebrew tap (`zuygui/homebrew-tap`, with `Formula/` and `Casks/`) is updated by the release workflow when the `HOMEBREW_TAP_TOKEN` secret holds a token allowed to push to it.

## Usage 🐐

```sh
hooker             # status: Git hooks, projects, hooks of the next commit
hooker init        # create hooker.config.json
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

`hooker install` doesn't overwrite hooks it didn't install, use `--force` for that.

`HOOKER_SKIP=1 git commit` skips the hooks.

## Desktop app 🖥️

`goat-hooker-gui [repository]` opens a repository in a desktop app (built with [GPUI](https://www.gpui.rs) and [GPUI Kit](https://gpui-kit.com)):

- browse the hooks by trigger or by project of the mono-repo
- see which hooks will run on the next commit, and on which staged files
- run a hook, or the whole pre-commit, and read its output
- add, edit and delete hooks without touching the JSON
- install the Git hooks
- **Graph** view: the hooks of a trigger as connected nodes, like in Blender
  - nodes are laid out from the config (no positions to save): each node sits right of what it waits for
  - drag from an output to a node to pass it as an input (`inputs`), or to its *waits for* row to add an order dependency (`needs`)
  - `×` on a link removes it; cycles are refused
  - run the trigger and see each node's result and duration
  - edits are written to the `hooker.config.json` files, so the JSON stays the source of truth
  - double-click the background (or drop a wire on it) to create a hook, hover a node to delete it
  - variables are drawn from the hooks writing them to the hooks reading them
- **History**: the last runs and the slowest hooks
- the **Source** of a hook: its JSON in the config file, the scripts it runs, the hooks file it loads; **Edit the JSON** opens an editor with colors and autocompletion (keys, triggers, hooks, variables, outputs) that checks the file while you type
- the graph edits a node in a panel next to it, and its changes can be undone (⌘/Ctrl+Z)
- **Variables**: value, writers and readers of each variable
- **Browse hooks**: search the hooks shared on GitHub, set their parameters and add them
- **To review**: the commands and permissions of the hooks of other repositories, to trust them
- a repository without config offers to import husky, lint-staged, lefthook or pre-commit
- a menu bar (native on macOS, in the title bar on Linux and Windows) with shortcuts: ⌘/Ctrl+O open a repository, ⌘/Ctrl+N new hook, ⌘/Ctrl+R run the pre-commit, ⌘/Ctrl+Shift+R reload the configs, ⌘/Ctrl+1 / 2 list or graph, ⌘/Ctrl+Shift+D dark mode

## Contributing 🤝

goat-hooker runs its own hooks (see [`hooker.config.json`](hooker.config.json)): `rustfmt` and `clippy` on commit, the JSON schema regenerated when the config changes, a conventional commit message (release-plz builds the changelog from it) and the tests before a push.

```sh
cargo install --path . && hooker install
```

## Configuration ⚙️

`hooker.config.json` at the root of the repository:

```json
{
  "$schema": "https://raw.githubusercontent.com/zuygui/goat-hooker/main/schema.json",
  "imports": ["api", "front"],
  "hooks": [
    {
      "name": "fmt",
      "trigger": "pre-commit",
      "include": ["src/**/*.rs"],
      "exclude": ["src/generated/*.rs"],
      "cmd": "rustfmt --check {files}"
    },
    {
      "name": "commit message",
      "trigger": "commit-msg",
      "cmds": ["grep -qE '^(feat|fix|chore|docs|refactor|test|ci)' \"$1\""]
    }
  ]
}
```

| Field     | Description |
|-----------|-------------|
| `imports` | Sub-projects folders, each one with its own `hooker.config.json`. Globs are supported (`packages/*`) |
| `discover` | `true` to load every `hooker.config.json` of the repository (files ignored by git are skipped) |
| `name`    | Hook name |
| `trigger` | Git hook: `pre-commit`, `prepare-commit-msg`, `commit-msg`, `post-commit`, `pre-rebase`, `post-rewrite`, `pre-push`, `applypatch-msg`, `pre-applypatch`, `post-applypatch`, `pre-auto-gc`, `post-checkout`, `post-merge` |
| `include` | Glob patterns, relative to the config folder. Without it, the hook targets all files |
| `exclude` | Glob patterns of files to ignore |
| `cmd` / `cmds` | Shell command(s) to run, from the config folder. Git arguments are available as `$1`, `$2`... |
| `stage_fixed` | `true` to re-stage the targeted files after the hook (formatters) |
| `path` | Hooks file to load (`{ "hooks": [...] }`), relative to the config folder |
| `from` / `rev` | Git repository (and branch or tag) to load `path` from, cloned once in `~/.cache/goat-hooker` |
| `parallel` | Root config only: `true` to run hooks in parallel |

`*` doesn't cross folders, use `**` for that.

## Mono-repos 🗂️

Each sub-project keeps its own `hooker.config.json` next to its code:

```
my-repo/
├── hooker.config.json          { "imports": ["packages/*"] }
└── packages/
    ├── api/hooker.config.json
    └── front/hooker.config.json
```

Or let goat-hooker find them all with `{ "discover": true }`.

For each sub-project:

- `include` / `exclude` patterns are relative to its folder
- commands run from its folder
- `pre-commit` hooks only run if a staged file belongs to it, `pre-push` hooks only if a pushed commit changed one of its files
- `{files}` in a command is replaced by the targeted files (relative to its folder, shell-escaped), also available in `$HOOKER_FILES` (one per line)

```json
{ "name": "lint", "trigger": "pre-commit", "include": ["src/**/*.ts"], "cmd": "eslint {files}" }
```

Other triggers (`commit-msg`, `post-merge`...) run all their hooks.

## Hooks graph 🕸️

Hooks can wait for each other, and pass values to each other, like nodes connected by wires:

```json
{
  "hooks": [
    {
      "name": "codegen",
      "trigger": "pre-commit",
      "outputs": ["schema"],
      "cmd": "npm run codegen && echo \"schema=gen/schema.ts\" >> $HOOKER_OUTPUT"
    },
    {
      "name": "lint",
      "trigger": "pre-commit",
      "steps": [
        { "id": "eslint", "cmd": "eslint {files}" },
        { "id": "types", "needs": ["eslint"], "inputs": { "schema": "codegen.schema" }, "cmd": "tsc --noEmit {schema}" }
      ]
    },
    { "name": "e2e", "trigger": "pre-commit", "needs": ["lint", "packages/api:build"], "cmd": "npm run e2e" }
  ]
}
```

| Field | Description |
|-------|-------------|
| `needs` | Hooks to wait for: `hook`, `hook/step`, or `project:hook` for a hook of another project of the mono-repo (`root:hook` for the root). Same trigger only |
| `steps` | Steps of a hook (`id`, `cmd`, and their own `needs` / `inputs` / `outputs`). A step can wait for the other steps of its hook by their `id` |
| `outputs` | Values a hook writes with `echo "name=value" >> $HOOKER_OUTPUT` |
| `inputs` | Values from other hooks: `{ "name": "hook.output" }`, available as `{name}` (shell-escaped) and `$name` in the commands. Every hook also has the `files`, `stdout` and `status` outputs |

- every hook starts as soon as the hooks it waits for have passed (in parallel with `"parallel": true`)
- when a hook fails, the hooks waiting for it are skipped
- a hook a matching hook waits for runs too, even if none of the staged files match it
- `hooker run pre-commit --dry-run` shows the graph: order, dependencies, inputs and files

## Shared hooks 📦

Hooks can be shared between projects in a hooks file (`{ "hooks": [...] }`), loaded with `path`, from the repository or from another git repository:

```json
{
  "hooks": [
    { "name": "local rules", "path": "tools/hooks/rust.json" },
    { "name": "team rules", "from": "https://github.com/acme/hooks", "rev": "v1", "path": "rust.json" }
  ]
}
```

Loaded hooks run from the project that loads them. Repositories are cloned once: `hooker outdated` shows the ones with a newer version, `hooker update` fetches it (you review what changed with `hooker trust` before it runs), and `hooker pin` sets `rev` to the commit in use so it never changes. `rev` can be a branch, a tag or a commit.

A hooks file can declare `params`, used as `{{name}}` in its hooks, and set with `with` by the hook loading it. This makes reusable nodes:

```json
// eslint.json, in the repository acme/hooks
{
  "params": {
    "files": { "description": "Files to lint" },
    "config": { "default": ".eslintrc.json" }
  },
  "hooks": [
    { "name": "eslint", "trigger": "pre-commit", "include": ["{{files}}"], "cmd": "eslint -c {{config}} {files}" }
  ]
}
```

```json
{ "name": "lint", "from": "https://github.com/acme/hooks", "rev": "v1", "path": "eslint.json",
  "with": { "files": ["src/**/*.ts", "src/**/*.tsx"] } }
```

`{{name}}` (two braces) is replaced when the config is loaded; `{files}` and the inputs (one brace) when the hook runs. A string that is exactly `{{name}}` takes the value as is, so a list fills an array.

### Ready-to-use hooks

goat-hooker shares the hooks of [`hooks/`](hooks): `rust.json`, `javascript.json`, `python.json`, `commits.json` and `checks.json`.

```json
{ "name": "rust", "from": "https://github.com/zuygui/goat-hooker", "path": "hooks/rust.json",
  "with": { "clippy_args": "--all-targets" } }
```

### Share your hooks

Add `share` to the root `hooker.config.json` of a repository: `hooker search` (and the app) find it on GitHub.

```json
"share": { "description": "Hooks of the Acme team", "tags": ["go", "lint"], "hooks": ["hooks/*.json"] }
```

A hooks file can have a `description`. GitHub's code search needs a token: `GITHUB_TOKEN`, `GH_TOKEN` or `gh auth login`. `HOOKER_HUB_REPOS=owner/name,owner/other@branch` adds repositories the search can't find (private ones, branches).

## Affected projects 🧬

A hook with `"affected": true` also runs when a project its project depends on changes: change `core`, and the tests of `api` (which uses `core`) run too.

```json
{ "name": "test", "trigger": "pre-commit", "cmd": "cargo test", "include": ["**/*.rs"], "affected": true }
```

The dependencies are read from the manifests (Cargo `path` dependencies, packages of the repository in `package.json`, `replace` in `go.mod`, `path` dependencies in `pyproject.toml`), plus `depends_on` in a config (`"depends_on": ["../core"]`). `hooker run --dry-run` says why each hook runs.

## CI 🤖

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

## Cache ⚡

A hook that already passed with the same files (their content), commands, inputs and variables isn't run again: its output, outputs and variables are replayed. Hooks with `include` are cached by default, `"cache": true` / `false` chooses for a hook. Formatters are cached on the fixed files, so unformatted files always go through them. `hooker run --no-cache` runs everything, `hooker cache-clear` forgets the results.

## Variables 🔤

Variables are values shared between hooks, kept between commits (in `.git/goat-hooker`):

```json
{
  "variables": {
    "version": { "description": "Next version", "default": "0.1.0", "writers": ["bump"], "readers": ["*"] }
  },
  "hooks": [
    { "name": "bump", "trigger": "pre-commit", "writes": ["version"], "cmd": "echo version=0.2.0 >> $HOOKER_VARS" },
    { "name": "changelog", "trigger": "pre-commit", "reads": ["version"], "cmd": "echo \"## $version\" >> CHANGELOG.md" }
  ]
}
```

- a hook declares what it `writes` (`echo "name=value" >> $HOOKER_VARS`) and `reads` (`$name`)
- readers run after the writers of the same trigger
- `writers` and `readers` (`name`, `project:name`, `project:*` or `*`) say who may use it, any other hook is refused
- a hook writing a variable it didn't declare fails
- the values are saved when the whole run passes; `hooker vars --reset name` goes back to the default

## Permissions and trust 🔐

`permissions` says what a hook may do:

```json
{ "name": "license check", "trigger": "pre-commit", "cmd": "...",
  "permissions": { "files": "read", "network": false, "env": ["LICENSE_KEY"] } }
```

- `files`: `read` (the targeted files modified by the hook are reverted and the hook fails) or `write`
- `network`: blocked when false, with Linux user namespaces (`unshare`) or the macOS sandbox
- `env`: the environment variables it gets besides the basic ones (`PATH`, `HOME`, `GIT_*`...)

Without `permissions`, a hook of the repository can do everything. A hook loaded with `from` only reads files, without network and with the basic environment, unless its hooks file asks for more.

A hook without permission is **denied**: it doesn't run (not trusted, forbidden variable) or fails (it modified a read-only file, wrote an undeclared variable), and every hook after it, through `needs` or variables, is skipped. The other hooks still run, and the commit is stopped.

Hooks loaded with `from` don't run until they are trusted: `hooker trust` (or the app) shows their commands and permissions. They must be trusted again when their file or their `with` values change. `HOOKER_TRUST_ALL=1` skips the check (CI).

## Import 📥

`hooker import` converts the config of another hooks manager (`--dry-run` to only show it):

- husky (`.husky/*`, or `husky.hooks` in package.json) and its lint-staged config (one hook per command, in order)
- lefthook (`lefthook.yml`): commands, jobs, scripts, `root` becomes a sub-project, `piped` becomes `needs`
- pre-commit (`.pre-commit-config.yaml`): local hooks, the well-known hooks (pre-commit-hooks, ruff, black, prettier, eslint, Rust, conventional commits), the other ones through `pre-commit run`

What can't be converted exactly (regexes, `skip` conditions...) is listed.

## Thanks to all contributors 🧡
<p align="center">
  <img src="https://contrib.rocks/image?repo=zuygui/goat-hooker" />
</p>
