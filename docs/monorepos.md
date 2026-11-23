# Mono-repos

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

## Affected projects

A hook with `"affected": true` also runs when a project its project depends on changes: change `core`, and the tests of `api` (which uses `core`) run too.

```json
{ "name": "test", "trigger": "pre-commit", "cmd": "cargo test", "include": ["**/*.rs"], "affected": true }
```

The dependencies are read from the manifests (Cargo `path` dependencies, packages of the repository in `package.json`, `replace` in `go.mod`, `path` dependencies in `pyproject.toml`), plus `depends_on` in a config (`"depends_on": ["../core"]`). `hooker run --dry-run` says why each hook runs.
