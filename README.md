![Goat Hooker is a open-source Git hooks manager](https://raw.githubusercontent.com/zuygui/goat-hooker/refs/heads/main/.github/assets/banner-light.svg#gh-light-mode-only)
![Goat Hooker is a open-source Git hooks manager](https://raw.githubusercontent.com/zuygui/goat-hooker/refs/heads/main/.github/assets/banner-dark.svg#gh-dark-mode-only)

<p align="center">
  <b>Git hooks that only run what changed, for every project of your mono-repo, from the terminal or a desktop app.</b>
</p>

<p align="center">
  <a href="docs/README.md">Documentation</a> ·
  <a href="docs/installation.md">Install</a> ·
  <a href="CHANGELOG.md">Changelog</a> ·
  <a href="https://github.com/zuygui/goat-hooker/releases/latest">Latest release</a>
</p>

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/zuygui/goat-hooker/main/install.sh | sh
```

This installs the `hooker` CLI. It's also on Homebrew (`brew install zuygui/tap/goat-hooker`) and crates.io (`cargo binstall goat-hooker`). The desktop app ships as a `.dmg`, an `.exe` installer, a `.deb` and an `.AppImage` in the [latest release](https://github.com/zuygui/goat-hooker/releases/latest). More in the [installation guide](docs/installation.md).

## A taste of Goat Hooker

```sh
hooker init -t auto   # start from the hooks found for your project
hooker install        # plug them into Git
```

`hooker.config.json`:

```json
{
  "$schema": "https://raw.githubusercontent.com/zuygui/goat-hooker/main/schema.json",
  "hooks": [
    { "name": "fmt", "trigger": "pre-commit", "include": ["**/*.rs"], "cmd": "rustfmt {files}", "stage_fixed": true },
    { "name": "clippy", "trigger": "pre-commit", "include": ["**/*.rs"], "cmd": "cargo clippy -- -D warnings", "needs": ["fmt"], "cache": true },
    { "name": "test", "trigger": "pre-push", "include": ["**/*.rs"], "cmd": "cargo test" },
    { "name": "message", "trigger": "commit-msg", "cmd": "grep -qE '^(feat|fix|docs|chore)(\\(.+\\))?: ' \"$1\"" }
  ]
}
```

<p align="center">
  <img src=".github/assets/cli-light.svg#gh-light-mode-only" alt="Two git commits: the first runs fmt then clippy, the second takes both from the cache and stops on the commit message" />
  <img src=".github/assets/cli-dark.svg#gh-dark-mode-only" alt="Two git commits: the first runs fmt then clippy, the second takes both from the cache and stops on the commit message" />
</p>

`fmt` formats the staged Rust files and stages them again, and `clippy` waits for it. The second commit has the same files, so both come from the cache. Only the commit message fails, with a hint to skip just that hook.

## Why Goat Hooker?

Goats keep their footing on the steepest cliffs. Goat Hooker keeps your commits steady on the steepest mono-repos.

- **Only what changed.** Each hook runs on the staged files it targets and sees exactly what will be committed: unstaged changes are put aside. A hook whose files didn't change since it last passed is skipped.
- **Built for mono-repos.** Each project owns its `hooker.config.json`. Dependencies between projects are read from Cargo, `package.json`, `go.mod` and `pyproject.toml`, so a change in `core` also runs the tests of `api`.
- **Fast.** One native binary, hooks in parallel and a cache. `hooker watch` runs the hooks while you stage, so the commit itself is instant.
- **Hooks that work together.** Hooks wait for each other and pass values: `codegen` hands its schema to `client`. Shared variables record who may read and write them.
- **Safe with other people's hooks.** Hooks from other repositories run only once you reviewed them, and again only after you review their changes. They get read-only files, no network and a filtered environment unless they are allowed more.
- **A desktop app.** See what the next commit will run, edit hooks without touching the JSON, wire them like nodes in Blender, and read their output live.
- **Easy to adopt.**
  - `hooker import` converts husky / lint-staged, lefthook and pre-commit configs.
  - `hooker init` starts from ready-made hooks for Rust, JavaScript and Python.
  - `hooker doctor` says what is wrong and how to fix it.

## How it compares

<p align="center">
  <img src=".github/assets/features-light.svg#gh-light-mode-only" alt="Features of Goat Hooker, lefthook, pre-commit and husky + lint-staged" />
  <img src=".github/assets/features-dark.svg#gh-dark-mode-only" alt="Features of Goat Hooker, lefthook, pre-commit and husky + lint-staged" />
</p>

lefthook and pre-commit are great tools, and pre-commit still does something Goat Hooker doesn't: it installs the tools your hooks need. Spotted a mistake in this table? [Open an issue](https://github.com/zuygui/goat-hooker/issues/new).

## Speed

<p align="center">
  <img src=".github/assets/speed-light.svg#gh-light-mode-only" alt="Time of a git commit. One hook doing nothing: Goat Hooker 30 ms, lefthook 28 ms, pre-commit 146 ms, husky + lint-staged 395 ms. Five hooks of 200 ms: Goat Hooker 238 ms, lefthook 241 ms, pre-commit 1173 ms, husky + lint-staged 541 ms. The same commit again: Goat Hooker 24 ms, lefthook 233 ms, pre-commit 1176 ms, husky + lint-staged 567 ms" />
  <img src=".github/assets/speed-dark.svg#gh-dark-mode-only" alt="Time of a git commit. One hook doing nothing: Goat Hooker 30 ms, lefthook 28 ms, pre-commit 146 ms, husky + lint-staged 395 ms. Five hooks of 200 ms: Goat Hooker 238 ms, lefthook 241 ms, pre-commit 1173 ms, husky + lint-staged 541 ms. The same commit again: Goat Hooker 24 ms, lefthook 233 ms, pre-commit 1176 ms, husky + lint-staged 567 ms" />
</p>

- **The tool itself:** Goat Hooker adds about as much to a commit as lefthook, a fifth of what pre-commit adds, and a sixteenth of what husky + lint-staged adds through `npx`.
- **Five hooks:** they run in parallel, like with lefthook, where pre-commit runs them one after the other.
- **The same commit again**, after a rejected commit message for example: the hooks come from the cache, so the commit takes 24 ms instead of 233 ms or more.

These are small numbers on a 2-core VM. Method and caveats are in [`benchmarks/`](benchmarks/README.md), and you can reproduce them with `python3 benchmarks/run.py`.

## The desktop app

<p align="center">
  <img src=".github/assets/app-light.png#gh-light-mode-only" alt="The hooks of a mono-repo, with the staged files each one will check and its last run" />
  <img src=".github/assets/app-dark.png#gh-dark-mode-only" alt="The hooks of a mono-repo, with the staged files each one will check and its last run" />
</p>

<p align="center">
  <img src=".github/assets/graph-light.png#gh-light-mode-only" alt="The pre-commit hooks as a graph of nodes: secrets, lint steps, codegen passing its schema to client" />
  <img src=".github/assets/graph-dark.png#gh-dark-mode-only" alt="The pre-commit hooks as a graph of nodes: secrets, lint steps, codegen passing its schema to client" />
</p>

`goat-hooker-gui [repository]` opens a repository:
- **Hooks**: the hooks of the next commit and the files they will check. You can run a hook and follow its output live.
- **Graph**: wire the hooks together.
- **Impact**: what a file triggers.
- **Fix files**: review a formatter's changes before you stage them.
- **Doctor**: problems with the setup, and buttons to fix them.
- **Command palette**: ⌘/Ctrl+K reaches everything.

More in [the app's documentation](docs/desktop-app.md).

## How it works

```text
 git commit ─▶ .git/hooks/pre-commit ─▶ hooker run pre-commit
                                              │
   configs ─▶ staged files ─▶ Matcher ─▶ Graph ─▶ Runner ──────────▶ Git
   (projects,   (unstaged      (include,   (needs,   (parallel, cache,   (fixed files
    shared       changes        affected    inputs)   sandbox, timeout)   staged again,
    files,       put aside)     projects)                                 changes put back)
    local)
```

Each stage is its own module in [`src/`](src/): [`config`](src/config/mod.rs), [`matcher`](src/matcher.rs), [`graph`](src/graph.rs), [`runner`](src/runner.rs) and [`cache`](src/cache.rs). The desktop app in [`gui/`](gui/) uses the same library, so the CLI and the app always run hooks the same way.

## Documentation

- [Installation](docs/installation.md)
- [The `hooker` CLI](docs/cli.md): commands, skipping hooks, `watch`, `doctor`, CI, cache
- [Configuration](docs/configuration.md): every field, branches, timeout, local changes
- [Mono-repos](docs/monorepos.md) and [affected projects](docs/monorepos.md#affected-projects)
- [Hooks graph and variables](docs/graph.md)
- [Shared hooks](docs/shared-hooks.md): hooks files, the catalogue, permissions and trust, imports
- [Desktop app](docs/desktop-app.md)

## Building from source

```sh
cargo build --workspace         # the hooker CLI and the desktop app
cargo test --workspace
cargo run -p goat-hooker-gui    # the app
```

On Linux, the app needs the development packages of `libxkbcommon`, `wayland`, `vulkan`, `fontconfig` and `xcb`. Goat Hooker checks itself with its own hooks (`hooker.config.json`). See [CONTRIBUTING.md](CONTRIBUTING.md) and [how releases work](docs/releasing.md).

## Contributors

<p align="center">
  <img src="https://contrib.rocks/image?repo=zuygui/goat-hooker" />
</p>

MIT licensed.
