# Hooks of the pre-commit ecosystem

Hundreds of tools publish hooks for pre-commit: a `.pre-commit-hooks.yaml` at the root of their repository. Goat Hooker runs any of them with `from`, `rev` and `id`, without pre-commit:

```toml
[[hooks]]
from = "https://github.com/astral-sh/ruff-pre-commit"
rev = "v0.8.0"
id = "ruff"
args = ["--fix"]

[[hooks]]
from = "https://github.com/astral-sh/ruff-pre-commit"
rev = "v0.8.0"
id = "ruff-format"

[[hooks]]
from = "https://github.com/pre-commit/pre-commit-hooks"
rev = "v5.0.0"
id = "trailing-whitespace"

[[hooks]]
from = "https://github.com/pre-commit/pre-commit-hooks"
rev = "v5.0.0"
id = "check-yaml"
```

Then trust them once:

```sh
hooker trust
```

## What comes from the repository

The definition of the hook gives:

- its command: `entry`, then `args`, then the files (unless `pass_filenames: false`)
- its files: `types` / `types_or` (as globs and [file kinds](files.md#file-kinds)), `files`, `exclude`, `exclude_types`
- its trigger: the first of its `stages`, else `pre-commit`
- `always_run`

What the hook sets in your config wins: `args` replaces the arguments, `include` / `include_regex` / `exclude` replace the files, and `trigger`, `cmd`, `name` work as for any hook. The name is the `id` unless you give one.

These hooks may fix the files they target, and the fixes are staged again (`stage_fixed = false` to keep them out of the commit and fix them yourself).

## The tools

A hook needs its tool: `ruff`, `prettier`, `trailing-whitespace-fixer`...

1. **Installed on your computer**: when its program is in the `PATH`, it is used as it is. Nothing to build, nothing to download.
2. **Otherwise**, Goat Hooker builds it once in `~/.cache/goat-hooker/envs`, with the toolchain of your computer:

| `language` | Built with | Needs |
|------------|------------|-------|
| `python` | a virtualenv, with `uv` when it is installed (much faster), else `python3 -m venv` + `pip` | Python 3, or uv |
| `node` | `npm install --global --prefix` | Node.js and npm |
| `rust` | `cargo install` (and `cli:` dependencies) | Rust |
| `golang` | `go install` | Go |
| `ruby` | `gem build` + `gem install` | Ruby |
| `conda` | `mamba` or `conda create` | conda |
| `docker` | `docker build`, then `docker run` | Docker |
| `docker_image` | `docker run` | Docker |
| `system`, `script` | nothing: a program of your computer, or a script of the repository | — |
| `pygrep` | nothing: `hooker pygrep` looks for the regex | — |
| `fail` | nothing: fails with its message when it has files | — |

Goat Hooker never downloads a language runtime: when the toolchain is missing, the hook fails and says what to install. The other languages of pre-commit (`dotnet`, `swift`, `haskell`, `lua`, `perl`, `r`, `dart`, `julia`, `coursier`) aren't supported yet; write a `cmd` with the tool of your computer instead.

### Choosing

```toml
[[hooks]]
from = "https://github.com/pre-commit/mirrors-prettier"
rev = "v3.1.0"
id = "prettier"
isolate = true                       # always its own environment, at this rev
additional_dependencies = ["prettier-plugin-svelte@3.2.0"]
```

- `isolate = true`: always build the environment, to get exactly the version of `rev` (the installed tool may be another version).
- `isolate = false`: never build it; the hook fails when the tool isn't installed.
- `additional_dependencies`: more packages in the environment (pip, npm, gem or go packages, `cli:name:version` for cargo binaries). With dependencies, the environment is always built.

An environment is built once per repository, `rev`, language and dependencies, and shared by all your repositories. Several hooks building the same one wait for each other. To rebuild them all, remove `~/.cache/goat-hooker/envs`.

## Updating

```sh
hooker outdated   # hooks with a newer version
hooker update     # fetch the new version of `rev` (a branch), then review it with hooker trust
hooker pin        # set `rev` to the commit in use
```

## From a .pre-commit-config.yaml

A repository with a `.pre-commit-config.yaml` and no `hooker.toml` already runs this way, the config read as it is. `hooker import` writes it as a `hooker.toml` (see [Coming from another tool](migrating.md)).
